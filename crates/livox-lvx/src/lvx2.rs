//! Independent parser for the published LVX2 2.0.0.0 format.
use crate::{
    Format, LvxError, LvxHeader, MAGIC, ReaderOptions, SIGNATURE, bytes, detect_format,
    frame_bounds, wire,
};
use livox_protocol::{CartesianHighPoints, CartesianLowPoints, DeviceType, TimeType};
use std::io::{Read, Seek, SeekFrom};

#[derive(Debug, Clone, PartialEq)]
pub struct Lvx2DeviceInfo {
    pub lidar_serial: [u8; 16],
    pub hub_serial: [u8; 16],
    pub lidar_id: u32,
    pub lidar_type: u8,
    pub device_type: DeviceType,
    pub extrinsic_enabled: bool,
    pub rotation_deg: [f32; 3],
    pub translation_m: [f32; 3],
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lvx2Packet {
    pub lidar_id: u32,
    pub lidar_type: u8,
    pub time_type: TimeType,
    pub timestamp: u64,
    pub udp_counter: u16,
    pub data_type: u8,
    pub frame_counter: u8,
    pub reserved: [u8; 4],
    payload: Vec<u8>,
}
#[derive(Debug, Clone, PartialEq)]
pub enum Lvx2Payload<'a> {
    CartesianHigh(CartesianHighPoints<'a>),
    CartesianLow(CartesianLowPoints<'a>),
    Unknown { data_type: u8, bytes: &'a [u8] },
}
impl Lvx2Packet {
    pub fn payload_bytes(&self) -> &[u8] {
        &self.payload
    }
    pub fn payload(&self) -> Result<Lvx2Payload<'_>, LvxError> {
        let count =
            |width| u16::try_from(self.payload.len() / width).map_err(|_| LvxError::Overflow);
        Ok(match self.data_type {
            1 => {
                Lvx2Payload::CartesianHigh(CartesianHighPoints::decode(&self.payload, count(14)?)?)
            }
            2 => Lvx2Payload::CartesianLow(CartesianLowPoints::decode(&self.payload, count(8)?)?),
            data_type => Lvx2Payload::Unknown {
                data_type,
                bytes: &self.payload,
            },
        })
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lvx2Frame {
    pub index: u64,
    pub packets: Vec<Lvx2Packet>,
}
/// Separate LVX2 record parser: 27-byte file headers, never UDP headers.
pub fn parse_frame(
    input: &[u8],
    position: u64,
    device_ids: &[u32],
    max_frame_bytes: u64,
) -> Result<Lvx2Frame, LvxError> {
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
        let h: wire::Packet2 = wire::decode(&bytes::<27>(remaining, 0)?)?;
        if h.version != 0 {
            return Err(livox_protocol::DecodeError::UnsupportedVersion(h.version).into());
        }
        let lidar_id = h.lidar_id;
        if !device_ids.contains(&lidar_id) {
            return Err(LvxError::UnknownLidarId(lidar_id));
        }
        let size = usize::try_from(h.length).map_err(|_| LvxError::Overflow)?;
        let total = 27usize.checked_add(size).ok_or(LvxError::Overflow)?;
        let record = remaining.get(..total).ok_or(LvxError::Truncated {
            expected: total,
            actual: remaining.len(),
        })?;
        let mut payload = Vec::new();
        payload
            .try_reserve_exact(size)
            .map_err(|_| LvxError::Allocation)?;
        payload.extend_from_slice(&record[27..]);
        let packet = Lvx2Packet {
            lidar_id,
            lidar_type: h.lidar_type,
            time_type: h.time_type.into(),
            timestamp: h.timestamp,
            udp_counter: h.udp_counter,
            data_type: h.data_type,
            frame_counter: h.frame_counter,
            reserved: h.reserved,
            payload,
        };
        // Published records have 96 points, but divisibility/count safety is enough
        // to preserve other valid package sizes rather than invent a fixed length.
        packet.payload()?;
        packets.try_reserve(1).map_err(|_| LvxError::Allocation)?;
        packets.push(packet);
        remaining = &remaining[total..];
    }
    Ok(Lvx2Frame { index, packets })
}
pub struct Lvx2Reader<R> {
    reader: R,
    header: LvxHeader,
    devices: Vec<Lvx2DeviceInfo>,
    position: u64,
    file_size: u64,
    options: ReaderOptions,
    done: bool,
}
impl<R: Read + Seek> Lvx2Reader<R> {
    pub fn new(reader: R) -> Result<Self, LvxError> {
        Self::with_options(reader, ReaderOptions::default())
    }
    pub fn with_options(mut reader: R, options: ReaderOptions) -> Result<Self, LvxError> {
        let file_size = reader.seek(SeekFrom::End(0))?;
        reader.seek(SeekFrom::Start(0))?;
        let mut public = [0; 24];
        reader.read_exact(&mut public)?;
        let format = detect_format(&public)?;
        if format != Format::Lvx2 {
            return Err(LvxError::UnsupportedFormat(format));
        }
        let mut private = [0; 5];
        reader.read_exact(&mut private)?;
        let p: wire::Private = wire::decode(&private)?;
        let header = LvxHeader {
            signature: SIGNATURE,
            version: [2, 0, 0, 0],
            magic: MAGIC,
            frame_duration_ms: p.frame_duration,
            device_count: p.device_count,
        };
        if header.frame_duration_ms != 50 {
            tracing::warn!(
                duration_ms = header.frame_duration_ms,
                "LVX2 frame duration differs from published 50 ms"
            );
        }
        let mut devices = Vec::<Lvx2DeviceInfo>::new();
        for _ in 0..header.device_count {
            let mut d = [0; 63];
            reader.read_exact(&mut d)?;
            let d: wire::Device2 = wire::decode(&d)?;
            let lidar_id = d.lidar_id;
            if devices.iter().any(|x| x.lidar_id == lidar_id) {
                return Err(LvxError::DuplicateLidarId(lidar_id));
            }
            if d.extrinsic_enable > 1 {
                return Err(LvxError::InvalidExtrinsic(d.extrinsic_enable));
            }
            devices.push(Lvx2DeviceInfo {
                lidar_serial: d.lidar_serial,
                hub_serial: d.hub_serial,
                lidar_id,
                lidar_type: d.lidar_type,
                device_type: d.device_type.into(),
                extrinsic_enabled: d.extrinsic_enable == 1,
                rotation_deg: d.rotation,
                translation_m: d.translation,
            });
        }
        let position = 29 + u64::from(header.device_count) * 63;
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
    pub fn devices(&self) -> &[Lvx2DeviceInfo] {
        &self.devices
    }
    pub fn frames(&mut self) -> Lvx2FrameIterator<'_, R> {
        Lvx2FrameIterator { reader: self }
    }
    fn read_frame(&mut self) -> Result<Lvx2Frame, LvxError> {
        let mut h = [0; 24];
        self.reader.read_exact(&mut h)?;
        let (next, _) = frame_bounds(
            &h,
            self.position,
            self.file_size,
            self.options.max_frame_bytes,
        )?;
        let size = usize::try_from(next - self.position).map_err(|_| LvxError::Overflow)?;
        let mut b = Vec::new();
        b.try_reserve_exact(size)
            .map_err(|_| LvxError::Allocation)?;
        b.extend_from_slice(&h);
        b.resize(size, 0);
        self.reader.read_exact(&mut b[24..])?;
        let ids: Vec<_> = self.devices.iter().map(|d| d.lidar_id).collect();
        let frame = parse_frame(&b, self.position, &ids, self.options.max_frame_bytes)?;
        self.position = next;
        Ok(frame)
    }
}
pub struct Lvx2FrameIterator<'a, R> {
    reader: &'a mut Lvx2Reader<R>,
}
impl<R: Read + Seek> Iterator for Lvx2FrameIterator<'_, R> {
    type Item = Result<Lvx2Frame, LvxError>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.reader.done || self.reader.position == self.reader.file_size {
            self.reader.done = true;
            return None;
        }
        let r = self.reader.read_frame();
        if r.is_err() {
            self.reader.done = true;
        }
        Some(r)
    }
}
impl<R: Read + Seek> std::iter::FusedIterator for Lvx2FrameIterator<'_, R> {}
