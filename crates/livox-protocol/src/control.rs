//! Mid-360 discovery and key/value command payloads (not UDP framing).
use crate::{DecodeError, wire};
use serde::{Deserialize, Serialize};
pub const DISCOVERY: u16 = 0;
pub const SET_PARAMETERS: u16 = 0x100;
pub const GET_PARAMETERS: u16 = 0x101;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveryResponse {
    pub ret_code: u8,
    pub device_type: u8,
    pub serial: [u8; 16],
    pub ip: [u8; 4],
    pub command_port: u16,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostEndpoint {
    pub ip: [u8; 4],
    pub destination_port: u16,
    pub source_port: u16,
}
#[derive(Deserialize)]
struct ListHeader {
    count: u16,
    _reserved: u16,
}
#[derive(Deserialize)]
struct ValueHeader {
    key: u16,
    length: u16,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyValue<'a> {
    pub key: u16,
    pub value: &'a [u8],
}
fn take(input: &mut &[u8], n: usize) -> Result<Vec<u8>, DecodeError> {
    let b = input.get(..n).ok_or(DecodeError::TooShort {
        expected: n,
        actual: input.len(),
    })?;
    let out = b.to_vec();
    *input = &input[n..];
    Ok(out)
}
/// Decode all TLVs only after bounded slices and the count are checked.
pub fn decode_set(input: &[u8]) -> Result<Vec<KeyValue<'_>>, DecodeError> {
    let mut rest = input;
    let h: ListHeader = wire::decode(&take(&mut rest, 4)?)?;
    if usize::from(h.count) > rest.len() / 4 {
        return Err(DecodeError::InvalidLength {
            declared: usize::from(h.count) * 4 + 4,
            actual: input.len(),
        });
    }
    let mut out = Vec::with_capacity(usize::from(h.count));
    for _ in 0..h.count {
        let h: ValueHeader = wire::decode(&take(&mut rest, 4)?)?;
        let n = usize::from(h.length);
        let value = rest.get(..n).ok_or(DecodeError::TooShort {
            expected: n,
            actual: rest.len(),
        })?;
        out.push(KeyValue { key: h.key, value });
        rest = &rest[n..];
    }
    if !rest.is_empty() {
        return Err(DecodeError::InvalidLength {
            declared: input.len() - rest.len(),
            actual: input.len(),
        });
    }
    Ok(out)
}
pub fn decode_get(input: &[u8]) -> Result<Vec<u16>, DecodeError> {
    let mut rest = input;
    let h: ListHeader = wire::decode(&take(&mut rest, 4)?)?;
    let expected = usize::from(h.count) * 2;
    if rest.len() != expected {
        return Err(DecodeError::InvalidLength {
            declared: expected + 4,
            actual: input.len(),
        });
    }
    rest.chunks_exact(2).map(wire::decode).collect()
}
impl HostEndpoint {
    pub fn decode(input: &[u8]) -> Result<Self, DecodeError> {
        wire::decode(input)
    }
}
/// Fixed primitives/arrays only; no bincode enum tags or container prefixes.
pub fn encode_fixed<T: Serialize>(value: &T) -> Result<Vec<u8>, bincode::error::EncodeError> {
    bincode::serde::encode_to_vec(
        value,
        bincode::config::standard()
            .with_fixed_int_encoding()
            .with_little_endian(),
    )
}
impl DiscoveryResponse {
    pub fn decode(input: &[u8]) -> Result<Self, DecodeError> {
        wire::decode(input)
    }
}
