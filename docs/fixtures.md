# Verification data and reproducibility

Reviewed/downloaded 2026-09-27. Tests run offline; no test downloads anything.
Checksums for checked-in fixtures are in [SHA256SUMS](../fixtures/SHA256SUMS).

## Official recordings

The [official downloads center](https://www.livoxtech.com/downloads) publishes both
LVX and LVX2 recordings, as well as both format PDFs.

| Fixture | Official source | Selection / verification |
|---|---|---|
| `official-mid360-first-frame.lvx2` | [Mid-360 Indoor](https://terra-1-g.djicdn.com/65c028cd298f4669a7f0e40e50ba1131/Mid360/Indoor_sampledata.lvx2) | Unchanged first 142700 bytes, ending exactly at first next_offset; one frame, 104 type-1 packets, 96 points/packet |
| `official-tele15-first-frame.lvx` | [Tele-15 Black SUV archive](https://terra-1-g.djicdn.com/65c028cd298f4669a7f0e40e50ba1131/Download/TELE15/Sample%20Point%20Cloud_Black%20SUV.zip) | Unchanged first 170917 bytes of `Black SUV Point Cloud Testing at Different Distance.lvx`; one frame, 125 tagged Cartesian + 10 IMU packets |

These are **real capture prefixes**, not complete recordings or synthesized files.
Headers/offsets/point bytes have not been rewritten. LVX has no footer; cutting at
an existing frame boundary produces a readable shorter file. Source data remains
attributed to Livox; no claim of an MIT license for third-party capture data.
Full recordings are intentionally not checked in.

The entire official Mid-360 recording was downloaded and scanned with both an
independent Python `struct` inspection and the Rust `lvx-info` CLI:

| Property | Observed value |
|---|---|
| File length | 222540611 bytes |
| SHA-256 | `f892732ff43882b56d1cebc683f6ea9374ab3d3ac688368c9d560f49dcd4d647` |
| Version | 2.0.0.0 |
| Device | Mid360 (9), LiDAR ID 738306240, serial 47MDK9DF710030 |
| Frames | 1534 |
| Packets | 162293, all type 1 Cartesian high |
| First / last timestamp | 711620799460 / 789520959460 ns, uptime |
| End boundary | Exactly EOF |

Interesting capture observations: device `lidar_type` reserved byte is 247,
first packet's reserved lidar_type is 8, and hub serial storage contains nonzero
bytes after its initial NUL. These bytes are preserved, not rejected as invalid
or interpreted as undocumented flags. They demonstrate why reserved bytes
must not be assumed zero. The first prefix timestamps end at 711670239460 ns.
The Tele-15 prefix timestamps run from 3194290697090 to 3194340211930 ns.
The complete 2634022176-byte Tele-15 recording was not scanned in this iteration;
only its first complete frame was extracted from the official archive.

## C/C++ behavior fixtures

[Adapters](../tests/upstream) compile against **unmodified upstream source**:

- SDK2 `c0796f04c143143899c87a773d9f6b7136453c0b`: `SdkProtocol::Pack`,
  `CheckPreamble`, `ParsePacket`, `FastCRC`, actual point/IMU header definitions.
- SDK1 `9306596a2bf15c1343bc023b497465ed0a32909d`: actual `LvxFileHandle`
  writer, `BasePointsHandle`, and all nine supported legacy point type sizes.

| Fixture | Oracle / assertions |
|---|---|
| sdk2-command.bin | SDK Pack request 0x0100, input sequence 0x12345678 becomes 0x5678; payload 00 ff 42 80 01 |
| sdk2-command-empty.bin | SDK Pack's 24-byte empty frame and zero data CRC |
| sdk2-high.bin | Actual SDK header/point fields, documented CRC range calculated with upstream FastCRC; negative coordinates and timestamp checked |
| sdk2-imu.bin | Actual SDK IMU fields with exact binary floats; documented CRC calculated by upstream FastCRC |
| sdk1-writer.lvx | Actual SDK writer: device ID 7, extrinsics, nine packet types, one populated and one empty frame |

The generated SDK2 point fixtures are **not captured network traffic**. SDK2 has
no data decoder validation to compare against; the oracle here is its packed
representation plus official CRC coverage. No hardware Mid360S/L testing has
been performed. Real LVX2 captures cannot independently verify UDP CRC because
that CRC is absent from the file format.

```sh
tests/upstream/regenerate.sh /path/to/Livox-SDK /path/to/Livox-SDK2
(cd fixtures && shasum -a 256 -c SHA256SUMS)
cargo test --release --workspace
```

A C++ compiler is needed only for regenerating these developer fixtures. It is
not a Rust build/runtime dependency; no native Livox SDK binding is used.
