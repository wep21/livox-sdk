use livox_lvx::*;
use livox_protocol::{DeviceType, ImuData, legacy};
use std::io::Cursor;
const SDK: &[u8] = include_bytes!("../../../fixtures/sdk1-writer.lvx");
const TELE: &[u8] = include_bytes!("../../../fixtures/official-tele15-first-frame.lvx");
const MID: &[u8] = include_bytes!("../../../fixtures/official-mid360-first-frame.lvx2");
fn header(version: [u8; 4]) -> Vec<u8> {
    let mut b = Vec::from(SIGNATURE);
    b.extend(version);
    b.extend(MAGIC.to_le_bytes());
    b.extend(50u32.to_le_bytes());
    b.push(0);
    b
}
#[test]
fn upstream_writer_all_legacy_types() {
    let mut r = LvxReader::new(Cursor::new(SDK)).unwrap();
    assert_eq!(r.header().frame_duration_ms, 50);
    assert_eq!(r.devices()[0].device_index, 7);
    assert_eq!(r.devices()[0].device_type, DeviceType::Horizon);
    assert_eq!(r.devices()[0].rotation_deg, [1.5, 0., 0.]);
    assert_eq!(r.devices()[0].translation_m, [-2.25, 0., 0.]);
    let frames = r.frames().collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].packets.len(), 9);
    assert!(frames[1].packets.is_empty());
    for (i, p) in frames[0].packets.iter().enumerate() {
        assert_eq!(p.header.data_type, legacy::DataType::from(i as u8));
        assert_eq!(p.payload.len(), p.header.data_type.payload_len().unwrap());
        assert_eq!(p.header.timestamp_ns(), Some(1000000000 + i as u64));
        assert_eq!(p.header.status, 0x12345678);
    }
    assert_eq!(
        ImuData::decode(&frames[0].packets[6].payload).unwrap().gyro,
        [1.25, -2.5, 3.75]
    );
    assert!(r.frames().next().is_none());
}
#[test]
fn official_lvx1_prefix() {
    let mut r = LvxReader::new(Cursor::new(TELE)).unwrap();
    assert_eq!(r.devices()[0].device_type, DeviceType::Tele15);
    let frames = r.frames().collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].index, 0);
    assert_eq!(frames[0].packets.len(), 135);
}
#[test]
fn official_mid360_prefix() {
    let mut r = Lvx2Reader::new(Cursor::new(MID)).unwrap();
    assert_eq!(r.header().version, [2, 0, 0, 0]);
    assert_eq!(r.devices()[0].device_type, DeviceType::Mid360);
    assert_eq!(r.devices()[0].lidar_id, 738306240);
    let f = r.frames().next().unwrap().unwrap();
    assert_eq!(f.index, 0);
    assert_eq!(f.packets.len(), 104);
    assert_eq!(f.packets[0].timestamp, 711620799460);
    assert_eq!(f.packets[0].udp_counter, 17473);
    let lvx2::Lvx2Payload::CartesianHigh(p) = f.packets[0].payload().unwrap() else {
        panic!()
    };
    assert_eq!(p.len(), 96);
    assert_eq!(p.iter().next().unwrap().xyz_mm, [0, 0, 0]);
    assert!(r.frames().next().is_none());
}
#[test]
fn versions_and_smallest_files() {
    assert_eq!(detect_format(MID).unwrap(), Format::Lvx2);
    assert_eq!(detect_format(SDK).unwrap(), Format::Lvx1_1);
    assert!(matches!(
        LvxReader::new(Cursor::new(MID)),
        Err(LvxError::UnsupportedFormat(Format::Lvx2))
    ));
    assert!(matches!(
        Lvx2Reader::new(Cursor::new(SDK)),
        Err(LvxError::UnsupportedFormat(Format::Lvx1_1))
    ));
    let mut r = LvxReader::new(Cursor::new(header([1, 1, 0, 0]))).unwrap();
    assert!(r.frames().next().is_none());
    let mut r = Lvx2Reader::new(Cursor::new(header([2, 0, 0, 0]))).unwrap();
    assert!(r.frames().next().is_none());
    assert!(matches!(
        LvxReader::new(Cursor::new(header([1, 0, 0, 0]))),
        Err(LvxError::UnsupportedFormat(_))
    ));
}
#[test]
fn bad_headers() {
    for n in 0..88 {
        assert!(LvxReader::new(Cursor::new(&SDK[..n])).is_err());
    }
    for n in 0..92 {
        assert!(Lvx2Reader::new(Cursor::new(&MID[..n])).is_err());
    }
    let mut b = SDK.to_vec();
    b[20] ^= 1;
    assert!(matches!(
        LvxReader::new(Cursor::new(b)),
        Err(LvxError::InvalidMagic(_))
    ));
    let mut b = SDK.to_vec();
    b[0] = 0;
    assert!(matches!(
        LvxReader::new(Cursor::new(b)),
        Err(LvxError::InvalidSignature)
    ));
    let mut b = SDK.to_vec();
    b[29 + 34] = 2;
    assert!(matches!(
        LvxReader::new(Cursor::new(b)),
        Err(LvxError::InvalidExtrinsic(2))
    ));
    let mut b = SDK[..88].to_vec();
    b[28] = 2;
    b.extend_from_slice(&SDK[29..88]);
    assert!(matches!(
        LvxReader::new(Cursor::new(b)),
        Err(LvxError::DuplicateDevice(7))
    ));
    let mut b = MID[..92].to_vec();
    b[28] = 2;
    b.extend_from_slice(&MID[29..92]);
    assert!(matches!(
        Lvx2Reader::new(Cursor::new(b)),
        Err(LvxError::DuplicateLidarId(_))
    ));
}
#[test]
fn frame_bounds_truncation_and_fused_errors() {
    for next in [0u64, 88, 111, u64::MAX] {
        let mut b = SDK.to_vec();
        b[96..104].copy_from_slice(&next.to_le_bytes());
        let mut r = LvxReader::new(Cursor::new(b)).unwrap();
        assert!(r.frames().next().unwrap().is_err());
        assert!(r.frames().next().is_none());
    }
    let mut b = SDK.to_vec();
    b[88] = 0;
    let mut r = LvxReader::new(Cursor::new(b)).unwrap();
    assert!(matches!(
        r.frames().next().unwrap(),
        Err(LvxError::InvalidFrameOffsets { .. })
    ));
    for n in [89, 100, SDK.len() - 1] {
        let mut r = LvxReader::new(Cursor::new(&SDK[..n])).unwrap();
        assert!(r.frames().any(|f| f.is_err()));
        assert!(r.frames().next().is_none());
    }
    let mut r = LvxReader::with_options(
        Cursor::new(SDK),
        ReaderOptions {
            max_frame_bytes: 24,
        },
    )
    .unwrap();
    assert!(matches!(
        r.frames().next().unwrap(),
        Err(LvxError::FrameTooLarge { .. })
    ));
    assert!(matches!(
        parse_frame(&[0; 24], u64::MAX, &[], 100),
        Err(LvxError::Overflow)
    ));
}
#[test]
fn unknown_legacy_cannot_be_skipped() {
    let mut b = SDK.to_vec();
    b[122] = 255;
    let mut r = LvxReader::new(Cursor::new(b)).unwrap();
    assert!(matches!(
        r.frames().next().unwrap(),
        Err(LvxError::Packet(
            livox_protocol::DecodeError::UnsupportedLegacyType(255)
        ))
    ));
    let mut b = SDK.to_vec();
    b[112] = 99;
    let mut r = LvxReader::new(Cursor::new(b)).unwrap();
    assert!(matches!(
        r.frames().next().unwrap(),
        Err(LvxError::UnknownDevice(99))
    ));
}
#[test]
fn lvx2_unknown_preserved_and_length_checked() {
    let mut b = MID.to_vec();
    b[116 + 17] = 255;
    let mut r = Lvx2Reader::new(Cursor::new(b)).unwrap();
    let f = r.frames().next().unwrap().unwrap();
    assert!(matches!(
        f.packets[0].payload().unwrap(),
        lvx2::Lvx2Payload::Unknown { data_type: 255, .. }
    ));
    let mut b = MID.to_vec();
    b[116 + 18..116 + 22].copy_from_slice(&u32::MAX.to_le_bytes());
    let mut r = Lvx2Reader::new(Cursor::new(b)).unwrap();
    assert!(r.frames().next().unwrap().is_err());
    assert!(r.frames().next().is_none());
    let mut b = MID.to_vec();
    b[100..108].copy_from_slice(&u64::MAX.to_le_bytes());
    let mut r = Lvx2Reader::new(Cursor::new(b)).unwrap();
    assert!(r.frames().next().unwrap().is_err());
    let mut r = Lvx2Reader::new(Cursor::new(&MID[..MID.len() - 1])).unwrap();
    assert!(r.frames().next().unwrap().is_err());
    let mut b = MID.to_vec();
    b[117] = 255;
    let mut r = Lvx2Reader::new(Cursor::new(b)).unwrap();
    assert!(matches!(
        r.frames().next().unwrap(),
        Err(LvxError::UnknownLidarId(_))
    ));
}
#[test]
fn unknown_device_and_time_retained() {
    let mut b = SDK.to_vec();
    b[62] = 250;
    b[121] = 3;
    let mut r = LvxReader::new(Cursor::new(b)).unwrap();
    assert_eq!(r.devices()[0].device_type, DeviceType::Unknown(250));
    let f = r.frames().next().unwrap().unwrap();
    assert_eq!(f.packets[0].header.timestamp_ns(), None);
}
#[test]
fn arbitrary_bytes_never_panic() {
    let mut seed = 42u64;
    for size in 0..4096 {
        let mut b = vec![0; size];
        for x in &mut b {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            *x = seed as u8;
        }
        if let Ok(mut r) = LvxReader::new(Cursor::new(&b)) {
            for f in r.frames() {
                let _ = f;
            }
        }
        if let Ok(mut r) = Lvx2Reader::new(Cursor::new(&b)) {
            for f in r.frames() {
                let _ = f;
            }
        }
        let _ = parse_frame(&b, 0, &[0, 7], 4096);
        let _ = lvx2::parse_frame(&b, 0, &[0], 4096);
    }
}

