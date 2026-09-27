# LVX1 (implemented version 1.1.0.0)

Primary [official LVX specification PDF](https://terra-1-g.djicdn.com/65c028cd298f4669a7f0e40e50ba1131/Download/LVX%20Specifications.pdf), cover v1.1, May 2020; linked by [official downloads](https://www.livoxtech.com/downloads). Also [legacy communication protocol](https://github.com/Livox-SDK/Livox-SDK/wiki/Livox-SDK-Communication-Protocol).

Corresponding [SDK1 commit 9306596](https://github.com/Livox-SDK/Livox-SDK/tree/9306596a2bf15c1343bc023b497465ed0a32909d): `sample/lidar_lvx_file/{lvx_file.h,lvx_file.cpp}`, `sample/hub_lvx_file/lvx_file.h`, `sdk_core/include/livox_def.h`. Lidar and hub header layouts agree. Offsets below were calculated from the official field widths and checked against these structures/writer, not assumed from a native ABI.

| File offset | Width | Field | Type/endian | Meaning / validation |
|---|---|---|---|---|
| 0 | 16 | signature | bytes | `livox_tech` (10 ASCII bytes) then 6 zero bytes |
| 16 | 4 | version | u8[4] | Exactly [1,1,0,0] supported; LVX2 [2,0,0,0] separately detected |
| 20 | 4 | magic | u32 LE | 0xac0ea767 |
| 24 | 4 | frame_duration | u32 LE | ms; spec fixes 50; reader preserves other values with warning in CLI |
| 28 | 1 | device_count | u8 | Number of 59-byte device entries |
| 29 | count*59 | devices | records | Then frames until EOF |

| Device-relative offset | Width | Field | Type/endian | Meaning / validation |
|---|---|---|---|---|
| 0 | 16 | lidar_broadcast_code | bytes | Serial/broadcast code, preserve raw bytes |
| 16 | 16 | hub_broadcast_code | bytes | Empty when directly connected |
| 32 | 1 | device_index | u8 | Unique lookup ID, not necessarily vector position (SDK uses handle) |
| 33 | 1 | device_type | u8 | 0 hub, 1 Mid40/Mid100, 2 Tele15, 3 Horizon; newer SDK 6 Mid70, 7 Avia; unknown retained |
| 34 | 1 | extrinsic_enable | u8 | 0/1 only |
| 35/39/43 | 4 each | roll/pitch/yaw | f32 LE | Degrees; retained, not applied |
| 47/51/55 | 4 each | x/y/z | f32 LE | Metres; retained, not applied |

| Frame-relative offset | Width | Field | Type/endian | Meaning / validation |
|---|---|---|---|---|
| 0 | 8 | current_offset | u64 LE | Must equal actual absolute frame position |
| 8 | 8 | next_offset | u64 LE | >=current+24 and <=file size; checked arithmetic; last frame ends at EOF |
| 16 | 8 | frame_index | u64 LE | Preserved; ordering not assumed |
| 24 | next-current-24 | packets | records | Must end exactly on frame boundary |

| Packet-relative offset | Width | Field | Type/endian | Meaning / validation |
|---|---|---|---|---|
| 0 | 1 | device_index | u8 | Must reference device table |
| 1 | 1 | version | u8 | 5 |
| 2 | 1 | slot_id | u8 | Direct=1, hub=1..9; preserve |
| 3 | 1 | lidar_id | u8 | Legacy Mid100 subunit ID; preserve |
| 4 | 1 | reserved | u8 | Preserve |
| 5 | 4 | status | u32 LE | Preserve status bits |
| 9 | 1 | timestamp_type | u8 | Legacy types: 0 uptime,1 PTP,3 GPS UTC,4 PPS; unknown retained |
| 10 | 1 | data_type | u8 | Determines implicit record length |
| 11 | 8 | timestamp | bytes | Raw; UTC type 3 must not be interpreted as u64 nanoseconds |
| 19 | variable | points | records | No length field and no CRC |

| Type | Name | Records | Record bytes | Payload bytes |
|---|---|---|---|---|
| 0 | Cartesian | 100 | 13 | 1300 |
| 1 | Spherical | 100 | 9 | 900 |
| 2 | Cartesian tagged | 96 | 14 | 1344 |
| 3 | Spherical tagged | 96 | 10 | 960 |
| 4 | Dual Cartesian | 48 | 28 | 1344 |
| 5 | Dual spherical | 48 | 16 | 768 |
| 6 | IMU | 1 | 24 | 24 |
| 7 | Triple Cartesian (SDK extension) | 30 | 42 | 1260 |
| 8 | Triple spherical (SDK extension) | 30 | 22 | 660 |

Cartesian x/y/z: i32 LE at 0/4/8, mm; reflectivity u8 at 12; tagged adds tag u8 at 13. Spherical depth u32 LE at 0, theta/phi u16 LE at 4/6 (0.01 degree), reflectivity at 8 and optional tag at 9. Dual/triple Cartesian concatenate tagged records. Dual/triple spherical share theta/phi at 0/2 followed by 2/3 six-byte returns (u32 depth, reflectivity u8, tag u8). IMU is six f32 LE at 0/4/8/12/16/20, rad/s then g. Units and tag bytes are preserved; no extrinsic conversion.

## Differences, validation policy, unresolved questions

- The signature is ten ASCII characters followed by six zero bytes; SDK zero-initialization and the published table agree.
- PDF introduction says v1.0 despite v1.1 title and [1,1,0,0] table. Version bytes select the parser; 1.0.0.0 is explicitly unsupported, not silently read as 1.1.
- PDF frame offsets are signed `long long`, SDK uses u64. Reader handles u64 with bounds checks; negative bit patterns cannot address real file data.
- PDF spherical depths say signed int; SDK and network spec use u32. Raw bytes retained, reusable protocol decoding uses u32.
- `LvxBasePackDetail::raw_point[1500]` is capacity, not on-disk padding. `pack_size` is bookkeeping and is NOT serialized. `BasePointsHandle` and `SaveFrameToLvxFile` establish actual lengths above.
- Unknown type cannot be skipped safely because there is no record length. Return a structured unsupported-type error rather than guessing. Unknown device/time enum values are retained.
- Reader bounds frame allocation (configurable 64 MiB default), validates device uniqueness, offsets and packet bounds; partial final frame returns an error once and ends iteration. Empty frame and header-only file are permitted.
- CRC cannot validate LVX1 file integrity because none is present. Strict physical bounds are not proof a capture is semantically correct.

Implementation uses private Serde schemas and bincode fixed-width little-endian decoding for headers. No strings, Vecs, enums, or variable-length prefixes are decoded from these fixed schemas; file payload allocations follow explicit bounds checks. Public domain types are separate from the serialized schemas.
