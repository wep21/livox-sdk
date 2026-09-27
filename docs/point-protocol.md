# SDK2 point / IMU protocol

Primary sources: [English Mid-360 protocol](https://livox-wiki-en.readthedocs.io/en/latest/tutorials/new_product/mid360/livox_eth_protocol_mid360.html) and [Chinese Mid-360/S/L protocol](https://livox-wiki-cn.readthedocs.io/zh_CN/latest/tutorials/new_product/mid360/livox_eth_protocol_mid360.html), inspected 2026-09-27. Corresponding [SDK2 commit c0796f0](https://github.com/Livox-SDK/Livox-SDK2/tree/c0796f04c143143899c87a773d9f6b7136453c0b): `include/livox_lidar_def.h` (`LivoxLidarEthernetPacket` and point structures), `sdk_core/data_handler/data_handler.cpp`, `sdk_core/comm/define.h`.

This is not the legacy SDK1 packet stored in LVX1, nor the LVX2 file record header.

| Offset | Width | Field | Type/endian | Meaning / validation |
|---|---|---|---|---|
| 0 | 1 | version | u8 | 0 supported |
| 1 | 2 | length | u16 LE | Total UDP payload; >=36, equal to input |
| 3 | 2 | time_interval | u16 LE | Last minus first sample time, 0.1 microsecond units |
| 5 | 2 | dot_num | u16 LE | Point count; validate against known payload sizes |
| 7 | 2 | udp_cnt | u16 LE | Packet counter resets per scan frame |
| 9 | 1 | frame_cnt | u8 | Frame counter; invalid for non-repeating scan |
| 10 | 1 | data_type | u8 | See below; unknown retained as bytes |
| 11 | 1 | time_type | u8 | 0 device uptime, 1 PTP/gPTP, 2 GPS; unknown retained |
| 12 | 12 | reserved | bytes | Retain without interpretation |
| 24 | 4 | crc32 | u32 LE | CRC-32/ISO-HDLC over timestamp AND payload: [28,length) |
| 28 | 8 | timestamp | u64 LE | First sample time, nanoseconds for documented types |
| 36 | variable | data | bytes | Type-specific records |

CRC parameters are in [control-protocol.md](control-protocol.md). Data CRC does **not** cover the first 24 bytes. A modified timestamp must fail checksum. No automatic CRC bypass is provided.

| Data type | Record width | Relative fields (offset:width:type) | Units / valid values |
|---|---|---|---|
| 0 IMU | 24 | gyro x/y/z at 0/4/8:f32; acc x/y/z at 12/16/20:f32 | rad/s, g; all floats LE; one sample |
| 1 Cartesian high | 14 | x/y/z at 0/4/8:i32; reflectivity at 12:u8; tag at 13:u8 | mm; raw tag preserved |
| 2 Cartesian low | 8 | x/y/z at 0/2/4:i16; reflectivity at 6:u8; tag at 7:u8 | cm; raw tag preserved |
| 3 Spherical | 10 | depth 0:4:u32; theta 4:2:u16; phi 6:2:u16; reflectivity 8:u8; tag 9:u8 | mm, 0.01 degree; theta 0..18000, phi 0..36000 (not filtered) |
| 0x11 double echo | 28 (SDK struct) | two consecutive high Cartesian records at 0 and 14 | SDK-confirmed layout; Mid-360 count semantics not documented here; retained opaque |
| other | unknown | raw bytes | Length and CRC still checked; no inferred record size |

For types 1..3, payload size must equal dot_num * record width. For IMU a 24-byte sample is required; dot_num is preserved (the point count field's IMU semantics are not independently established). Floats, including nonfinite sensor values, are preserved. Record iteration is safe on unaligned input.

## Differences and unknowns

- SDK `DataHandler::Handle` casts the buffer and invokes callbacks without length/version/CRC verification; Rust validates before exposing payloads.
- Both retrieved official language versions describe `reserved[12]`, yet tag prose refers to `pack_info.tag_type`, without assigning its bit/byte location. Do not invent a tag_type offset.
- SDK adds type 0x11; retrieved Mid-360 data type table lists 0..3. Its existence alone is not proof all Mid-360 models emit it. A future documented profile can add semantic double-echo parsing.
- Chinese documentation explicitly includes Mid-360, Mid-360S and Mid-360L; spherical output is marked unsupported by 360L. The shared codec parses the format, not device capabilities.
- SDK device IDs: Mid360=9, Mid360S=35, Mid360L=41, Avia2=40, industrial HAP=10, HAP=15. Unknown IDs are preserved.
- No raw Mid-360 UDP capture is bundled yet; LVX2 recordings use a different header and omit the UDP CRC. Generated SDK fixtures validate byte interpretation, not real firmware CRC compliance.

Mid-360 family default ports: discovery 56000; device control/push/point/IMU/log 56100/56200/56300/56400/56500; recommended host counterparts 56101/56201/56301/56401/56501. No sockets are implemented.

Fixed headers use private `serde::Deserialize` wire structs with bincode 2 fixed-width LE configuration (256-byte decode limit, exact consumption checked). This is a decoding mechanism for the documented field sequence, not the default bincode format and not serialization of public Rust enums. Encoding and point iterators use explicit LE fields. Length and CRC checks remain independent.

`DataPacketOwned::encode` now serializes the same fixed Serde wire schema, derives
total length and timestamp+payload CRC, validates through decode before appending,
and rejects datagrams above the IPv4 UDP payload maximum (65507 bytes). Length
and CRC fields in the supplied semantic header are replaced by derived values.
