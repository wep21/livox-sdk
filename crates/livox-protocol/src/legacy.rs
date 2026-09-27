//! SDK1 packet framing. Type numbers and timestamps differ from SDK2.
use crate::{DecodeError, wire};
pub const HEADER_LEN: usize = 18;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataType {
    Cartesian,
    Spherical,
    CartesianTagged,
    SphericalTagged,
    DualCartesian,
    DualSpherical,
    Imu,
    TripleCartesian,
    TripleSpherical,
    Unknown(u8),
}
impl From<u8> for DataType {
    fn from(x: u8) -> Self {
        match x {
            0 => Self::Cartesian,
            1 => Self::Spherical,
            2 => Self::CartesianTagged,
            3 => Self::SphericalTagged,
            4 => Self::DualCartesian,
            5 => Self::DualSpherical,
            6 => Self::Imu,
            7 => Self::TripleCartesian,
            8 => Self::TripleSpherical,
            x => Self::Unknown(x),
        }
    }
}
impl DataType {
    pub fn payload_len(self) -> Result<usize, DecodeError> {
        Ok(match self {
            Self::Cartesian => 1300,
            Self::Spherical => 900,
            Self::CartesianTagged | Self::DualCartesian => 1344,
            Self::SphericalTagged => 960,
            Self::DualSpherical => 768,
            Self::Imu => 24,
            Self::TripleCartesian => 1260,
            Self::TripleSpherical => 660,
            Self::Unknown(x) => return Err(DecodeError::UnsupportedLegacyType(x)),
        })
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub slot_id: u8,
    pub lidar_id: u8,
    pub reserved: u8,
    pub status: u32,
    pub timestamp_type: u8,
    pub data_type: DataType,
    pub timestamp: [u8; 8],
}
impl Header {
    pub fn decode(input: &[u8]) -> Result<Self, DecodeError> {
        if input.len() < HEADER_LEN {
            return Err(DecodeError::TooShort {
                expected: HEADER_LEN,
                actual: input.len(),
            });
        }
        let h: wire::Legacy = wire::decode(&input[..HEADER_LEN])?;
        if h.version != 5 {
            return Err(DecodeError::UnsupportedVersion(h.version));
        }
        Ok(Self {
            slot_id: h.slot_id,
            lidar_id: h.lidar_id,
            reserved: h.reserved,
            status: h.status,
            timestamp_type: h.timestamp_type,
            data_type: h.data_type.into(),
            timestamp: h.timestamp,
        })
    }
    /// UTC type 3 and unknown representations intentionally return None.
    pub fn timestamp_ns(&self) -> Option<u64> {
        match self.timestamp_type {
            0 | 1 | 4 => Some(u64::from_le_bytes(self.timestamp)),
            _ => None,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packet<'a> {
    pub header: Header,
    pub payload: &'a [u8],
}
impl<'a> Packet<'a> {
    pub fn decode(input: &'a [u8]) -> Result<Self, DecodeError> {
        let header = Header::decode(input)?;
        let expected = HEADER_LEN + header.data_type.payload_len()?;
        if input.len() != expected {
            return Err(DecodeError::PayloadSize {
                expected,
                actual: input.len(),
            });
        }
        Ok(Self {
            header,
            payload: &input[HEADER_LEN..],
        })
    }
}
