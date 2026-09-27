# livox-rs — safe Livox protocols and recording readers

Incremental Rust implementation, prioritizing **Mid-360 / Mid-360S / Mid-360L**.
Pure Rust 2024, no unsafe in project Rust code and no Livox SDK FFI.
Both official LVX1 and LVX2 specifications and sample recordings were found.
Because Mid-360 uses LVX2, the verified LVX2 2.0 format has its own reader in
addition to the originally requested LVX1 1.1 reader.

## Workspace

```text
livox-protocol     command + SDK2 point/IMU codecs; separate legacy SDK1 types
      ↑
livox-lvx          independent LVX1 1.1 and LVX2 2.0 readers (Read + Seek)
      ↑
lvx-info           offline inspection CLI

livox-transport   std::net UDP sockets
livox-device      minimal Mid-360 command state
livox-sim         LVX2 replay clock + orchestration
virtual-lidar     livox-virtual-device CLI
livox-viewer      LVX2 / UDP → local HTTP → bundled WebGL point-cloud viewer
```

Fixed headers use **Serde + bincode 2**, explicitly configured for fixed-width
little-endian fields, exact byte consumption, and a small decoding limit.
Private wire schemas contain only fixed arrays and primitives. Public enums and
Vecs are never blindly deserialized from the wire. Payload bounds, CRCs and
frame offsets are checked explicitly; point values are decoded with safe LE
conversions. `thiserror` provides classified errors; `tracing` emits library
warnings for nonstandard frame durations; the CLI uses `clap`.

Transport, device behavior and replay are separate crates. Future Python/C
bindings will depend on a high-level Rust API. No C++ class hierarchy
was ported and no speculative traits or empty future crates were created.

## Use

```sh
cargo run --release -p lvx-info -- capture.lvx2
cargo run --release -p lvx-info -- capture.lvx
cargo run --release -p lvx-info -- capture.lvx2 --header-only
cargo test --release --workspace
cargo clippy --release --workspace --all-targets -- -D warnings
```

The default CLI scans all frames; this is linear I/O, not a cheap header count.
`--header-only` avoids scanning. Statistics are packet counts. First/last
observed timestamps are grouped by device and clock type, never merged across
unrelated clocks. Legacy UTC timestamps stay raw. `--max-frame-bytes` changes
the default 64 MiB per-frame limit. Files need no network access.

```rust,no_run
use std::{fs::File, io::BufReader};
use livox_lvx::Lvx2Reader;
# fn main() -> Result<(), Box<dyn std::error::Error>> {
let mut reader = Lvx2Reader::new(BufReader::new(File::open("capture.lvx2")?))?;
for frame in reader.frames() {
    for packet in frame?.packets {
        println!("{} {:?}", packet.timestamp, packet.payload()?);
    }
}
# Ok(()) }
```

`LvxReader` handles LVX1 only; `Lvx2Reader` handles LVX2 only. Use
`detect_format` on the 24-byte public header to dispatch. Frames allocate owned
packets; readers iterate once, continuing from the current position and ending
permanently after an error. Extrinsic metadata is exposed but never applied.

## Verified scope and limits

- SDK2 command v0: 24-byte framing, both CRCs, owned encoding, unknown enum
  preservation. Full 32-bit sequences preserved; upstream Pack truncates to 16.
- SDK2 data v0: 36-byte header, timestamp+payload CRC, typed IMU/high/low/spherical
  data, unaligned input. Type 0x11 remains opaque until count semantics are
  verified for the intended device. No CRC bypass.
- LVX1 1.1: device metadata, bounded frames, SDK1 packet types 0..8. Unknown types
  cannot be skipped because records have no length field; classified error.
  Versions such as LVX1 1.0 are explicitly unsupported.
- LVX2 2.0: independent 63-byte device/27-byte record parsing, typed Cartesian
  1/2 payloads, opaque unknown payloads with validated lengths. The 2023 PDF
  does not specify IMU/spherical/newer-device extensions; these are not guessed.
- Full official Mid-360 indoor recording passed: **1534 frames, 162293 packets**.
  The official Tele-15 first frame and C++-generated fixtures are also tested.
- Minimal virtual Mid-360: directed discovery, firmware query, destination
  settings, sampling/pause/resume and paced LVX2 replay over UDP. SDK2 callbacks
  verified with a macOS-only upstream socket workaround; see the full limits
  and reproduction steps in [virtual-device.md](docs/virtual-device.md).
- No physical Mid360S/L emulation, IMU synthesis, full broadcast fidelity, async,
  PyO3/maturin, C ABI or cbindgen yet.
- Serde schemas depend on field order: offset tables, C++ fixtures and real
  recordings guard against changes. This is not a generic bincode file format.
- Memory limits bound frame bytes, not total process RSS; parsed packet objects
  add overhead. Standard allocation failure/OS termination is distinct from the
  tested malformed-input no-panic invariant.

## Virtual Mid-360

```sh
cargo run --release -p virtual-lidar --bin livox-virtual-device -- replay capture.lvx2 \
  --bind 127.0.0.1 --host 127.0.0.1 --point-port 56301 --start --no-announce
```

For SDK-controlled operation, omit `--start`, use distinct assigned device/host
IPs, and optionally add `--loop`. See [usage and interoperability notes](docs/virtual-device.md).
The transport uses `std::net`; there are no native SDK bindings.

## Point-cloud viewer

```sh
cargo run --release -p livox-viewer -- file fixtures/official-mid360-first-frame.lvx2
cargo run --release -p livox-viewer -- file capture.lvx2 --history 5
cargo run --release -p livox-viewer -- udp --bind 127.0.0.1:56301
```

Open the printed URL (default `http://127.0.0.1:8080`) in Chrome. The English UI
uses a simple, light Material-inspired layout: point cloud, playback controls,
and collapsible display settings. Drag to orbit, Shift-drag to pan, scroll to
zoom; color by height, reflectivity or range. HTML/JS are embedded in the Rust
binary with no external CDN, npm runtime or added graphics `sys` crates.
After a UI update, restart the rebuilt binary and reload the browser.

The viewer currently displays Cartesian high/low SDK2 data and LVX2 2.0 only.
It retains bounded recent history in sensor coordinates; no extrinsics, motion
compensation, device configuration or LVX1 visualization yet. See
[viewer usage and limits](docs/viewer.md) for UDP replay and browser tests.

## Documentation and tests

- [Control protocol](docs/control-protocol.md)
- [Point/IMU protocol and Mid-360 family differences](docs/point-protocol.md)
- [LVX1](docs/lvx1.md) / [LVX2](docs/lvx2.md)
- [Official samples, hashes and C++ oracle provenance](docs/fixtures.md)
- [Mutation fuzz targets](fuzz/README.md) — pure Rust, no native fuzz dependency

Next: run end-to-end with unmodified SDK2 on Linux, capture real Mid-360 UDP
traffic to compare hardware behavior, and investigate Mid360S/L and LVX2 IMU
extensions before expanding the supported device profile.

Rust 1.94 was used for validation; declared MSRV 1.85 matches edition/dependency
requirements but has not been separately tested. On this macOS host the Xcode
shim was unavailable; checks used the installed Command Line Tools directly:

```sh
SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk \
CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER=/Library/Developer/CommandLineTools/usr/bin/clang \
cargo test --release --workspace
```

No system Xcode settings or licenses were modified.
