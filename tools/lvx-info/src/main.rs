#![forbid(unsafe_code)]
use clap::Parser;
use livox_lvx::{Format, Lvx2Reader, LvxHeader, LvxReader, ReaderOptions, detect_format};
use livox_protocol::TimeType;
use std::{
    collections::BTreeMap,
    fs::File,
    io::{BufReader, Read, Seek, SeekFrom},
    path::PathBuf,
};
#[derive(Parser)]
#[command(about = "Inspect LVX1 1.1 and LVX2 2.0 recordings offline")]
struct Args {
    path: PathBuf,
    /// Read metadata only; default scans frames for packet counts and timestamps.
    #[arg(long)]
    header_only: bool,
    #[arg(long,default_value_t=livox_lvx::DEFAULT_MAX_FRAME_BYTES)]
    max_frame_bytes: u64,
}
fn serial(b: &[u8]) -> String {
    String::from_utf8_lossy(b.split(|x| *x == 0).next().unwrap_or_default())
        .escape_default()
        .to_string()
}
fn print_header(h: &LvxHeader) {
    println!(
        "format: LVX {}.{}\nversion: {}.{}.{}.{}\nsignature: {}\nmagic: {:#010x}\nframe_duration_ms: {}\ndevices: {}",
        h.version[0],
        h.version[1],
        h.version[0],
        h.version[1],
        h.version[2],
        h.version[3],
        serial(&h.signature),
        h.magic,
        h.frame_duration_ms,
        h.device_count
    );
    if h.frame_duration_ms != 50 {
        eprintln!("warning: frame duration differs from the published 50 ms");
    }
}
#[derive(Default)]
struct Stats {
    frames: u64,
    counts: BTreeMap<String, u64>,
    // File-order endpoints, separated by device and timestamp domain.
    times: BTreeMap<(u32, u8), (String, String)>,
}
impl Stats {
    fn packet(&mut self, kind: String, device: u32, time_type: u8, time: String) {
        *self.counts.entry(kind).or_default() += 1;
        self.times
            .entry((device, time_type))
            .and_modify(|v| v.1 = time.clone())
            .or_insert((time.clone(), time));
    }
    fn print(self) {
        println!("\nframes: {}\ndata types:", self.frames);
        for (kind, count) in self.counts {
            println!("  {kind}: {count} packets");
        }
        println!("timestamps (first/last in file order, per clock domain):");
        for ((device, kind), (first, last)) in self.times {
            println!("  device={device} time_type={kind}: {first} .. {last}");
        }
    }
}
fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = BufReader::new(File::open(args.path)?);
    let mut public = [0; 24];
    file.read_exact(&mut public)?;
    let format = detect_format(&public)?;
    file.seek(SeekFrom::Start(0))?;
    let options = ReaderOptions {
        max_frame_bytes: args.max_frame_bytes,
    };
    let mut stats = Stats::default();
    match format {
        Format::Lvx1_1 => {
            let mut reader = LvxReader::with_options(file, options)?;
            print_header(reader.header());
            for d in reader.devices() {
                println!(
                    "\ndevice[{}]:\n  type: {:?}\n  serial: {}\n  hub: {}\n  extrinsic_enabled: {}\n  rotation_deg: {:?}\n  translation_m: {:?}",
                    d.device_index,
                    d.device_type,
                    serial(&d.lidar_broadcast_code),
                    serial(&d.hub_broadcast_code),
                    d.extrinsic_enabled,
                    d.rotation_deg,
                    d.translation_m
                );
            }
            if !args.header_only {
                for frame in reader.frames() {
                    let frame = frame?;
                    stats.frames += 1;
                    for p in frame.packets {
                        let time = p
                            .header
                            .timestamp_ns()
                            .map(|t| format!("{t} ns"))
                            .unwrap_or_else(|| format!("{:02x?} raw", p.header.timestamp));
                        stats.packet(
                            format!("{:?}", p.header.data_type),
                            u32::from(p.device_index),
                            p.header.timestamp_type,
                            time,
                        );
                    }
                }
            }
        }
        Format::Lvx2 => {
            let mut reader = Lvx2Reader::with_options(file, options)?;
            print_header(reader.header());
            for d in reader.devices() {
                println!(
                    "\ndevice[{}]:\n  type: {:?}\n  serial: {}\n  hub_raw: {:02x?}\n  lidar_type_reserved: {}\n  extrinsic_enabled: {}\n  rotation_deg: {:?}\n  translation_m: {:?}",
                    d.lidar_id,
                    d.device_type,
                    serial(&d.lidar_serial),
                    d.hub_serial,
                    d.lidar_type,
                    d.extrinsic_enabled,
                    d.rotation_deg,
                    d.translation_m
                );
            }
            if !args.header_only {
                for frame in reader.frames() {
                    let frame = frame?;
                    stats.frames += 1;
                    for p in frame.packets {
                        let kind = match p.data_type {
                            1 => "CartesianHigh".into(),
                            2 => "CartesianLow".into(),
                            x => format!("Unknown({x})"),
                        };
                        let (tt, unit) = match p.time_type {
                            TimeType::Uptime => (0, "ns"),
                            TimeType::Ptp => (1, "ns"),
                            TimeType::Gps => (2, "ns"),
                            TimeType::Unknown(x) => (x, "raw u64"),
                        };
                        stats.packet(kind, p.lidar_id, tt, format!("{} {unit}", p.timestamp));
                    }
                }
            }
        }
        unknown => return Err(livox_lvx::LvxError::UnsupportedFormat(unknown).into()),
    }
    if args.header_only {
        println!("\nframes: not scanned");
    } else {
        stats.print();
    }
    Ok(())
}
fn main() -> std::process::ExitCode {
    match run(Args::parse()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("lvx-info: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