#[test]
fn lvx2_low_empty_frames_and_packet_bounds() {
    // Isolate a single low-resolution record with length-based framing.
    let mut b = MID[..116].to_vec();
    let mut h = MID[116..143].to_vec();
    h[17] = 2;
    h[18..22].copy_from_slice(&8u32.to_le_bytes());
    b.extend(h);
    b.extend([255, 255, 2, 0, 0, 128, 200, 17]);
    let end = b.len() as u64;
    b[100..108].copy_from_slice(&end.to_le_bytes());
    let mut r = Lvx2Reader::new(Cursor::new(&b)).unwrap();
    let f = r.frames().next().unwrap().unwrap();
    let lvx2::Lvx2Payload::CartesianLow(p) = f.packets[0].payload().unwrap() else {
        panic!()
    };
    assert_eq!(p.iter().next().unwrap().xyz_cm, [-1, 2, i16::MIN]);
    // Divisibility and truncated record header cases reach the record parser.
    for n in [117usize, 142, b.len() - 1] {
        let mut bad = b[..n].to_vec();
        bad[100..108].copy_from_slice(&(n as u64).to_le_bytes());
        let mut r = Lvx2Reader::new(Cursor::new(bad)).unwrap();
        assert!(r.frames().next().unwrap().is_err());
    }
    let mut empty = b[..116].to_vec();
    empty[100..108].copy_from_slice(&116u64.to_le_bytes());
    let mut r = Lvx2Reader::new(Cursor::new(empty)).unwrap();
    assert!(r.frames().next().unwrap().unwrap().packets.is_empty());
    assert!(matches!(
        lvx2::parse_frame(&[0; 24], u64::MAX, &[], 100),
        Err(LvxError::Overflow)
    ));
}

#[test]
fn official_lvx1_values_and_unaligned_frame() {
    let mut r = LvxReader::new(Cursor::new(TELE)).unwrap();
    let f = r.frames().next().unwrap().unwrap();
    assert_eq!(f.packets[0].header.timestamp_ns(), Some(3194290697090));
    assert_eq!(
        f.packets.last().unwrap().header.timestamp_ns(),
        Some(3194340211930)
    );
    assert_eq!(
        f.packets
            .iter()
            .filter(|p| p.header.data_type == legacy::DataType::CartesianTagged)
            .count(),
        125
    );
    let mut unaligned = vec![0xff];
    unaligned.extend_from_slice(&TELE[88..]);
    assert_eq!(
        parse_frame(&unaligned[1..], 88, &[0], DEFAULT_MAX_FRAME_BYTES).unwrap(),
        f
    );
    let mut r = LvxReader::new(Cursor::new(SDK)).unwrap();
    let f = r.frames().next().unwrap().unwrap();
    assert_eq!(
        &f.packets[0].payload[..13],
        &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]
    );
}
