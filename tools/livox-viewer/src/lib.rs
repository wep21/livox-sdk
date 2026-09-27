//! Presentation adapter. Livox parsing remains in the protocol/file crates.
#![forbid(unsafe_code)]
pub mod http;

use livox_lvx::{Lvx2Reader, LvxError, lvx2::Lvx2Payload};
use livox_protocol::{DataPacket, DataPayload};
use serde::Serialize;
use std::{
    collections::VecDeque,
    fs::File,
    io::BufReader,
    net::{IpAddr, SocketAddr, UdpSocket},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("LVX2: {0}")]
    Lvx(#[from] LvxError),
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("select one device with --device-id; available IDs: {0:?}")]
    SelectDevice(Vec<u32>),
    #[error("device ID {0} is absent from this recording")]
    MissingDevice(u32),
    #[error("frame duration is zero")]
    ZeroFrameDuration,
    #[error("viewer limits must be positive")]
    InvalidLimit,
    #[error("receiver state lock failed")]
    ReceiverLock,
}

/// Metres in the device coordinate system and raw reflectivity (0..255).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point(pub [f32; 4]);

fn point(xyz: [f32; 3], reflectivity: u8) -> Option<Point> {
    (xyz.iter().all(|x| x.is_finite()) && xyz != [0.; 3]).then_some(Point([
        xyz[0],
        xyz[1],
        xyz[2],
        f32::from(reflectivity),
    ]))
}

fn high(points: livox_protocol::CartesianHighPoints<'_>) -> Vec<Point> {
    points
        .iter()
        .filter_map(|p| point(p.xyz_mm.map(|v| v as f32 * 0.001), p.reflectivity))
        .collect()
}

