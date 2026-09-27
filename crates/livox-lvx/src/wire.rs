//! Fixed-size Serde schemas. No variable-length allocation is delegated to Serde.
use crate::LvxError;
use serde::de::DeserializeOwned;
pub(crate) fn decode<T: DeserializeOwned>(input: &[u8]) -> Result<T, LvxError> {
    let config = bincode::config::standard()
        .with_fixed_int_encoding()
        .with_little_endian()
        .with_limit::<256>();
    let (value, used) =
        bincode::serde::decode_from_slice(input, config).map_err(|_| LvxError::WireDecode)?;
    if used != input.len() {
        return Err(LvxError::WireDecode);
    }
    Ok(value)
}
#[derive(serde::Deserialize)]
pub(crate) struct Public {
    pub signature: [u8; 16],
    pub version: [u8; 4],
    pub magic: u32,
}
#[derive(serde::Deserialize)]
pub(crate) struct Private {
    pub frame_duration: u32,
    pub device_count: u8,
}
#[derive(serde::Deserialize)]
pub(crate) struct Frame {
    pub current: u64,
    pub next: u64,
    pub index: u64,
}
#[derive(serde::Deserialize)]
pub(crate) struct Device1 {
    pub lidar_serial: [u8; 16],
    pub hub_serial: [u8; 16],
    pub index: u8,
    pub device_type: u8,
    pub extrinsic_enable: u8,
    pub rotation: [f32; 3],
    pub translation: [f32; 3],
}
#[derive(serde::Deserialize)]
pub(crate) struct Device2 {
    pub lidar_serial: [u8; 16],
    pub hub_serial: [u8; 16],
    pub lidar_id: u32,
    pub lidar_type: u8,
    pub device_type: u8,
    pub extrinsic_enable: u8,
    pub rotation: [f32; 3],
    pub translation: [f32; 3],
}
#[derive(serde::Deserialize)]
pub(crate) struct Packet2 {
    pub version: u8,
    pub lidar_id: u32,
    pub lidar_type: u8,
    pub time_type: u8,
    pub timestamp: u64,
    pub udp_counter: u16,
    pub data_type: u8,
    pub length: u32,
    pub frame_counter: u8,
    pub reserved: [u8; 4],
}
