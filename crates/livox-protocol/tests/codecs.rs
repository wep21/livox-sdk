use livox_protocol::*;
const COMMAND: &[u8] = include_bytes!("../../../fixtures/sdk2-command.bin");
const EMPTY: &[u8] = include_bytes!("../../../fixtures/sdk2-command-empty.bin");
const HIGH: &[u8] = include_bytes!("../../../fixtures/sdk2-high.bin");
const IMU: &[u8] = include_bytes!("../../../fixtures/sdk2-imu.bin");
fn data(kind: u8, count: u16, payload: &[u8]) -> Vec<u8> {
    let mut b = vec![0; 36];
    b[1..3].copy_from_slice(&((36 + payload.len()) as u16).to_le_bytes());
    b[5..7].copy_from_slice(&count.to_le_bytes());
    b[10] = kind;
    b[28..36].copy_from_slice(&42u64.to_le_bytes());
    b.extend(payload);
    let crc = crc32(&b[28..]);
    b[24..28].copy_from_slice(&crc.to_le_bytes());
    b
}
#[test]
fn crc_parameters() {
    assert_eq!(crc16(b"123456789"), 0x29b1);
    assert_eq!(crc32(b"123456789"), 0xcbf43926);
    assert_eq!(crc32(&[]), 0);
}
#[test]
fn cpp_command_reference_and_roundtrip() {
    for bytes in [COMMAND, EMPTY] {
        let p = CommandPacket::decode(bytes).unwrap();
        assert_eq!(p.header.seq_num, 0x5678);
        assert_eq!(p.header.cmd_id, 0x100);
        assert_eq!(p.header.cmd_type, CommandType::Request);
        assert_eq!(p.header.sender_type, SenderType::Host);
        let owned = CommandPacketOwned {
            header: p.header.clone(),
            payload: p.payload.to_vec(),
        };
        let mut out = Vec::new();
        owned.encode(&mut out).unwrap();
        assert_eq!(out, bytes);
        assert_eq!(CommandPacket::decode(&out).unwrap(), p);
    }
    assert_eq!(
        CommandPacket::decode(COMMAND).unwrap().payload,
        [0, 255, 66, 128, 1]
    );
    assert!(CommandPacket::decode(EMPTY).unwrap().payload.is_empty());
}
#[test]
fn full_sequence_unknown_values_reserved_and_append() {
    let p = CommandPacketOwned {
        header: CommandHeader {
            seq_num: u32::MAX,
            cmd_id: u16::MAX,
            cmd_type: CommandType::Unknown(42),
            sender_type: SenderType::Unknown(99),
            reserved: [0xab; 6],
        },
        payload: vec![123; 1376],
    };
    let mut out = vec![9];
    p.encode(&mut out).unwrap();
    assert_eq!(out[0], 9);
    let d = CommandPacket::decode(&out[1..]).unwrap();
    assert_eq!(d.header, p.header);
    assert_eq!(d.payload, p.payload);
    let mut oversized = p.clone();
    oversized.payload.push(0);
    let before = out.clone();
    assert!(oversized.encode(&mut out).is_err());
    assert_eq!(out, before);
}
#[test]
fn command_malformed() {
    for n in 0..COMMAND.len() {
        assert!(CommandPacket::decode(&COMMAND[..n]).is_err());
    }
    let mut b = COMMAND.to_vec();
    b[0] = 0;
    assert!(matches!(
        CommandPacket::decode(&b),
        Err(DecodeError::InvalidSof(0))
    ));
    b = COMMAND.to_vec();
    b[1] = 1;
    assert!(matches!(
        CommandPacket::decode(&b),
        Err(DecodeError::UnsupportedVersion(1))
    ));
    for len in [0u16, 23, 28, 30, 1401, u16::MAX] {
        b = COMMAND.to_vec();
        b[2..4].copy_from_slice(&len.to_le_bytes());
        assert!(matches!(
            CommandPacket::decode(&b),
            Err(DecodeError::InvalidLength { .. })
        ));
    }
    b = COMMAND.to_vec();
    b.push(0);
    assert!(CommandPacket::decode(&b).is_err());
    b = COMMAND.to_vec();
    b[4] ^= 1;
    assert_eq!(CommandPacket::decode(&b), Err(DecodeError::Crc16Mismatch));
    b = COMMAND.to_vec();
    b[24] ^= 1;
    assert_eq!(CommandPacket::decode(&b), Err(DecodeError::Crc32Mismatch));
    b = EMPTY.to_vec();
    b[20] = 1;
    assert_eq!(CommandPacket::decode(&b), Err(DecodeError::Crc32Mismatch));
}
#[test]
fn cpp_data_reference() {
    let d = DataPacket::decode(HIGH).unwrap();
    assert_eq!(d.header.timestamp, 1234567890123);
    assert_eq!(d.header.time_interval, 1000);
    assert_eq!(d.header.udp_cnt, 65535);
    assert_eq!(d.header.frame_cnt, 254);
    assert_eq!(d.header.time_type, TimeType::Ptp);
    let DataPayload::CartesianHigh(points) = d.payload else {
        panic!()
    };
    assert_eq!(
        points.iter().collect::<Vec<_>>(),
        [CartesianHighPoint {
            xyz_mm: [-123456, 234567, -345678],
            reflectivity: 200,
            tag: 0x30
        }]
    );
    let d = DataPacket::decode(IMU).unwrap();
    assert_eq!(
        d.payload,
        DataPayload::Imu(ImuData {
            gyro: [1.25, -2.5, 3.75],
            acceleration: [0.5, -0.25, 1.0]
        })
    );
}
#[test]
fn data_variants_and_unaligned() {
    let low = data(2, 1, &[255, 255, 2, 0, 0, 128, 240, 7]);
    let p = DataPacket::decode(&low).unwrap();
    let DataPayload::CartesianLow(points) = p.payload else {
        panic!()
    };
    assert_eq!(points.iter().next().unwrap().xyz_cm, [-1, 2, i16::MIN]);
    let spherical = data(3, 1, &[0xe8, 3, 0, 0, 0x28, 0x23, 0x50, 0x46, 22, 33]);
    let DataPayload::Spherical(points) = DataPacket::decode(&spherical).unwrap().payload else {
        panic!()
    };
    assert_eq!(
        points.iter().next().unwrap(),
        SphericalPoint {
            depth_mm: 1000,
            theta: 9000,
            phi: 18000,
            reflectivity: 22,
            tag: 33
        }
    );
    for kind in [17, 255] {
        let b = data(kind, 65535, &[1, 2, 3]);
        assert_eq!(
            DataPacket::decode(&b).unwrap().payload,
            DataPayload::Unknown {
                data_type: kind,
                bytes: &[1, 2, 3]
            }
        );
    }
    for kind in [1, 2, 3, 255] {
        assert!(DataPacket::decode(&data(kind, 0, &[])).is_ok());
    }
    let mut unaligned = vec![7];
    unaligned.extend(HIGH);
    assert_eq!(
        DataPacket::decode(&unaligned[1..]),
        DataPacket::decode(HIGH)
    );
}
#[test]
fn data_malformed() {
    for n in 0..HIGH.len() {
        assert!(DataPacket::decode(&HIGH[..n]).is_err());
    }
    let mut b = HIGH.to_vec();
    b[0] = 5;
    assert!(matches!(
        DataPacket::decode(&b),
        Err(DecodeError::UnsupportedVersion(5))
    ));
    for n in [0u16, 35, 49, 51, u16::MAX] {
        b = HIGH.to_vec();
        b[1..3].copy_from_slice(&n.to_le_bytes());
        assert!(matches!(
            DataPacket::decode(&b),
            Err(DecodeError::InvalidLength { .. })
        ));
    }
    for offset in [24, 28, 36] {
        b = HIGH.to_vec();
        b[offset] ^= 1;
        assert!(matches!(
            DataPacket::decode(&b),
            Err(DecodeError::Crc32Mismatch)
        ));
    }
    assert!(matches!(
        DataPacket::decode(&data(1, u16::MAX, &[])),
        Err(DecodeError::PayloadSize { .. })
    ));
    assert!(DataPacket::decode(&data(0, 1, &[])).is_err());
    b = HIGH.to_vec();
    b[11] = 99;
    b[12] = 0xaf;
    let p = DataPacket::decode(&b).unwrap();
    assert_eq!(p.header.time_type, TimeType::Unknown(99));
    assert_eq!(p.header.reserved[0], 0xaf);
}
#[test]
fn arbitrary_bytes_never_panic() {
    let mut seed = 0x12345678u64;
    for size in 0..2048 {
        let mut b = vec![0; size];
        for x in &mut b {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            *x = seed as u8;
        }
        let _ = CommandPacket::decode(&b);
        let _ = DataPacket::decode(&b);
        let _ = legacy::Packet::decode(&b);
    }
}

