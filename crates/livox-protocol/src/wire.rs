//! Serde decoding restricted to fixed-width wire structures, never public enums.
use crate::DecodeError;
use serde::de::DeserializeOwned;
pub(crate) fn decode<T: DeserializeOwned>(input: &[u8]) -> Result<T, DecodeError> {
    let config = bincode::config::standard()
        .with_fixed_int_encoding()
        .with_little_endian()
        .with_limit::<256>();
    let (value, used) =
        bincode::serde::decode_from_slice(input, config).map_err(|_| DecodeError::WireDecode)?;
    if used != input.len() {
        return Err(DecodeError::InvalidLength {
            declared: used,
            actual: input.len(),
        });
    }
    Ok(value)
}
#[derive(serde::Deserialize)]
pub(crate) struct Command {
    pub sof: u8,
    pub version: u8,
    pub length: u16,
    pub seq_num: u32,
    pub cmd_id: u16,
    pub cmd_type: u8,
    pub sender_type: u8,
    pub reserved: [u8; 6],
    pub crc16: u16,
    pub crc32: u32,
}
#[derive(serde::Deserialize, serde::Serialize)]
pub(crate) struct Data {
    pub version: u8,
    pub length: u16,
    pub time_interval: u16,
    pub dot_num: u16,
    pub udp_cnt: u16,
    pub frame_cnt: u8,
    pub data_type: u8,
    pub time_type: u8,
    pub reserved: [u8; 12],
    pub crc32: u32,
    pub timestamp: u64,
}
#[derive(serde::Deserialize)]
pub(crate) struct Legacy {
    pub version: u8,
    pub slot_id: u8,
    pub lidar_id: u8,
    pub reserved: u8,
    pub status: u32,
    pub timestamp_type: u8,
    pub data_type: u8,
    pub timestamp: [u8; 8],
}
