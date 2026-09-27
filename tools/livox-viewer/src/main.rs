#![forbid(unsafe_code)]
use clap::{Parser, Subcommand};
use livox_viewer::{FileSource, LiveSource, Source, http};
use std::{
    net::{IpAddr, SocketAddr, TcpListener},
    path::PathBuf,
};

#[derive(Parser)]
#[command(about = "Offline WebGL point-cloud viewer for Mid-360 LVX2 and SDK2 UDP")]
struct Cli {
    /// Loopback HTTP port. Zero chooses an available port.
    #[arg(long, global = true, default_value_t = 8080)]
    http_port: u16,
    /// Maximum retained/displayed points, independently of file length.
    #[arg(long, global = true, default_value_t = 200_000, value_parser = clap::value_parser!(u32).range(1..=2_000_000))]
    max_points: u32,
    #[command(subcommand)]
    command: Action,
}

#[derive(Subcommand)]
enum Action {
    /// Play LVX2 2.0 in file-frame order; no whole-file preload.
    File {
        capture: PathBuf,
        #[arg(long)]
        device_id: Option<u32>,
        /// Number of recent file frames overlaid in sensor coordinates.
        #[arg(long, default_value_t = 5, value_parser = clap::value_parser!(u32).range(1..=200))]
        history: u32,
    },
    /// Listen for point datagrams; this does not configure/start a real device.
    Udp {
        #[arg(long, default_value = "127.0.0.1:56301")]
        bind: SocketAddr,
        /// Optional sender IP filter; first valid Cartesian sender is then locked.
        #[arg(long)]
        source_ip: Option<IpAddr>,
        /// Keep points received within this many milliseconds.
        #[arg(long, default_value_t = 250, value_parser = clap::value_parser!(u32).range(1..=10_000))]
        window_ms: u32,
    },
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = Cli::parse();
    let max = args.max_points as usize;
    let source = match args.command {
        Action::File {
            capture,
            device_id,
            history,
        } => Source::File(Box::new(FileSource::open(
            &capture,
            device_id,
            max,
            history as usize,
        )?)),
        Action::Udp {
            bind,
            source_ip,
            window_ms,
        } => Source::Udp(LiveSource::bind(
            bind,
            source_ip,
            max,
            u64::from(window_ms),
        )?),
    };
    let listener = TcpListener::bind(("127.0.0.1", args.http_port))?;
    println!("viewer: http://{}", listener.local_addr()?);
    println!("source: {} {}", source.info().mode, source.info().source);
    println!("Open the URL in a WebGL-capable browser. Stop with Ctrl-C.");
    http::serve(listener, source)?;
    Ok(())
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("livox-viewer: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
