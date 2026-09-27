#![forbid(unsafe_code)]
use clap::{Parser, Subcommand};
use livox_sim::{ReplayOptions, VirtualDevice};
use livox_transport::Ports;
use std::{net::Ipv4Addr, path::PathBuf, sync::atomic::AtomicBool, time::Duration};
#[derive(Parser)]
#[command(about = "Minimal virtual Mid-360 using SDK2 UDP commands and LVX2 replay")]
struct Cli {
    #[command(subcommand)]
    command: Action,
}
#[derive(Subcommand)]
enum Action {
    Replay(Replay),
}
#[derive(clap::Args)]
struct Replay {
    capture: PathBuf,
    #[arg(long, default_value = "127.0.0.1")]
    bind: Ipv4Addr,
    #[arg(long, default_value = "127.0.0.1")]
    host: Ipv4Addr,
    #[arg(long)]
    device_id: Option<u32>,
    #[arg(long, default_value_t = 56000)]
    discovery_port: u16,
    #[arg(long, default_value_t = 56100)]
    command_port: u16,
    #[arg(long, default_value_t = 56300)]
    point_source_port: u16,
    #[arg(long, default_value_t = 56301)]
    point_port: u16,
    #[arg(long, default_value_t = 56401)]
    imu_port: u16,
    /// Send periodic unicast discovery ACKs to this SDK host port.
    #[arg(long, default_value_t = 56000)]
    announce_port: u16,
    #[arg(long)]
    no_announce: bool,
    /// Start immediately instead of waiting for SDK sampling command.
    #[arg(long)]
    start: bool,
    /// Repeat with monotonically advanced packet timestamps.
    #[arg(long = "loop")]
    looping: bool,
    #[arg(long, default_value_t = 1.)]
    speed: f64,
    /// Missing LVX2 intra-packet spacing; zero means explicitly unknown.
    #[arg(long, default_value_t = 0)]
    point_interval_ns: u64,
    #[arg(long)]
    max_packets: Option<u64>,
    /// Exit after this many wall-clock seconds, including idle time.
    #[arg(long)]
    run_for: Option<f64>,
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let Action::Replay(r) = Cli::parse().command;
    let run_for = r.run_for.map(Duration::try_from_secs_f64).transpose()?;
    let mut device = VirtualDevice::bind(ReplayOptions {
        path: r.capture,
        device_id: r.device_id,
        bind: r.bind,
        host: r.host,
        ports: Ports {
            discovery: r.discovery_port,
            command: r.command_port,
            point: r.point_source_port,
            ..Ports::default()
        },
        point_port: r.point_port,
        imu_port: r.imu_port,
        announce_port: if r.no_announce {
            None
        } else {
            Some(r.announce_port)
        },
        start: r.start,
        looping: r.looping,
        speed: r.speed,
        point_interval_ns: r.point_interval_ns,
        max_packets: r.max_packets,
        run_for,
    })?;
    let p = device.ports();
    println!(
        "ready: bind={} discovery={} command={} point_source={} virtual_sn=VIRTUALMID36001",
        r.bind, p.discovery, p.command, p.point
    );
    if r.point_interval_ns == 0 {
        eprintln!("note: LVX2 lacks intra-packet timing; UDP time_interval is zero (unknown)");
    }
    let stats = device.run(&AtomicBool::new(false))?;
    println!(
        "finished: packets={} replies={} malformed={}",
        stats.points_sent, stats.commands_replied, stats.malformed_dropped
    );
    Ok(())
}
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("livox-virtual-device: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
