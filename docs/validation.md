# First iteration validation

Environment: Rust 1.94.0, macOS arm64; 2026-09-27.

| Check | Result |
|---|---|
| Workspace tests | 22 passed (8 protocol, 12 file reader, 2 CLI) |
| Upstream SDK2 C++ adapter | Compiled/run against pinned unmodified SDK2; generated command/high/IMU fixtures |
| Upstream SDK1 C++ adapter | Compiled/run against pinned writer; all nine legacy types, extrinsics and empty frame |
| Official Mid-360 full LVX2 | Rust CLI: 1534 frames, 162293 high Cartesian packets; end exactly at EOF |
| Official Tele-15 LVX1 excerpt | One complete frame, 125 point packets + 10 IMU packets |
| Serde | Fixed headers validated against byte offsets and C++/real-file fixtures |
| Unsafe / native bindings | Project core crates forbid unsafe; no Livox SDK FFI |

Malformed-input tests cover truncation, lengths below/above buffer bounds, CRC16,
CRC32 (including timestamp corruption), unknown variants, bad signature/magic,
unsupported versions, duplicate/unknown device IDs, backward/oversized/overflow
frame offsets, incomplete final frames, point count mismatch, payload truncation,
frame allocation ceilings, and iterator exhaustion after error. Unaligned
buffers are exercised. Command encoding is compared byte-for-byte with SDK2
and checked for round trips, 32-bit sequence preservation, maximum length and
error atomicity. Handwritten low/spherical values complement SDK fixtures.

The separate pure-Rust mutation targets exercise command decode/re-encode,
point decode/iteration, both reader constructors/frame iterators, and both
standalone frame parsers. Some mutations repair CRC/length/offset fields to
exercise deeper paths. These are smoke fuzzing runs, not coverage-guided fuzzing
or a proof of safety for every possible byte sequence. See [fuzz](../fuzz/README.md).

Known verification gaps: physical Mid360S/L, raw network CRC compliance, LVX2
IMU/extensions, other platforms/MSRV, and long-running coverage-guided fuzzing.
Do not infer SDK2 network interoperability from successful LVX2 file parsing.

Final checks: workspace Clippy with `-D warnings` passed; workspace and fuzz
format checks passed; all seven fixture SHA-256 checks passed. Each of the four
mutation targets completed **10000 iterations** (40000 total) without panic.

## Virtual-device iteration

Workspace tests now total **32**: 10 protocol, 12 file, 4 command state, 2 replay,
2 inspection CLI and 2 actual UDP integration tests. All passed on macOS with
local socket access enabled. Workspace Clippy with `-D warnings` passed.
The SDK public-API integration client reported **connected=1, started=1,
points=104, invalid=0** using the documented two-site macOS broadcast-bind
workaround in a separate test copy. Unmodified SDK2 built successfully but
failed initialization on this host at its broadcast bind, independently of
the Rust protocol code. This is not an unmodified-Linux interoperability claim.
See [virtual-device.md](virtual-device.md) for exact scope and reproduction.

The virtual-device iteration also ran 10000 command/TLV mutations and 10000
point decode/encode mutations without panic.

## Viewer iteration

Release workspace tests total **38** (the previous 32 plus 4 cloud tests and
2 HTTP/UDP integration tests). They passed on macOS with local socket access.
The official Mid-360 first frame displays 6407 nonzero-coordinate points from
104 packets; viewer tests compare all displayed values to the recording and
check identical conversion through reconstructed UDP packets.

Chrome reproduced the initial shader link failure:
`Precisions of uniform 'u_grid' differ between VERTEX and FRAGMENT shaders.`
Both shader stages now explicitly declare the shared uniform and color varying
with matching precision. Chrome 153 in a normal window passed actual linking,
drawing, playback/pause/restart, display settings and desktop/mobile layouts.
Live UDP display also passed in Chrome using the virtual device replaying the
official first-frame fixture. Screenshots were visually checked.
Release Clippy with `-D warnings` and Rust
format checks passed. See [viewer.md](viewer.md) for the browser regression.
The project uses ordinary `cargo ... --release` commands; no justfile is kept.
