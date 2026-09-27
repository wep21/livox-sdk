# Virtual Mid-360: UDP recording replay

Scope: a minimal SDK2-facing **Mid-360 (type 9)** device, not full firmware or
Mid360S/L emulation. It uses the documented command wire format and replays
published LVX2 Cartesian types 1/2. It does not implement firmware updates,
network address changes, factory reset, scan geometry, IMU synthesis or bindings.

## Command mapping checked before implementation

Primary: [official Mid-360 family protocol](https://livox-wiki-en.readthedocs.io/en/latest/tutorials/new_product/mid360/livox_eth_protocol_mid360.html), commands 0x0000, 0x0100, 0x0101 and return codes.
SDK2 reference commit `c0796f04c143143899c87a773d9f6b7136453c0b`:
`sdk_core/command_handler/{general_command_handler.cpp,mid360_command_handler.cpp,build_request.cpp}`,
`sdk_core/device_manager.cpp`, `sdk_core/comm/define.h`.

All offsets below are **within command payload**, all multibyte values LE.

| Command | Direction | Offset:size:field | Validation/meaning |
|---|---|---|---|
| 0x0000 discovery | request | empty | Only empty payload accepted |
| 0x0000 | ACK | 0:1:return, 1:1:type, 2:16:serial, 18:4:IPv4, 22:2:command port | Type 9; serial is virtual-device identity |
| 0x0100 set | request | 0:2:count, 2:2:reserved, 4:variable:TLVs | Count must match entire payload; each TLV is key:u16, value length:u16, value bytes |
| 0x0100 | ACK | 0:1:return, 1:2:error key | 0/0 success; errors do not partially apply a batch |
| 0x0101 get | request | 0:2:count, 2:2:reserved, 4:count*2:keys | Exact length checked |
| 0x0101 | ACK | 0:1:return, 1:2:count, 3:variable:TLVs | Header has no reserved field |

| Key | Value width | Meaning / simulator behavior |
|---|---|---|
| 0x0000 | 1 | Point format; accepts only the recording's selected Cartesian format |
| 0x0005/6/7 | 8 | Status/point/IMU destinations: IPv4[4], destination u16, source u16; source must match simulator's fixed bound/configured port |
| 0x001A | 1 | Working mode: sampling=1, idle=2, ready=9; controls replay pause/resume |
| 0x001C | 1 | IMU enable; disabling accepted, enabling explicitly unsupported (no recorded IMU interpretation) |
| 0x8000 | 16 | Virtual serial |
| 0x8001 | 64 | Explicitly identifies virtual Mid-360 |
| 0x8002 | 4 | Synthetic application version, not a claimed hardware firmware |
| 0x8006 | 1 | Current simulated mode |
| 0x800E | 2 | Simulated healthy diagnostic status |
| 0x8010 | 1 | Application firmware mode (1), necessary for SDK2 connection |

Return codes used: 0 success, 3 out of range, 0x20 unsupported parameter,
0x22 read-only, 0x23 invalid length, 0x24 count/list mismatch. Unknown commands
are not acknowledged as successful. Incoming ACKs/LiDAR-origin commands and
malformed wire packets are ignored. ACKs preserve request sequence and cmd_id.

SDK2's configured-device path: discovery -> get 0x8010 -> subsequent discovery
-> set status/point/IMU endpoints -> info callback -> set sampling mode -> point
callback. This is the interoperability path targeted here. SDK2's Viewer mode,
all-parameter queries and full status push are outside the supported subset.

## Reconstruction policy

LVX2 preserves point payloads, device ID, first-point timestamp and UDP counter;
its frame counter is declared reserved. It does **not** preserve the 36-byte UDP header, time_interval,
CRC or reserved fields. Replay reconstructs those fields explicitly, never
casts a file record into a UDP packet. Reserved bytes and the unused Mid-360
non-repeating UDP frame counter become zero; CRC is
recomputed over timestamp + payload; point count is payload bytes / point size.
`--point-interval-ns` supplies the missing intra-packet point spacing (default
zero, explicitly unknown); interval = (count-1)*spacing in 100 ns units, checked
for exact representation and u16 range. No sensor timing claim is made for zero.
Unknown file data/time types and changes of point format are errors for replay,
even though the file reader can safely retain them.

Playback deadlines follow recorded timestamp differences using monotonic wall
clock and configurable speed. File timestamps are retained on the first pass.
Pausing does not consume samples. Looping advances the timestamp epoch so the
next cycle starts one declared file-frame duration after the previous last
packet. Regressing timestamps within one pass are rejected. Extrinsics are not
applied; the recorded coordinates are sent unchanged.

## Run

Simple UDP replay without an SDK handshake (stop after one pass):

```sh
cargo run --release -p virtual-lidar --bin livox-virtual-device -- replay capture.lvx2 \
  --bind 127.0.0.1 --host 127.0.0.1 --point-port 56301 --start --no-announce
```

SDK-facing mode, using addresses actually assigned to the two machines:

```sh
cargo run --release -p virtual-lidar --bin livox-virtual-device -- replay capture.lvx2 \
  --bind 192.168.1.10 --host 192.168.1.5 --loop
```

Without `--start`, no point data is sent until a sampling command arrives.
The SDK must configure MID360, the simulated LiDAR IP, and its own host IP.
It should call `SetLivoxLidarWorkMode(handle, kLivoxLidarNormal, ...)` from its
info-change callback. The latest upstream quick-start example also sends newer
model settings; these are intentionally not all supported by this minimal
Mid-360 emulator. Unsupported requests receive errors, not fake success.

Device source ports: discovery 56000, command 56100, point 56300. The point/IMU
host defaults are 56301/56401. Discovery and point source/command ports are
CLI-configurable for isolated tests; use standard source ports with SDK2,
which dispatches based on them. No IMU or status-push datagrams are emitted.
The source port encoded in a SET endpoint must match the simulator's port;
dynamic port rebinding and multicast are explicitly unsupported.

`--device-id` selects a recording device if there are several. Only Mid360
(device type 9) is currently accepted; Mid360S/L models are not advertised as
implemented. `--speed`, `--loop`, `--max-packets`, and `--run-for` control replay.
A zero/negative/nonfinite speed, arithmetic overflow, invalid point spacing,
empty selection, timestamp regression, unknown type or changing point format
produces an error. Standard Ctrl-C terminates the process; the library also
accepts an `AtomicBool` cancellation token.

## Discovery differences

This minimum implementation uses **directed discovery**: it responds to requests
on its bound IP and optionally sends a discovery ACK every second to the explicit
`--host` address, port 56000 (`--announce-port` overrides it). These unsolicited
announcements use sequence zero; ordinary replies echo the request sequence.
`--no-announce` disables periodic announcements. This is a simulator convenience,
not the documented hardware broadcast discovery behavior. We do not claim
broadcast discovery fidelity. The explicit announcement lets the SDK find it
without requiring wildcard sockets, raw sockets, socket-reuse FFI or sys crates.

For two processes on one machine, use **different local IP addresses**. SDK2
ignores datagrams whose source equals its configured host IP. Linux normally
allows distinct 127/8 loopback addresses; macOS requires addresses that are
actually assigned. No interface aliases or system settings are created by this
project. Tests on this macOS machine used the simulator at 127.0.0.1 and the SDK
bound to the machine's existing LAN IPv4 address; both processes were local.

## SDK interoperability evidence and reproduction

- Unmodified SDK2 commit `c0796f04c143143899c87a773d9f6b7136453c0b` compiled
  successfully, but SDK initialization failed on macOS because it tries to bind
  broadcast receive sockets to `255.255.255.255` (`EADDRNOTAVAIL`). This failure
  occurs before discovering the simulator.
- A **separate test copy** used
  [macos-directed-discovery.patch](../tests/upstream/macos-directed-discovery.patch)
  to omit only the discovery/status broadcast receive sockets, as its Windows
  branch already does. Command state machines, serializers, parsers, CRCs,
  callbacks, public API and data handlers were unchanged.
- The [public-API client](../tests/upstream/virtual_device_client.cpp) then observed
  `connected=1 started=1 points=104 invalid=0`. It validated device type, version,
  packet length, count and data CRC inside the actual SDK point callback.
- This verifies the targeted SDK command/configuration/data path **with the
  macOS socket workaround**. An unmodified end-to-end Linux SDK run and real
  hardware fidelity remain unverified. The patch is never applied automatically
  to the user's SDK checkout.

Build a separate upstream checkout (the test needs its static SDK library):

```sh
cmake -S /path/to/Livox-SDK2 -B /tmp/sdk2-build \
  -DCMAKE_POLICY_VERSION_MINIMUM=3.5 -DCMAKE_BUILD_TYPE=Release
cmake --build /tmp/sdk2-build --target livox_lidar_sdk_static -j 4
c++ -std=c++11 -I/path/to/Livox-SDK2/include -I/path/to/Livox-SDK2/3rdparty \
  tests/upstream/virtual_device_client.cpp \
  /tmp/sdk2-build/sdk_core/liblivox_lidar_sdk_static.a -pthread -o /tmp/sdk-client
cargo build --release -p virtual-lidar
python3 tests/upstream/run_virtual_device.py --sdk-client /tmp/sdk-client \
  --host HOST_LOCAL_IP --device DEVICE_LOCAL_IP
```

On macOS, apply the linked patch only to a test copy of the SDK before building;
use the working Command Line Tools compiler/make/sysroot if the Xcode shims are
unavailable (see README). Upstream's warnings-as-errors may require `-w` for
this compiler version. The SDK itself emits its normal discovery broadcasts;
the simulator's announcements and data go only to the configured destinations.
The integration client is a developer-only C++ executable, never a dependency
or FFI binding of the Rust application.

## Tests

`cargo test --release --workspace` includes actual loopback UDP tests (socket access is
required), as well as pure command-state tests, transactional SET failures,
Serde TLV count/bounds checks, data encode/decode against upstream C++ fixtures,
replay deadlines and reconstruction errors. Tests use OS-assigned source ports
to avoid collisions. They verify no data before sampling, directed discovery,
firmware query, endpoint configuration, malformed-packet rejection, pause/resume,
original payload bytes, source ports, recomputed CRC, monotonic loop timestamps
and finite packet limits.