#[test]
fn data_encoding_matches_cpp_and_is_atomic() {
    for bytes in [HIGH, IMU] {
        let p = DataPacket::decode(bytes).unwrap();
        let owned = DataPacketOwned {
            header: p.header,
            payload: bytes[36..].to_vec(),
        };
        let mut out = Vec::new();
        owned.encode(&mut out).unwrap();
        assert_eq!(out, bytes);
        let mut bad = owned.clone();
        bad.payload.push(0);
        let before = out.clone();
        assert!(bad.encode(&mut out).is_err());
        assert_eq!(out, before);
        bad.payload = vec![0; 65535];
        assert!(bad.encode(&mut out).is_err());
        assert_eq!(out, before);
    }
}
#[test]
fn parameter_payload_bounds() {
    use livox_protocol::control::*;
    assert_eq!(
        decode_get(&[2, 0, 0, 0, 0x10, 0x80, 0x1a, 0]).unwrap(),
        [0x8010, 0x1a]
    );
    let b = [1, 0, 0, 0, 0x1a, 0, 1, 0, 1];
    assert_eq!(
        decode_set(&b).unwrap(),
        [KeyValue {
            key: 0x1a,
            value: &[1]
        }]
    );
    for n in 0..b.len() {
        assert!(decode_set(&b[..n]).is_err());
    }
    assert!(decode_set(&[255, 255, 0, 0]).is_err());
    assert!(decode_get(&[1, 0, 0, 0]).is_err());
    assert!(decode_get(&[0, 0, 0, 0, 1]).is_err());
    let ep = HostEndpoint {
        ip: [127, 0, 0, 1],
        destination_port: 56301,
        source_port: 56300,
    };
    assert_eq!(
        HostEndpoint::decode(&encode_fixed(&ep).unwrap()).unwrap(),
        ep
    );
    for len in 0..1024 {
        let b = vec![255; len];
        let _ = decode_set(&b);
        let _ = decode_get(&b);
    }
}
