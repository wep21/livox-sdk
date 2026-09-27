# Test organization

`cargo test --release --workspace` runs deterministic integration tests in:

- `crates/livox-protocol/tests`: command/point/IMU codecs and C++ byte oracles.
- `crates/livox-lvx/tests`: LVX1/LVX2 readers, official captures, malformed frames.
- `tools/lvx-info/tests`: offline CLI output and failure behavior.

`tests/upstream` contains developer-only C++ oracle adapters. They are not linked
into any Rust crate and are not required for ordinary tests. Regeneration pins
are recorded in `docs/fixtures.md`. `fuzz/` contains separate pure-Rust mutation
targets with no native fuzz engine.

Virtual-device additions:
- `crates/livox-device/tests`: minimal Mid-360 command state and atomic changes.
- `crates/livox-sim/tests`: recording-to-UDP reconstruction and replay deadlines.
- `tools/virtual-lidar/tests`: real UDP discovery/config/start/pause/loop tests.
- `tests/upstream/virtual_device_client.cpp` + `run_virtual_device.py`: full SDK2
  public-API interoperability client. See `docs/virtual-device.md` for the explicit
  macOS-only test-copy patch and the unmodified Linux verification gap.
