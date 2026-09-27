use crate::{DecodeError, array, crc32, wire};
pub const DATA_HEADER_LEN: usize = 36;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeType {
    Uptime,
    Ptp,
    Gps,
    Unknown(u8),
}
impl From<u8> for TimeType {
    fn from(v: u8) -> Self {
        match v {
            0 => Self::Uptime,
            1 => Self::Ptp,
            2 => Self::Gps,
            x => Self::Unknown(x),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataHeader {
    pub length: u16,
    pub time_interval: u16,
    pub dot_num: u16,
    pub udp_cnt: u16,
    pub frame_cnt: u8,
    pub data_type: u8,
    pub time_type: TimeType,
    pub reserved: [u8; 12],
    pub crc32: u32,
    pub timestamp: u64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct DataPacket<'a> {
    pub header: DataHeader,
    pub payload: DataPayload<'a>,
}
#[derive(Debug, Clone, PartialEq)]
pub enum DataPayload<'a> {
    Imu(ImuData),
    CartesianHigh(CartesianHighPoints<'a>),
    CartesianLow(CartesianLowPoints<'a>),
    Spherical(SphericalPoints<'a>),
    /// Includes SDK-only 0x11 until its count semantics have a verified profile.
    Unknown {
        data_type: u8,
        bytes: &'a [u8],
    },
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImuData {
    pub gyro: [f32; 3],
    pub acceleration: [f32; 3],
}
impl ImuData {
    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        exact_size(bytes, 24)?;
        let f = |o| -> Result<f32, DecodeError> { Ok(f32::from_le_bytes(array(bytes, o)?)) };
        Ok(Self {
            gyro: [f(0)?, f(4)?, f(8)?],
            acceleration: [f(12)?, f(16)?, f(20)?],
        })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CartesianHighPoint {
    pub xyz_mm: [i32; 3],
    pub reflectivity: u8,
    pub tag: u8,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CartesianLowPoint {
    pub xyz_cm: [i16; 3],
    pub reflectivity: u8,
    pub tag: u8,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SphericalPoint {
    pub depth_mm: u32,
    pub theta: u16,
    pub phi: u16,
    pub reflectivity: u8,
    pub tag: u8,
}
fn exact_size(b: &[u8], expected: usize) -> Result<(), DecodeError> {
    if b.len() != expected {
        Err(DecodeError::PayloadSize {
            expected,
            actual: b.len(),
        })
    } else {
        Ok(())
    }
}

// Private byte storage ensures iterators only see whole records. Each fixed-size
// conversion below indexes a chunks_exact record, never unvalidated user offsets.
macro_rules! points {
    ($name:ident,$point:ty,$width:expr,$decode:expr) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name<'a> {
            bytes: &'a [u8],
        }
        impl<'a> $name<'a> {
            pub fn decode(bytes: &'a [u8], count: u16) -> Result<Self, DecodeError> {
                exact_size(bytes, usize::from(count) * $width)?;
                Ok(Self { bytes })
            }
            pub fn len(&self) -> usize {
                self.bytes.len() / $width
            }
            pub fn is_empty(&self) -> bool {
                self.bytes.is_empty()
            }
            pub fn as_bytes(&self) -> &'a [u8] {
                self.bytes
            }
            pub fn iter(&self) -> impl ExactSizeIterator<Item = $point> + 'a {
                self.bytes.chunks_exact($width).map($decode)
            }
        }
    };
}
points!(CartesianHighPoints, CartesianHighPoint, 14, |b: &[u8]| {
    CartesianHighPoint {
        xyz_mm: [
            i32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            i32::from_le_bytes([b[4], b[5], b[6], b[7]]),
            i32::from_le_bytes([b[8], b[9], b[10], b[11]]),
        ],
        reflectivity: b[12],
        tag: b[13],
    }
});
points!(CartesianLowPoints, CartesianLowPoint, 8, |b: &[u8]| {
    CartesianLowPoint {
        xyz_cm: [
            i16::from_le_bytes([b[0], b[1]]),
            i16::from_le_bytes([b[2], b[3]]),
            i16::from_le_bytes([b[4], b[5]]),
        ],
        reflectivity: b[6],
        tag: b[7],
    }
});
points!(SphericalPoints, SphericalPoint, 10, |b: &[u8]| {
    SphericalPoint {
        depth_mm: u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
        theta: u16::from_le_bytes([b[4], b[5]]),
        phi: u16::from_le_bytes([b[6], b[7]]),
        reflectivity: b[8],
        tag: b[9],
    }
});
impl<'a> DataPacket<'a> {
    pub fn decode(input: &'a [u8]) -> Result<Self, DecodeError> {
        if input.len() < DATA_HEADER_LEN {
            return Err(DecodeError::TooShort {
                expected: DATA_HEADER_LEN,
                actual: input.len(),
            });
        }
        let h: wire::Data = wire::decode(&input[..DATA_HEADER_LEN])?;
        if h.version != 0 {
            return Err(DecodeError::UnsupportedVersion(h.version));
        }
        let length = h.length;
        if usize::from(length) != input.len() {
            return Err(DecodeError::InvalidLength {
                declared: usize::from(length),
                actual: input.len(),
            });
        }
        let expected = h.crc32;
        if crc32(&input[28..]) != expected {
            return Err(DecodeError::Crc32Mismatch);
        }
        let header = DataHeader {
            length,
            time_interval: h.time_interval,
            dot_num: h.dot_num,
            udp_cnt: h.udp_cnt,
            frame_cnt: h.frame_cnt,
            data_type: h.data_type,
            time_type: h.time_type.into(),
            reserved: h.reserved,
            crc32: expected,
            timestamp: h.timestamp,
        };
        let bytes = &input[DATA_HEADER_LEN..];
        let payload = match header.data_type {
            0 => DataPayload::Imu(ImuData::decode(bytes)?),
            1 => DataPayload::CartesianHigh(CartesianHighPoints::decode(bytes, header.dot_num)?),
            2 => DataPayload::CartesianLow(CartesianLowPoints::decode(bytes, header.dot_num)?),
            3 => DataPayload::Spherical(SphericalPoints::decode(bytes, header.dot_num)?),
            data_type => DataPayload::Unknown { data_type, bytes },
        };
        Ok(Self { header, payload })
    }
}

/// Owned SDK2 data packet. Length and CRC fields in `header` are recomputed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataPacketOwned {
    pub header: DataHeader,
    pub payload: Vec<u8>,
}
#[derive(Debug, thiserror::Error)]
pub enum DataEncodeError {
    #[error("data packet exceeds UDP IPv4 payload maximum 65507 bytes")]
    TooLarge,
    #[error("invalid payload: {0}")]
    InvalidPayload(#[from] DecodeError),
    #[error("wire serialization failed: {0}")]
    Wire(#[from] bincode::error::EncodeError),
}
impl DataPacketOwned {
    /// Appends a validated datagram atomically; Serde emits the documented header.
    pub fn encode(&self, out: &mut Vec<u8>) -> Result<(), DataEncodeError> {
        let length = self
            .payload
            .len()
            .checked_add(DATA_HEADER_LEN)
            .filter(|x| *x <= 65507)
            .ok_or(DataEncodeError::TooLarge)?;
        let time_type = match self.header.time_type {
            TimeType::Uptime => 0,
            TimeType::Ptp => 1,
            TimeType::Gps => 2,
            TimeType::Unknown(v) => v,
        };
        let mut crc_bytes = Vec::with_capacity(8 + self.payload.len());
        crc_bytes.extend(self.header.timestamp.to_le_bytes());
        crc_bytes.extend_from_slice(&self.payload);
        let h = wire::Data {
            version: 0,
            length: length as u16,
            time_interval: self.header.time_interval,
            dot_num: self.header.dot_num,
            udp_cnt: self.header.udp_cnt,
            frame_cnt: self.header.frame_cnt,
            data_type: self.header.data_type,
            time_type,
            reserved: self.header.reserved,
            crc32: crc32(&crc_bytes),
            timestamp: self.header.timestamp,
        };
        let mut bytes = crate::control::encode_fixed(&h)?;
        bytes.extend_from_slice(&self.payload);
        DataPacket::decode(&bytes)?;
        out.extend(bytes);
        Ok(())
    }
}