fn low(points: livox_protocol::CartesianLowPoints<'_>) -> Vec<Point> {
    points
        .iter()
        .filter_map(|p| point(p.xyz_cm.map(|v| f32::from(v) * 0.01), p.reflectivity))
        .collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct Info {
    pub mode: &'static str,
    pub source: String,
    pub device_id: Option<u32>,
    pub serial: Option<String>,
    pub frame_duration_ms: u32,
    pub max_points: usize,
    pub history_frames: usize,
    pub window_ms: u64,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct Status {
    pub frame: Option<u64>,
    /// String preserves nanosecond timestamps beyond JavaScript's integer range.
    pub timestamp: Option<String>,
    pub time_type: Option<String>,
    pub packets: u64,
    pub skipped: u64,
    pub invalid: u64,
    pub foreign: u64,
    pub points: usize,
    pub ended: bool,
    pub peer: Option<String>,
    pub error: Option<String>,
}

struct Batch {
    arrival: Instant,
    points: VecDeque<Point>,
}

/// A bounded recent history; exact FIFO truncation keeps the newest points.
pub struct Cloud {
    batches: VecDeque<Batch>,
    max_points: usize,
    count: usize,
    pub status: Status,
}

impl Cloud {
    pub fn new(max_points: usize) -> Result<Self, Error> {
        if max_points == 0 {
            return Err(Error::InvalidLimit);
        }
        Ok(Self {
            batches: VecDeque::new(),
            max_points,
            count: 0,
            status: Status::default(),
        })
    }

    pub fn push(&mut self, points: Vec<Point>, now: Instant, max_batches: usize) {
        // Empty file frames still advance the history window.
        let skip = points.len().saturating_sub(self.max_points);
        let points: VecDeque<_> = points.into_iter().skip(skip).collect();
        self.count += points.len();
        self.batches.push_back(Batch {
            arrival: now,
            points,
        });
        while self.batches.len() > max_batches.max(1) {
            self.pop();
        }
        while self.count > self.max_points {
            let excess = self.count - self.max_points;
            if let Some(front) = self.batches.front_mut() {
                let n = excess.min(front.points.len());
                front.points.drain(..n);
                self.count -= n;
                if front.points.is_empty() {
                    self.pop();
                }
            } else {
                break;
            }
        }
        self.status.points = self.count;
    }

    fn pop(&mut self) {
        if let Some(batch) = self.batches.pop_front() {
            self.count -= batch.points.len();
        }
    }

    pub fn expire(&mut self, now: Instant, window: Duration) {
        while self
            .batches
            .front()
            .is_some_and(|b| now.saturating_duration_since(b.arrival) > window)
        {
            self.pop();
        }
        self.status.points = self.count;
    }

    /// Local UI format: u32 LE JSON byte count, UTF-8 metadata, then xyzi f32 LE.
    pub fn encode(&self) -> Result<Vec<u8>, Error> {
        let json = serde_json::to_vec(&self.status)?;
        let mut out = Vec::with_capacity(4 + json.len() + self.count * 16);
        out.extend((json.len() as u32).to_le_bytes());
        out.extend(json);
        for batch in &self.batches {
            for point in &batch.points {
                for value in point.0 {
                    out.extend(value.to_le_bytes());
                }
            }
        }
        Ok(out)
    }
}

pub struct FileSource {
    path: PathBuf,
    reader: Lvx2Reader<BufReader<File>>,
    pub info: Info,
    cloud: Cloud,
    device_id: u32,
}

impl FileSource {
    pub fn open(
        path: &Path,
        device_id: Option<u32>,
        max_points: usize,
        history: usize,
    ) -> Result<Self, Error> {
        if history == 0 {
            return Err(Error::InvalidLimit);
        }
        let reader = Lvx2Reader::new(BufReader::new(File::open(path)?))?;
        let ids: Vec<_> = reader.devices().iter().map(|d| d.lidar_id).collect();
        let id = match device_id {
            Some(id) => id,
            None if ids.len() == 1 => ids[0],
            None => return Err(Error::SelectDevice(ids)),
        };
        let d = reader
            .devices()
            .iter()
            .find(|d| d.lidar_id == id)
            .ok_or(Error::MissingDevice(id))?;
        let serial =
            String::from_utf8_lossy(d.lidar_serial.split(|b| *b == 0).next().unwrap_or_default())
                .into_owned();
        let duration = reader.header().frame_duration_ms;
        if duration == 0 {
            return Err(Error::ZeroFrameDuration);
        }
        Ok(Self {
            info: Info {
                mode: "file",
                source: path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                device_id: Some(id),
                serial: Some(serial),
                frame_duration_ms: duration,
                max_points,
                history_frames: history,
                window_ms: 0,
            },
            path: path.to_owned(),
            reader,
            cloud: Cloud::new(max_points)?,
            device_id: id,
        })
    }

    pub fn advance(&mut self) -> Result<Vec<u8>, Error> {
        if self.cloud.status.error.is_some() || self.cloud.status.ended {
            return self.cloud.encode();
        }
        match self.reader.frames().next() {
            Some(Ok(frame)) => {
                self.cloud.status.frame = Some(frame.index);
                let mut points = VecDeque::new();
                for p in frame
                    .packets
                    .into_iter()
                    .filter(|p| p.lidar_id == self.device_id)
                {
                    self.cloud.status.timestamp = Some(p.timestamp.to_string());
                    self.cloud.status.time_type = Some(format!("{:?}", p.time_type));
                    let decoded = match p.payload()? {
                        Lvx2Payload::CartesianHigh(p) => high(p),
                        Lvx2Payload::CartesianLow(p) => low(p),
                        Lvx2Payload::Unknown { .. } => {
                            self.cloud.status.skipped += 1;
                            continue;
                        }
                    };
                    self.cloud.status.packets += 1;
                    points.extend(decoded);
                    let excess = points.len().saturating_sub(self.info.max_points);
                    points.drain(..excess);
                }
                self.cloud
                    .push(points.into(), Instant::now(), self.info.history_frames);
            }
            Some(Err(e)) => {
                self.cloud.status.error = Some(e.to_string());
                self.cloud.status.ended = true;
            }
            None => self.cloud.status.ended = true,
        }
        self.cloud.encode()
    }

    pub fn snapshot(&self) -> Result<Vec<u8>, Error> {
        self.cloud.encode()
    }

    pub fn reset(&mut self) -> Result<Vec<u8>, Error> {
        *self = Self::open(
            &self.path,
            Some(self.device_id),
            self.info.max_points,
            self.info.history_frames,
        )?;
        self.advance()
    }
}

pub struct LiveState {
    pub cloud: Cloud,
    source_ip: Option<IpAddr>,
    peer: Option<SocketAddr>,
}

impl LiveState {
    pub fn new(max_points: usize, source_ip: Option<IpAddr>) -> Result<Self, Error> {
        Ok(Self {
            cloud: Cloud::new(max_points)?,
            source_ip,
            peer: None,
        })
    }

    pub fn receive(&mut self, bytes: &[u8], peer: SocketAddr, now: Instant) {
        if self.source_ip.is_some_and(|ip| ip != peer.ip()) || self.peer.is_some_and(|p| p != peer)
        {
            self.cloud.status.foreign += 1;
            return;
        }
        let packet = match DataPacket::decode(bytes) {
            Ok(p) => p,
            Err(_) => {
                self.cloud.status.invalid += 1;
                return;
            }
        };
        let points = match packet.payload {
            DataPayload::CartesianHigh(p) => high(p),
            DataPayload::CartesianLow(p) => low(p),
            _ => {
                self.cloud.status.skipped += 1;
                return;
            }
        };
        self.peer = Some(peer);
        self.cloud.status.peer = Some(peer.to_string());
        self.cloud.status.packets += 1;
        self.cloud.status.timestamp = Some(packet.header.timestamp.to_string());
        self.cloud.status.time_type = Some(format!("{:?}", packet.header.time_type));
        // Empty packets don't allocate history nodes; prevent an empty-packet flood.
        if !points.is_empty() {
            self.cloud.push(points, now, self.cloud.max_points);
        }
    }
}

pub struct LiveSource {
    pub info: Info,
    state: Arc<Mutex<LiveState>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl LiveSource {
    pub fn bind(
        bind: SocketAddr,
        source_ip: Option<IpAddr>,
        max_points: usize,
        window_ms: u64,
    ) -> Result<Self, Error> {
        if window_ms == 0 {
            return Err(Error::InvalidLimit);
        }
        let socket = UdpSocket::bind(bind)?;
        socket.set_read_timeout(Some(Duration::from_millis(20)))?;
        let source = socket.local_addr()?.to_string();
        let state = Arc::new(Mutex::new(LiveState::new(max_points, source_ip)?));
        let stop = Arc::new(AtomicBool::new(false));
        let shared = Arc::clone(&state);
        let stopping = Arc::clone(&stop);
        let worker = thread::spawn(move || {
            let mut bytes = [0u8; 65536];
            while !stopping.load(Ordering::Relaxed) {
                let received = socket.recv_from(&mut bytes);
                let Ok(mut state) = shared.lock() else {
                    break;
                };
                let now = Instant::now();
                state.cloud.expire(now, Duration::from_millis(window_ms));
                match received {
                    Ok((n, peer)) => state.receive(&bytes[..n], peer, now),
                    Err(e)
                        if matches!(
                            e.kind(),
                            std::io::ErrorKind::WouldBlock
                                | std::io::ErrorKind::TimedOut
                                | std::io::ErrorKind::Interrupted
                        ) => {}
                    Err(e) => {
                        state.cloud.status.error = Some(e.to_string());
                        break;
                    }
                }
            }
        });
        Ok(Self {
            info: Info {
                mode: "udp",
                source,
                device_id: None,
                serial: None,
                frame_duration_ms: 50,
                max_points,
                history_frames: 0,
                window_ms,
            },
            state,
            stop,
            worker: Some(worker),
        })
    }

    pub fn snapshot(&self) -> Result<Vec<u8>, Error> {
        self.state
            .lock()
            .map_err(|_| Error::ReceiverLock)?
            .cloud
            .encode()
    }
}

impl Drop for LiveSource {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub enum Source {
    File(Box<FileSource>),
    Udp(LiveSource),
}
impl Source {
    pub fn info(&self) -> &Info {
        match self {
            Self::File(f) => &f.info,
            Self::Udp(u) => &u.info,
        }
    }
    pub fn snapshot(&self) -> Result<Vec<u8>, Error> {
        match self {
            Self::File(f) => f.snapshot(),
            Self::Udp(u) => u.snapshot(),
        }
    }
    pub fn advance(&mut self) -> Result<Vec<u8>, Error> {
        match self {
            Self::File(f) => f.advance(),
            Self::Udp(u) => u.snapshot(),
        }
    }
    pub fn reset(&mut self) -> Result<Vec<u8>, Error> {
        match self {
            Self::File(f) => f.reset(),
            Self::Udp(u) => u.snapshot(),
        }
    }
}
