# Point-cloud viewer

`livox-viewer` is a Rust binary with embedded HTML, CSS and JavaScript. It uses
the existing Serde/bincode-backed Livox parsers and a local `std::net` HTTP
endpoint. The browser draws points with WebGL 1.0. No external assets or new
native/`sys` libraries are needed. The English interface uses white surfaces,
blue primary actions and collapsed secondary controls inspired by Material.

## Run (release by default)

```sh
cargo run --release -p livox-viewer -- file capture.lvx2
# Bundled official sample: one complete frame, useful for a quick preview.
cargo run --release -p livox-viewer -- file fixtures/official-mid360-first-frame.lvx2
```

Open the printed numeric loopback URL in Chrome. `--http-port 0` chooses a free
port. The server binds HTTP only to `127.0.0.1` and serves only embedded assets
and the selected source, never arbitrary files. Stop it with Ctrl-C. Assets
are compiled into the binary: rebuild **and restart** after UI changes, then
reload the browser. One browser session controls the shared file position.

File mode reads frames on demand, starts paused on the first frame, and supports
Play/Pause, Step, Restart and 0.25×–4× speed. The recorded frame duration controls
the intended pacing; slow parsing/rendering can lower the actual speed. No
whole-file preload or random-access seek index is created. At EOF the last
cloud remains visible; Play restarts. File parse errors are shown in the UI.

Use `--device-id ID` for multi-device recordings. Coordinates remain in one
device's coordinate system; extrinsics are not applied. `--history 5` (default)
overlays five file frames. `--max-points 200000` (default, maximum 2000000) keeps
the newest points when that limit is reached. File frames retain the reader's
64 MiB byte ceiling. Packet objects and JSON/browser buffers add memory overhead.

## Virtual device / UDP

In separate terminals:

```sh
cargo run --release -p livox-viewer -- udp --bind 127.0.0.1:56301
cargo run --release -p virtual-lidar --bin livox-virtual-device -- replay capture.lvx2 \
  --bind 127.0.0.1 --host 127.0.0.1 --point-port 56301 --start --no-announce --loop
```

For real hardware, bind an appropriate host interface (e.g. `0.0.0.0:56301`)
and optionally filter with `--source-ip DEVICE_IP`. Configure/start the LiDAR
separately: the viewer is a passive point receiver, not an SDK command client.
The first CRC-valid Cartesian sender (IP **and port**) is locked to avoid
combining unrelated sensor coordinates. Restart to select another sender.
`--window-ms 250` keeps a bounded arrival-time history, independent of device
clock domains. Pausing freezes the display while reception continues; Resume
shows current data. Silent senders age out of the display. UDP loss/reordering
is not repaired; the displayed counters are observed, not guaranteed totals.

## Display semantics and limits

- Cartesian high (type 1): millimetres to metres; low (type 2): centimetres to
  metres. All-zero coordinates are hidden. Reflectivity is kept as 0..255.
- IMU, spherical and unknown types are counted as skipped, never reinterpreted.
  Invalid CRC/length/version packets are counted as invalid.
- No LVX1 display, frame registration, motion compensation or semantic tag
  filtering. Motion can produce trails in the recent-frame overlay.
- Drag rotates around the target; Shift/right-drag pans; scroll zooms. Top view
  looks nearly along Z. Fit uses sampled 2nd/98th coordinate percentiles so
  isolated far returns do not dominate the view; points are still retained.
- Height colors use the bounds at the last Fit. Distance colors are relative
  to camera distance. Colors are visual guides, not calibrated measurements.
- The internal UI response is **not a Livox wire format**: u32 LE metadata byte
  count, Serde JSON status, then x/y/z/reflectivity as four f32 LE per point.
  Nanosecond timestamps are JSON strings to avoid JavaScript precision loss.

The shader's shared `u_grid` uniform and `v_color` varying explicitly use
`mediump` in both stages. Chrome rejected the initial implicit highp/mediump
uniform mismatch; an actual Chrome link/draw regression check now covers this.
Camera projection computations retain vertex high precision.

References: [WebGL specification](https://registry.khronos.org/webgl/specs/latest/1.0/),
[Google Material color roles](https://codelabs.developers.google.com/codelabs/apply-dynamic-color),
[Rust UDP API](https://doc.rust-lang.org/std/net/struct.UdpSocket.html).

## Verification

```sh
cargo test --release --workspace
cargo clippy --release --workspace --all-targets -- -D warnings
```

Viewer tests compare every visible point from the official first frame against
its original integer coordinates; verify equivalent file/UDP conversion,
centimetre scaling, zero filtering, timestamp precision, CRC/type rejection,
sender locking, FIFO/age limits, HTTP controls, EOF and reset.

For the browser check, start the viewer, then launch Chrome with a **dedicated
temporary profile**, e.g. on macOS:

```sh
"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
  --user-data-dir=/tmp/livox-viewer-chrome-test --remote-debugging-port=9222 \
  --no-first-run http://127.0.0.1:8080
node tests/viewer/chrome.mjs http://127.0.0.1:8080
```

Node 22+ is required only for this optional test, not for the viewer. The check
creates/closes its own tab, verifies actual WebGL linking, point display,
controls and responsive layouts, and saves screenshots under `/tmp`. Run with
a single viewer client and a file containing visible Cartesian points. Physical
GPU performance and Windows/Linux browser behavior have not been benchmarked.
