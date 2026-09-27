# SDK2 control protocol (Mid-360 family first)

Sources inspected 2026-09-27:
- [Official English protocol](https://livox-wiki-en.readthedocs.io/en/latest/tutorials/new_product/mid360/livox_eth_protocol_mid360.html), control format and CRC algorithm sections.
- [Official Chinese protocol (Mid-360/S/L)](https://livox-wiki-cn.readthedocs.io/zh_CN/latest/tutorials/new_product/mid360/livox_eth_protocol_mid360.html).
- [SDK2 source](https://github.com/Livox-SDK/Livox-SDK2/tree/c0796f04c143143899c87a773d9f6b7136453c0b): `sdk_core/comm/{sdk_protocol.h,sdk_protocol.cpp,protocol.h,define.h,generate_seq.cpp}`, `3rdparty/FastCRC/FastCRCsw.cpp`.

Offsets describe the UDP payload, independent of C struct alignment. All multibyte integers are LE.

| Offset | Width | Field | Type/endian | Meaning / valid values / validation |
|---|---|---|---|---|
| 0 | 1 | sof | u8 | Must be 0xAA |
| 1 | 1 | version | u8 | Must be 0 |
| 2 | 2 | length | u16 LE | Total including 24-byte header; 24..1400 |
| 4 | 4 | seq_num | u32 LE | Request counter; ACK echoes request; preserve all bits |
| 8 | 2 | cmd_id | u16 LE | 0 discovery, 0x100 configure, 0x101 query, 0x102 push; preserve unknown IDs |
| 10 | 1 | cmd_type | u8 | 0 request, 1 ACK; preserve unknown |
| 11 | 1 | sender_type | u8 | 0 host, 1 LiDAR; preserve unknown |
| 12 | 6 | reserved | bytes | Preserve; do not infer fields |
| 18 | 2 | crc16 | u16 LE | CRC over bytes [0,18) |
| 20 | 4 | crc32 | u32 LE | CRC over [24,length); empty payload => 0 |
| 24 | length-24 | payload | bytes | Command-specific interpretation deferred |

| CRC | Polynomial | Init | RefIn/RefOut | XorOut | Check `123456789` |
|---|---|---|---|---|---|
| CRC-16/CCITT-FALSE (IBM-3740) | 0x1021 | 0xffff | false/false | 0 | 0x29b1 |
| CRC-32/ISO-HDLC | 0x04c11db7 | 0xffffffff | true/true | 0xffffffff | 0xcbf43926 |

Decode requires one complete datagram: minimum header, SOF/version, declared length equal to input and <=1400, then both CRCs. Encoding writes explicit LE bytes and computes lengths/CRCs; rejects oversized payload before modifying output. No packed structures. Reserved bytes are preserved, not silently zeroed on re-encoding.

## Differences and unresolved behavior

- SDK `Pack` masks sequence to 16 bits. `GenerateSeq` starts with internal 1, returns 2 first, wraps 65535 to 1. Documentation specifies a 32-bit increasing field. Rust preserves the documented 32-bit value; future device policy can reproduce SDK sequencing separately.
- SDK `Pack` does not initialize reserved bytes itself; it depends on caller buffer initialization. Rust owns all encoded bytes.
- `CheckPreamble` accepts a buffer longer than the declared packet; Rust datagram decode rejects trailing bytes. `ParsePacket` alone does not verify CRC; upstream callers must call validation first.
- The SDK's 1400-byte buffer constant matches the documented maximum; its direct validator does not enforce that maximum. Rust does.
- C++ writes host-order packed fields; documentation explicitly requires LE. Rust works independently of host endian/alignment.
- Unknown commands and enum values can be framed safely, but their semantics and firmware support remain unimplemented. No command execution, device state machine or UDP transport in this milestone.

Fixed headers use private `serde::Deserialize` wire structs with bincode 2 fixed-width LE configuration (256-byte decode limit, exact consumption checked). This is a decoding mechanism for the documented field sequence, not the default bincode format and not serialization of public Rust enums. Encoding and point iterators use explicit LE fields. Length and CRC checks remain independent.
