//! LVX2 replay orchestration. Device state, file parsing and UDP stay separate.
#![forbid(unsafe_code)]
use livox_device::{DeviceError, Mid360};
use livox_lvx::{Lvx2Reader, LvxError, lvx2::Lvx2Packet};
use livox_protocol::{
    CommandPacket, DataEncodeError, DataHeader, DataPacketOwned, DeviceType, TimeType,
    control::HostEndpoint,
};
use livox_transport::{CommandChannel, Ports, UdpTransport};
use std::{
    fs::File,
    io::BufReader,
    net::{Ipv4Addr, SocketAddrV4},
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
#[derive(Debug, thiserror::Error)]
pub enum SimError {
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("LVX2: {0}")]
    Lvx(#[from] LvxError),
    #[error("device: {0}")]
    Device(#[from] DeviceError),
    #[error("data encoding: {0}")]
    Data(#[from] DataEncodeError),
    #[error("command encoding: {0}")]
    Command(#[from] livox_protocol::EncodeError),
    #[error("select a unique Mid-360 device using --device-id")]
    DeviceSelection,
    #[error("selected recording contains no point packets")]
    Empty,
    #[error("unsupported or changing recording data/time type")]
    UnsupportedRecord,
    #[error("recorded timestamps go backwards")]
    TimestampRegression,
    #[error("timestamp/duration/interval overflow or invalid replay option")]
    InvalidTiming,
    #[error("bind/host must be concrete unicast IPv4 addresses; destination ports must be nonzero")]
    InvalidAddress,
}
#[derive(Debug, Clone)]
pub struct ReplayOptions {
    pub path: PathBuf,
    pub device_id: Option<u32>,
    pub bind: Ipv4Addr,
    pub host: Ipv4Addr,
    pub ports: Ports,
    pub point_port: u16,
    pub imu_port: u16,
    pub announce_port: Option<u16>,
    pub start: bool,
    pub looping: bool,
    pub speed: f64,
    pub point_interval_ns: u64,
    pub max_packets: Option<u64>,
    pub run_for: Option<Duration>,
}
#[derive(Debug, Default, Clone, Copy)]
pub struct RunStats {
    pub points_sent: u64,
    pub commands_replied: u64,
    pub malformed_dropped: u64,
}
struct Source {
    path: PathBuf,
    reader: Lvx2Reader<BufReader<File>>,
    id: u32,
    packets: std::vec::IntoIter<Lvx2Packet>,
    first_raw: Option<u64>,
    last_raw: Option<u64>,
    last_out: Option<u64>,
    offset: u64,
    format: Option<u8>,
    time_type: Option<TimeType>,
    looping: bool,
    frame_ns: u64,
}
impl Source {
    fn open(o: &ReplayOptions) -> Result<Self, SimError> {
        let reader = Lvx2Reader::new(BufReader::new(File::open(&o.path)?))?;
        let choices: Vec<_> = reader
            .devices()
            .iter()
            .filter(|d| o.device_id.is_none_or(|id| d.lidar_id == id))
            .collect();
        if choices.len() != 1 || choices[0].device_type != DeviceType::Mid360 {
            return Err(SimError::DeviceSelection);
        }
        let id = choices[0].lidar_id;
        let frame_ns = u64::from(reader.header().frame_duration_ms) * 1_000_000;
        if frame_ns == 0 {
            return Err(SimError::InvalidTiming);
        }
        Ok(Self {
            path: o.path.clone(),
            reader,
            id,
            packets: Vec::new().into_iter(),
            first_raw: None,
            last_raw: None,
            last_out: None,
            offset: 0,
            format: None,
            time_type: None,
            looping: o.looping,
            frame_ns,
        })
    }
    fn next(&mut self) -> Result<Option<Lvx2Packet>, SimError> {
        loop {
            if let Some(mut p) = self.packets.next() {
                if p.lidar_id != self.id {
                    continue;
                }
                if !matches!(p.data_type, 1 | 2)
                    || matches!(p.time_type, TimeType::Unknown(_))
                    || p.payload_bytes().is_empty()
                    || self.format.is_some_and(|x| x != p.data_type)
                    || self.time_type.is_some_and(|x| x != p.time_type)
                {
                    return Err(SimError::UnsupportedRecord);
                }
                if self.last_raw.is_some_and(|last| p.timestamp < last) {
                    return Err(SimError::TimestampRegression);
                }
                self.first_raw.get_or_insert(p.timestamp);
                self.last_raw = Some(p.timestamp);
                self.format = Some(p.data_type);
                self.time_type = Some(p.time_type);
                p.timestamp = p
                    .timestamp
                    .checked_add(self.offset)
                    .ok_or(SimError::InvalidTiming)?;
                self.last_out = Some(p.timestamp);
                return Ok(Some(p));
            }
            match self.reader.frames().next() {
                Some(frame) => self.packets = frame?.packets.into_iter(),
                None => {
                    if !self.looping {
                        return Ok(None);
                    }
                    let (Some(first), Some(last)) = (self.first_raw, self.last_out) else {
                        return Err(SimError::Empty);
                    };
                    if self.last_raw.is_none() {
                        return Err(SimError::Empty);
                    }
                    self.offset = last
                        .checked_add(self.frame_ns)
                        .and_then(|v| v.checked_sub(first))
                        .ok_or(SimError::InvalidTiming)?;
                    self.reader = Lvx2Reader::new(BufReader::new(File::open(&self.path)?))?;
                    self.last_raw = None;
                }
            }
        }
    }
}
/// Reconstruct the missing UDP header; caller explicitly supplies point spacing.
pub fn encode_record(packet: &Lvx2Packet, point_interval_ns: u64) -> Result<Vec<u8>, SimError> {
    let width = match packet.data_type {
        1 => 14,
        2 => 8,
        _ => return Err(SimError::UnsupportedRecord),
    };
    if matches!(packet.time_type, TimeType::Unknown(_)) {
        return Err(SimError::UnsupportedRecord);
    }
    let count =
        u16::try_from(packet.payload_bytes().len() / width).map_err(|_| SimError::InvalidTiming)?;
    if count == 0 {
        return Err(SimError::Empty);
    }
    let interval = u64::from(count - 1)
        .checked_mul(point_interval_ns)
        .ok_or(SimError::InvalidTiming)?;
    if interval % 100 != 0 {
        return Err(SimError::InvalidTiming);
    }
    let time_interval = u16::try_from(interval / 100).map_err(|_| SimError::InvalidTiming)?;
    let p = DataPacketOwned {
        header: DataHeader {
            length: 0,
            time_interval,
            dot_num: count,
            udp_cnt: packet.udp_counter,
            // LVX2 declares its frame counter reserved. Mid-360 non-repeating
            // scan does not use the UDP frame counter; do not invent semantics.
            frame_cnt: 0,
            data_type: packet.data_type,
            time_type: packet.time_type,
            reserved: [0; 12],
            crc32: 0,
            timestamp: packet.timestamp,
        },
        payload: packet.payload_bytes().to_vec(),
    };
    let mut out = Vec::new();
    p.encode(&mut out)?;
    Ok(out)
}
/// Monotonic elapsed-play-time deadline, independent of wall-clock epoch.
pub fn replay_deadline(first: u64, timestamp: u64, speed: f64) -> Result<Duration, SimError> {
    if !speed.is_finite() || speed <= 0. {
        return Err(SimError::InvalidTiming);
    }
    let ns = timestamp
        .checked_sub(first)
        .ok_or(SimError::TimestampRegression)?;
    Duration::try_from_secs_f64(ns as f64 / 1e9 / speed).map_err(|_| SimError::InvalidTiming)
}
pub struct VirtualDevice {
    options: ReplayOptions,
    transport: UdpTransport,
    device: Mid360,
    source: Source,
    pending: Option<Lvx2Packet>,
    first: u64,
}
impl VirtualDevice {
    pub fn bind(options: ReplayOptions) -> Result<Self, SimError> {
        let valid = |ip: Ipv4Addr| !ip.is_unspecified() && !ip.is_multicast() && !ip.is_broadcast();
        if !valid(options.bind)
            || !valid(options.host)
            || options.point_port == 0
            || options.imu_port == 0
            || options.announce_port == Some(0)
        {
            return Err(SimError::InvalidAddress);
        }
        replay_deadline(0, 1, options.speed)?;
        if options.max_packets == Some(0) || options.run_for == Some(Duration::ZERO) {
            return Err(SimError::InvalidTiming);
        }
        let mut source = Source::open(&options)?;
        let pending = source.next()?.ok_or(SimError::Empty)?;
        encode_record(&pending, options.point_interval_ns)?;
        let first = pending.timestamp;
        let transport = UdpTransport::bind(options.bind, options.ports)?;
        let ports = transport.ports();
        let endpoint = |destination_port, source_port| HostEndpoint {
            ip: options.host.octets(),
            destination_port,
            source_port,
        };
        let device = Mid360 {
            ip: options.bind,
            serial: *b"VIRTUALMID36001\0",
            command_port: ports.command,
            point_format: pending.data_type,
            status_endpoint: endpoint(56201, ports.status),
            point_endpoint: endpoint(options.point_port, ports.point),
            imu_endpoint: endpoint(options.imu_port, ports.imu),
            work_mode: if options.start { 1 } else { 2 },
        };
        Ok(Self {
            options,
            transport,
            device,
            source,
            pending: Some(pending),
            first,
        })
    }
    pub fn ports(&self) -> Ports {
        self.transport.ports()
    }
    pub fn run(&mut self, stop: &AtomicBool) -> Result<RunStats, SimError> {
        let start = Instant::now();
        let mut previous = start;
        let mut active = Duration::ZERO;
        let mut was_sampling = self.device.sampling();
        let mut announce_at = Duration::ZERO;
        let mut stats = RunStats::default();
        // Full UDP receive capacity prevents truncated oversized datagrams looking valid.
        let mut buffer = vec![0; 65535];
        loop {
            if stop.load(Ordering::Relaxed) {
                return Ok(stats);
            }
            let now = Instant::now();
            let elapsed = now.duration_since(start);
            if self.options.run_for.is_some_and(|d| elapsed >= d) {
                return Ok(stats);
            }
            if was_sampling {
                active = active
                    .checked_add(now.duration_since(previous))
                    .ok_or(SimError::InvalidTiming)?;
            }
            previous = now;
            for channel in [CommandChannel::Discovery, CommandChannel::Control] {
                for _ in 0..64 {
                    let Some((n, peer)) = self.transport.receive(channel, &mut buffer)? else {
                        break;
                    };
                    let request = match CommandPacket::decode(&buffer[..n]) {
                        Ok(p) => p,
                        Err(_) => {
                            stats.malformed_dropped += 1;
                            continue;
                        }
                    };
                    if channel == CommandChannel::Discovery && request.header.cmd_id != 0 {
                        continue;
                    }
                    if let Some(response) = self.device.handle(&request)? {
                        let mut out = Vec::new();
                        response.encode(&mut out)?;
                        self.transport.reply(channel, &out, peer)?;
                        stats.commands_replied += 1;
                    }
                }
            }
            if elapsed >= announce_at {
                if let Some(port) = self.options.announce_port {
                    let mut bytes = Vec::new();
                    self.device.discovery(0)?.encode(&mut bytes)?;
                    self.transport.reply(
                        CommandChannel::Discovery,
                        &bytes,
                        SocketAddrV4::new(self.options.host, port),
                    )?;
                }
                announce_at = elapsed
                    .checked_add(Duration::from_secs(1))
                    .ok_or(SimError::InvalidTiming)?;
            }
            was_sampling = self.device.sampling();
            if self.device.sampling() {
                let Some(p) = &self.pending else {
                    return Ok(stats);
                };
                let deadline = replay_deadline(self.first, p.timestamp, self.options.speed)?;
                if active >= deadline {
                    let bytes = encode_record(p, self.options.point_interval_ns)?;
                    let e = self.device.point_endpoint;
                    self.transport
                        .send_point(&bytes, SocketAddrV4::new(e.ip.into(), e.destination_port))?;
                    stats.points_sent += 1;
                    if self
                        .options
                        .max_packets
                        .is_some_and(|n| stats.points_sent >= n)
                    {
                        return Ok(stats);
                    }
                    self.pending = self.source.next()?;
                    if self.pending.is_none() {
                        return Ok(stats);
                    }
                    continue;
                }
                std::thread::sleep((deadline - active).min(Duration::from_millis(1)));
            } else {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }
}
