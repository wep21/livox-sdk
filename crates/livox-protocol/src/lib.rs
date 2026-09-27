//! Explicit, endian-independent Livox wire codecs. No transport or file I/O.
#![forbid(unsafe_code)]
mod command;
pub mod control;
mod data;
pub mod legacy;
mod wire;
pub use command::*;
pub use data::*;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    #[error("fixed-width wire decoding failed")]
    WireDecode,
    #[error("packet too short: expected {expected}, got {actual}")]
    TooShort { expected: usize, actual: usize },
    #[error("invalid SOF: {0:#04x}")]
    InvalidSof(u8),
    #[error("unsupported protocol version: {0}")]
    UnsupportedVersion(u8),
    #[error("invalid declared length {declared} for buffer of {actual} bytes")]
    InvalidLength { declared: usize, actual: usize },
    #[error("CRC16 mismatch")]
    Crc16Mismatch,
    #[error("CRC32 mismatch")]
    Crc32Mismatch,
    #[error("payload size mismatch: expected {expected}, got {actual}")]
    PayloadSize { expected: usize, actual: usize },
    #[error("unsupported legacy data type: {0:#04x}")]
    UnsupportedLegacyType(u8),
}

// All offsets are fixed internal constants; get() also guards arbitrary inputs.
fn array<const N: usize>(input: &[u8], offset: usize) -> Result<[u8; N], DecodeError> {
    let expected = offset.saturating_add(N);
    let bytes = input.get(offset..expected).ok_or(DecodeError::TooShort {
        expected,
        actual: input.len(),
    })?;
    let mut out = [0; N];
    out.copy_from_slice(bytes);
    Ok(out)
}

/// CRC-16/CCITT-FALSE: poly 0x1021, init ffff, no reflection, xorout 0.
pub fn crc16(bytes: &[u8]) -> u16 {
    let mut crc = 0xffffu16;
    for &byte in bytes {
        crc ^= u16::from(byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}
/// CRC-32/ISO-HDLC, reflected polynomial edb88320, init/xorout ffffffff.
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb88320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceType {
    Hub,
    Mid40,
    Tele15,
    Horizon,
    Mid70,
    Avia,
    Mid360,
    IndustrialHap,
    Hap,
    Pa,
    Mid360S,
    Avia2,
    Mid360L,
    Unknown(u8),
}
impl From<u8> for DeviceType {
    fn from(v: u8) -> Self {
        match v {
            0 => Self::Hub,
            1 => Self::Mid40,
            2 => Self::Tele15,
            3 => Self::Horizon,
            6 => Self::Mid70,
            7 => Self::Avia,
            9 => Self::Mid360,
            10 => Self::IndustrialHap,
            15 => Self::Hap,
            16 => Self::Pa,
            35 => Self::Mid360S,
            40 => Self::Avia2,
            41 => Self::Mid360L,
            x => Self::Unknown(x),
        }
    }
}
