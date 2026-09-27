# LVX2 research (Mid-360 priority)

**An official specification and official recordings exist.** LVX2 is not an undocumented variant of LVX1. Research checked 2026-09-27.

Primary sources:
- [Official downloads](https://www.livoxtech.com/downloads).
- [LVX2 specification, v1.0 June 2023](https://terra-1-g.djicdn.com/65c028cd298f4669a7f0e40e50ba1131/LVX2%20Specifications.pdf), file version **2.0.0.0**.
- [Official Mid-360 indoor recording](https://terra-1-g.djicdn.com/65c028cd298f4669a7f0e40e50ba1131/Mid360/Indoor_sampledata.lvx2).

SDK2 c0796f04c143143899c87a773d9f6b7136453c0b has no LVX reader/writer source found by file/content search. `include/livox_lidar_def.h` confirms point layouts, not file packet headers. Do not substitute `LivoxLidarEthernetPacket` for an LVX2 record.

All fields below are **confirmed by the official PDF**; numeric offsets are sums of its field widths (the PDF lists ordered fields). Multibyte values are LE.

| Offset | Width | Public/private field | Type | Meaning / valid values |
|---|---|---|---|---|
| 0 | 16 | signature | bytes | Ten ASCII bytes `livox_tech` and six zeros |
| 16 | 4 | version | bytes | [2,0,0,0] |
| 20 | 4 | magic | u32 LE | 0xac0ea767 |
| 24 | 4 | frame_duration | u32 LE | Fixed 50 ms in published version |
| 28 | 1 | device_count | u8 | Number of 63-byte records |
| 29 | count*63 | device table | records | Separate from 59-byte LVX1 entries |

| Device offset | Width | Field | Type/endian | Meaning / valid values |
|---|---|---|---|---|
| 0 | 16 | lidar_serial | bytes | LiDAR serial |
| 16 | 16 | hub_serial | bytes | Empty for no hub |
| 32 | 4 | lidar_id | u32 LE | Device lookup key (NOT an LVX1 u8 index) |
| 36 | 1 | lidar_type | u8 | Reserved |
| 37 | 1 | device_type | u8 | Published: 9 Mid360, 10 HAP |
| 38 | 1 | extrinsic_enable | u8 | 0/1 |
| 39/43/47 | 4 each | roll/pitch/yaw | f32 LE | Degrees |
| 51/55/59 | 4 each | x/y/z | f32 LE | Metres |

| Frame offset | Width | Field | Type/endian | Meaning / validation for future parser |
|---|---|---|---|---|
| 0 | 8 | current_offset | i64 LE in PDF | Absolute file position, nonnegative |
| 8 | 8 | next_offset | i64 LE in PDF | >=current+24, <=file size |
| 16 | 8 | frame_index | i64 LE in PDF | Frame ordinal |
| 24 | variable | records | bytes | Must end exactly at next_offset |

| Record offset | Width | Field | Type/endian | Meaning / valid values |
|---|---|---|---|---|
| 0 | 1 | version | u8 | 0 |
| 1 | 4 | lidar_id | u32 LE | Device table key |
| 5 | 1 | lidar_type | u8 | Reserved |
| 6 | 1 | timestamp_type | u8 | 0 uptime, 1 PTP, 2 GPS |
| 7 | 8 | timestamp | u64 LE | Nanoseconds |
| 15 | 2 | udp_counter | u16 LE | Original UDP counter |
| 17 | 1 | data_type | u8 | Published: 1 high Cartesian, 2 low Cartesian |
| 18 | 4 | length | u32 LE | PAYLOAD ONLY, unlike UDP total length |
| 22 | 1 | frame_counter | u8 | Reserved |
| 23 | 4 | reserved | bytes | Preserve |
| 27 | length | points | bytes | Type 1: 14 bytes/point; type 2: 8 bytes/point |

Point offsets/units match types 1/2 in [point-protocol.md](point-protocol.md). Published HAP/Mid360 count is 96 records/package. Future validation must check divisibility and physical bounds before allocation; no UDP checksum is stored here.

| Status | Finding / open question |
|---|---|
| Confirmed | Published base layout is sufficient to implement a separate LVX2 2.0 reader. There is no need to reverse engineer those fields. |
| Confirmed | Shared signature/magic alone do not distinguish LVX1/LVX2; inspect all four version bytes. |
| Confirmed | PDF lists only Cartesian 1/2. IMU, spherical and newer-device file extensions are not specified by this PDF. |
| Inferred | Newer Viewer2 releases may write additional types; firmware SDK enums alone do not establish their on-disk LVX2 format. |
| Unknown | Mid360S/L Viewer2 recording extensions, reserved-byte meanings, IMU file framing, signed counter behavior. |
| Scope | User priority was updated to Mid-360. A separate `Lvx2Reader` now implements this confirmed 2.0 layout; known 1/2 payloads are typed, unknown length-delimited payloads are retained. No IMU interpretation is invented. |

Official sample verification and downloadable-file provenance are recorded in [fixtures.md](fixtures.md). No invented LVX2 fields or merged LVX1/LVX2 parser.

Implementation uses private Serde schemas and bincode fixed-width little-endian decoding for headers. No strings, Vecs, enums, or variable-length prefixes are decoded from these fixed schemas; file payload allocations follow explicit bounds checks. Public domain types are separate from the serialized schemas.

## Search trail

The [official Livox-Viewer repository](https://github.com/Livox-SDK/Livox-Viewer)
contains a README linking downloads, rather than reader source. Its
[GitHub release list](https://github.com/Livox-SDK/Livox-Viewer/releases) was empty
when checked. The [issue list](https://github.com/Livox-SDK/Livox-Viewer/issues)
and [SDK2 Viewer wiki](https://github.com/Livox-SDK/Livox-SDK2/wiki/Livox-Viewer)
were also inspected; they did not supply a stronger format specification.
The vendor downloads center supplies Viewer2 binary releases and, more usefully
for this task, the official PDF and direct `.lvx2` samples. No binary reverse
engineering was needed for the published base format. This search is not a
claim that no other implementation exists.

## Implemented validation and observed limits

`Lvx2Reader` accepts exactly file version 2.0.0.0 and packet version 0, validates
unique device IDs and packet references, checks frame offsets and payload
bounds, and limits frame allocation to 64 MiB by default. The shared point
views currently accept at most 65535 records in a single packet; published
Mid-360 packets contain 96. Divisibility by 14/8 is required for known types.
Frame duration values other than 50 ms are preserved and warned about.
Unknown data types remain opaque because LVX2 provides an explicit byte length.
Header reserved fields and bytes after a serial NUL may be nonzero in the
confirmed official sample and are preserved. There is no CRC in this file
record. Real sample observations are in [fixtures.md](fixtures.md).
