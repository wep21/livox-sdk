use crate::{DecodeError, crc16, crc32, wire};
pub const COMMAND_HEADER_LEN: usize = 24;
pub const MAX_COMMAND_LEN: usize = 1400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandType {
    Request,
    Ack,
    Unknown(u8),
}
impl From<u8> for CommandType {
    fn from(v: u8) -> Self {
        match v {
            0 => Self::Request,
            1 => Self::Ack,
            x => Self::Unknown(x),
        }
    }
}
impl CommandType {
    pub fn wire_value(self) -> u8 {
        match self {
            Self::Request => 0,
            Self::Ack => 1,
            Self::Unknown(x) => x,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SenderType {
    Host,
    Lidar,
    Unknown(u8),
}
impl From<u8> for SenderType {
    fn from(v: u8) -> Self {
        match v {
            0 => Self::Host,
            1 => Self::Lidar,
            x => Self::Unknown(x),
        }
    }
}
impl SenderType {
    pub fn wire_value(self) -> u8 {
        match self {
            Self::Host => 0,
            Self::Lidar => 1,
            Self::Unknown(x) => x,
        }
    }
}

/// Semantic header; fixed SOF/version, length and CRCs are derived by the codec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandHeader {
    pub seq_num: u32,
    pub cmd_id: u16,
    pub cmd_type: CommandType,
    pub sender_type: SenderType,
    pub reserved: [u8; 6],
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandPacket<'a> {
    pub header: CommandHeader,
    pub payload: &'a [u8],
}
impl<'a> CommandPacket<'a> {
    /// Decode exactly one complete datagram, including both CRC checks.
    pub fn decode(input: &'a [u8]) -> Result<Self, DecodeError> {
        if input.len() < COMMAND_HEADER_LEN {
            return Err(DecodeError::TooShort {
                expected: COMMAND_HEADER_LEN,
                actual: input.len(),
            });
        }
        let h: wire::Command = wire::decode(&input[..COMMAND_HEADER_LEN])?;
        if h.sof != 0xaa {
            return Err(DecodeError::InvalidSof(h.sof));
        }
        if h.version != 0 {
            return Err(DecodeError::UnsupportedVersion(h.version));
        }
        let length = usize::from(h.length);
        if !(COMMAND_HEADER_LEN..=MAX_COMMAND_LEN).contains(&length) || length != input.len() {
            return Err(DecodeError::InvalidLength {
                declared: length,
                actual: input.len(),
            });
        }
        if crc16(&input[..18]) != h.crc16 {
            return Err(DecodeError::Crc16Mismatch);
        }
        let payload = &input[COMMAND_HEADER_LEN..];
        if crc32(payload) != h.crc32 {
            return Err(DecodeError::Crc32Mismatch);
        }
        Ok(Self {
            header: CommandHeader {
                seq_num: h.seq_num,
                cmd_id: h.cmd_id,
                cmd_type: h.cmd_type.into(),
                sender_type: h.sender_type.into(),
                reserved: h.reserved,
            },
            payload,
        })
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandPacketOwned {
    pub header: CommandHeader,
    pub payload: Vec<u8>,
}
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EncodeError {
    #[error("payload length {actual} exceeds maximum {maximum}")]
    PayloadTooLarge { actual: usize, maximum: usize },
}
impl CommandPacketOwned {
    /// Append one datagram. On validation error, `out` is unchanged.
    pub fn encode(&self, out: &mut Vec<u8>) -> Result<(), EncodeError> {
        if self.payload.len() > MAX_COMMAND_LEN - COMMAND_HEADER_LEN {
            return Err(EncodeError::PayloadTooLarge {
                actual: self.payload.len(),
                maximum: MAX_COMMAND_LEN - COMMAND_HEADER_LEN,
            });
        }
        let mut header = Vec::with_capacity(COMMAND_HEADER_LEN);
        header.extend([0xaa, 0]);
        header.extend(((COMMAND_HEADER_LEN + self.payload.len()) as u16).to_le_bytes());
        header.extend(self.header.seq_num.to_le_bytes());
        header.extend(self.header.cmd_id.to_le_bytes());
        header.extend([
            self.header.cmd_type.wire_value(),
            self.header.sender_type.wire_value(),
        ]);
        header.extend(self.header.reserved);
        let crc = crc16(&header);
        header.extend(crc.to_le_bytes());
        header.extend(crc32(&self.payload).to_le_bytes());
        out.extend(header);
        out.extend(&self.payload);
        Ok(())
    }
}
