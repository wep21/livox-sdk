//! Independent LVX1 1.1 and LVX2 2.0 readers with explicit format detection.
#![forbid(unsafe_code)]
pub mod lvx2;
mod wire;
use livox_protocol::{DeviceType, legacy};
pub use lvx2::Lvx2Reader;
use std::io::{self, Read, Seek, SeekFrom};

pub const SIGNATURE: [u8; 16] = *b"livox_tech\0\0\0\0\0\0";
pub const MAGIC: u32 = 0xac0ea767;
pub const DEFAULT_MAX_FRAME_BYTES: u64 = 64 * 1024 * 1024;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Lvx1_1,
    Lvx2,
    Unknown([u8; 4]),
}
#[derive(Debug, thiserror::Error)]
pub enum LvxError {
    #[error("fixed-width wire decoding failed")]
    WireDecode,
    #[error("I/O: {0}")]
    Io(#[from] io::Error),
    #[error("truncated input: expected {expected} bytes, got {actual}")]
    Truncated { expected: usize, actual: usize },
    #[error("invalid LVX signature")]
    InvalidSignature,
    #[error("invalid LVX magic {0:#010x}")]
    InvalidMagic(u32),
    #[error("unsupported LVX format {0:?}")]
    UnsupportedFormat(Format),
    #[error("duplicate device ID {0}")]
    DuplicateDevice(u8),
    #[error("duplicate LVX2 LiDAR ID {0}")]
    DuplicateLidarId(u32),
    #[error("unknown LVX2 LiDAR ID {0}")]
    UnknownLidarId(u32),
    #[error("unknown device ID {0}")]
    UnknownDevice(u8),
    #[error("invalid extrinsic enable value {0}")]
    InvalidExtrinsic(u8),
    #[error(
        "invalid frame offsets: position {position}, current {current}, next {next}, file size {file_size}"
    )]
    InvalidFrameOffsets {
        position: u64,
        current: u64,
        next: u64,
        file_size: u64,
    },
    #[error("frame size {size} exceeds configured limit {limit}")]
    FrameTooLarge { size: u64, limit: u64 },
    #[error("integer overflow")]
    Overflow,
    #[error("allocation failed")]
    Allocation,
    #[error("packet: {0}")]
    Packet(#[from] livox_protocol::DecodeError),
}
fn bytes<const N: usize>(b: &[u8], o: usize) -> Result<[u8; N], LvxError> {
    let end = o.checked_add(N).ok_or(LvxError::Overflow)?;
    let b = b.get(o..end).ok_or(LvxError::Truncated {
        expected: end,
        actual: b.len(),
    })?;
    let mut a = [0; N];
    a.copy_from_slice(b);
    Ok(a)
}
/// Detect from a 24-byte public header. Filename extensions are ignored.
pub fn detect_format(b: &[u8]) -> Result<Format, LvxError> {
    let h: wire::Public = wire::decode(&bytes::<24>(b, 0)?)?;
    if h.signature != SIGNATURE {
        return Err(LvxError::InvalidSignature);
    }
    if h.magic != MAGIC {
        return Err(LvxError::InvalidMagic(h.magic));
    }
    Ok(match h.version {
        [1, 1, 0, 0] => Format::Lvx1_1,
        [2, 0, 0, 0] => Format::Lvx2,
        x => Format::Unknown(x),
    })
}
#[derive(Debug, Clone, PartialEq)]
pub struct LvxDeviceInfo {
    pub lidar_broadcast_code: [u8; 16],
    pub hub_broadcast_code: [u8; 16],
    pub device_index: u8,
    pub device_type: DeviceType,
    pub extrinsic_enabled: bool,
    pub rotation_deg: [f32; 3],
    pub translation_m: [f32; 3],
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LvxHeader {
    pub signature: [u8; 16],
    pub version: [u8; 4],
    pub magic: u32,
    pub frame_duration_ms: u32,
    pub device_count: u8,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LvxPacket {
    pub device_index: u8,
    pub header: legacy::Header,
    pub payload: Vec<u8>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LvxFrame {
    pub index: u64,
    pub packets: Vec<LvxPacket>,
}
#[derive(Debug, Clone, Copy)]
pub struct ReaderOptions {
    pub max_frame_bytes: u64,
}
impl Default for ReaderOptions {
    fn default() -> Self {
        Self {
            max_frame_bytes: DEFAULT_MAX_FRAME_BYTES,
        }
    }
}

fn frame_bounds(
    b: &[u8],
    position: u64,
    file_size: u64,
    limit: u64,
) -> Result<(u64, u64), LvxError> {
    let h: wire::Frame = wire::decode(&bytes::<24>(b, 0)?)?;
    let current = h.current;
    let next = h.next;
    let minimum = position.checked_add(24).ok_or(LvxError::Overflow)?;
    if current != position || next < minimum || next > file_size {
        return Err(LvxError::InvalidFrameOffsets {
            position,
            current,
            next,
            file_size,
        });
    }
    let size = next - position;
    if size > limit {
        return Err(LvxError::FrameTooLarge { size, limit });
    }
    Ok((next, h.index))
}
/// Parse one complete LVX1 frame. Offsets are absolute; `bytes` includes its header.
/// The same parser is used by the reader and by the frame fuzz target.
pub fn parse_frame(
    input: &[u8],
    position: u64,
    device_ids: &[u8],
    max_frame_bytes: u64,
) -> Result<LvxFrame, LvxError> {
    let file_end = position
        .checked_add(u64::try_from(input.len()).map_err(|_| LvxError::Overflow)?)
        .ok_or(LvxError::Overflow)?;
    let (next, index) = frame_bounds(input, position, file_end, max_frame_bytes)?;
    if next != file_end {
        return Err(LvxError::InvalidFrameOffsets {
            position,
            current: position,
            next,
            file_size: file_end,
        });
    }
    let mut remaining = &input[24..];
    let mut packets = Vec::new();
    while !remaining.is_empty() {
        let device_index = remaining[0];
        if !device_ids.contains(&device_index) {
            return Err(LvxError::UnknownDevice(device_index));
        }
        let header = legacy::Header::decode(&remaining[1..])?;
        let size = 19 + header.data_type.payload_len()?;
        let record = remaining.get(..size).ok_or(LvxError::Truncated {
            expected: size,
            actual: remaining.len(),
        })?;
        let mut payload = Vec::new();
        payload
            .try_reserve_exact(size - 19)
            .map_err(|_| LvxError::Allocation)?;
        payload.extend_from_slice(&record[19..]);
        packets.try_reserve(1).map_err(|_| LvxError::Allocation)?;
        packets.push(LvxPacket {
            device_index,
            header,
            payload,
        });
        remaining = &remaining[size..];
    }
    Ok(LvxFrame { index, packets })
}

pub struct LvxReader<R> {
    reader: R,
    header: LvxHeader,
    devices: Vec<LvxDeviceInfo>,
    position: u64,
    file_size: u64,
    options: ReaderOptions,
    done: bool,
}
impl<R: Read + Seek> LvxReader<R> {
    pub fn new(reader: R) -> Result<Self, LvxError> {
        Self::with_options(reader, ReaderOptions::default())
    }
    /// Starts at file offset zero, independent of the caller's current cursor.
    pub fn with_options(mut reader: R, options: ReaderOptions) -> Result<Self, LvxError> {
        let file_size = reader.seek(SeekFrom::End(0))?;
        reader.seek(SeekFrom::Start(0))?;
        let mut public = [0; 24];
        reader.read_exact(&mut public)?;
        let format = detect_format(&public)?;
        if format != Format::Lvx1_1 {
            return Err(LvxError::UnsupportedFormat(format));
        }
        let mut private = [0; 5];
        reader.read_exact(&mut private)?;
        let p: wire::Private = wire::decode(&private)?;
        let header = LvxHeader {
            signature: SIGNATURE,
            version: [1, 1, 0, 0],
            magic: MAGIC,
            frame_duration_ms: p.frame_duration,
            device_count: p.device_count,
        };
        if header.frame_duration_ms != 50 {
            tracing::warn!(
                duration_ms = header.frame_duration_ms,
                "LVX1 frame duration differs from published 50 ms"
            );
        }
        let mut devices = Vec::<LvxDeviceInfo>::new();
        for _ in 0..header.device_count {
            let mut d = [0; 59];
            reader.read_exact(&mut d)?;
            let d: wire::Device1 = wire::decode(&d)?;
            if devices.iter().any(|x| x.device_index == d.index) {
                return Err(LvxError::DuplicateDevice(d.index));
            }
            if d.extrinsic_enable > 1 {
                return Err(LvxError::InvalidExtrinsic(d.extrinsic_enable));
            }
            devices.push(LvxDeviceInfo {
                lidar_broadcast_code: d.lidar_serial,
                hub_broadcast_code: d.hub_serial,
                device_index: d.index,
                device_type: d.device_type.into(),
                extrinsic_enabled: d.extrinsic_enable == 1,
                rotation_deg: d.rotation,
                translation_m: d.translation,
            });
        }
        let position = 29 + u64::from(header.device_count) * 59;
        Ok(Self {
            reader,
            header,
            devices,
            position,
            file_size,
            options,
            done: false,
        })
    }
    pub fn header(&self) -> &LvxHeader {
        &self.header
    }
    pub fn devices(&self) -> &[LvxDeviceInfo] {
        &self.devices
    }
    /// Single-pass iterator; repeated calls continue from the current position.
    /// An error is returned once; later calls remain exhausted.
    pub fn frames(&mut self) -> LvxFrameIterator<'_, R> {
        LvxFrameIterator { reader: self }
    }
    fn read_frame(&mut self) -> Result<LvxFrame, LvxError> {
        let mut header = [0; 24];
        self.reader.read_exact(&mut header)?;
        let (next, _) = frame_bounds(
            &header,
            self.position,
            self.file_size,
            self.options.max_frame_bytes,
        )?;
        let size = usize::try_from(next - self.position).map_err(|_| LvxError::Overflow)?;
        let mut b = Vec::new();
        b.try_reserve_exact(size)
            .map_err(|_| LvxError::Allocation)?;
        b.extend_from_slice(&header);
        b.resize(size, 0);
        self.reader.read_exact(&mut b[24..])?;
        let ids: Vec<_> = self.devices.iter().map(|x| x.device_index).collect();
        let frame = parse_frame(&b, self.position, &ids, self.options.max_frame_bytes)?;
        self.position = next;
        Ok(frame)
    }
}
pub struct LvxFrameIterator<'a, R> {
    reader: &'a mut LvxReader<R>,
}
impl<R: Read + Seek> Iterator for LvxFrameIterator<'_, R> {
    type Item = Result<LvxFrame, LvxError>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.reader.done || self.reader.position == self.reader.file_size {
            self.reader.done = true;
            return None;
        }
        let result = self.reader.read_frame();
        if result.is_err() {
            self.reader.done = true;
        }
        Some(result)
    }
}
impl<R: Read + Seek> std::iter::FusedIterator for LvxFrameIterator<'_, R> {}
