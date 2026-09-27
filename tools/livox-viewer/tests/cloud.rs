use livox_lvx::Lvx2Reader;
use livox_protocol::{DataHeader, DataPacketOwned, TimeType};
use livox_viewer::{Cloud, FileSource, LiveState, Point};
use std::{
    io::Cursor,
    net::SocketAddr,
    path::PathBuf,
    time::{Duration, Instant},
};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/official-mid360-first-frame.lvx2")
}

fn decode(bytes: &[u8]) -> (serde_json::Value, Vec<[f32; 4]>) {
    let length = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
    let status = serde_json::from_slice(&bytes[4..4 + length]).unwrap();
    let points = bytes[4 + length..]
        .chunks_exact(16)
        .map(|p| {
            std::array::from_fn(|i| f32::from_le_bytes(p[i * 4..i * 4 + 4].try_into().unwrap()))
        })
        .collect();
    (status, points)
}

#[test]
fn official_record_units_counts_eof_and_reset() {
    let mut source = FileSource::open(&fixture(), None, 20_000, 5).unwrap();
    let (status, points) = decode(&source.advance().unwrap());
    assert_eq!(status["packets"], 104);
    assert_eq!(status["timestamp"], "711670239460");
    assert_eq!(status["frame"], 0);
    let mut r = Lvx2Reader::new(std::io::BufReader::new(
        std::fs::File::open(fixture()).unwrap(),
    ))
    .unwrap();
    let frame = r.frames().next().unwrap().unwrap();
    let expected: Vec<_> = frame
        .packets
        .iter()
        .flat_map(|p| p.payload_bytes().chunks_exact(14))
        .filter(|b| b[..12] != [0; 12])
        .map(|b| {
            let xyz: [f32; 3] = std::array::from_fn(|i| {
                i32::from_le_bytes(b[i * 4..i * 4 + 4].try_into().unwrap()) as f32 * 0.001
            });
            [xyz[0], xyz[1], xyz[2], f32::from(b[12])]
        })
        .collect();
    assert!(!points.is_empty());
    assert_eq!(points, expected);
    assert_eq!(status["points"], points.len());
    let (end, still) = decode(&source.advance().unwrap());
    assert_eq!(end["ended"], true);
    assert_eq!(still, points);
    let (reset, again) = decode(&source.reset().unwrap());
    assert_eq!(reset, status);
    assert_eq!(again, points);
    assert!(FileSource::open(&fixture(), Some(u32::MAX), 100, 1).is_err());
}

#[test]
fn history_count_and_time_bounds_keep_newest_points() {
    let t = Instant::now();
    let mut c = Cloud::new(3).unwrap();
    let p = |x| Point([x, 0., 1., 10.]);
    c.push(vec![p(1.), p(2.)], t, 5);
    c.push(vec![p(3.), p(4.)], t + Duration::from_millis(5), 5);
    assert_eq!(
        decode(&c.encode().unwrap())
            .1
            .iter()
            .map(|p| p[0])
            .collect::<Vec<_>>(),
        [2., 3., 4.]
    );
    c.expire(t + Duration::from_millis(12), Duration::from_millis(10));
    assert_eq!(c.status.points, 2);
    c.push(vec![], t + Duration::from_millis(13), 1);
    assert_eq!(c.status.points, 0);
    c.push(vec![p(1.), p(2.), p(3.), p(4.)], t, 1);
    assert_eq!(decode(&c.encode().unwrap()).1[0][0], 2.);
    assert!(Cloud::new(0).is_err());
}

fn low_packet() -> Vec<u8> {
    let mut out = vec![];
    DataPacketOwned {
        header: DataHeader {
            length: 0,
            time_interval: 0,
            dot_num: 2,
            udp_cnt: 0,
            frame_cnt: 0,
            data_type: 2,
            time_type: TimeType::Ptp,
            reserved: [0; 12],
            crc32: 0,
            timestamp: u64::MAX,
        },
        payload: vec![100, 0, 56, 255, 44, 1, 128, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    }
    .encode(&mut out)
    .unwrap();
    out
}

#[test]
fn udp_low_units_zero_returns_crc_unknown_and_sender_filter() {
    let mut state = LiveState::new(100, None).unwrap();
    let peer: SocketAddr = "127.0.0.1:56300".parse().unwrap();
    let other: SocketAddr = "127.0.0.2:56300".parse().unwrap();
    let t = Instant::now();
    let bytes = low_packet();
    state.receive(&[0; 4], other, t); // malformed sender must not acquire the lock
    state.receive(include_bytes!("../../../fixtures/sdk2-imu.bin"), other, t);
    state.receive(&bytes, peer, t);
    let (s, points) = decode(&state.cloud.encode().unwrap());
    assert_eq!(points, [[1., -2., 3., 128.]]);
    assert_eq!(s["timestamp"], u64::MAX.to_string());
    assert_eq!(s["invalid"], 1);
    assert_eq!(s["skipped"], 1);
    state.receive(&bytes, other, t);
    let mut corrupt = bytes.clone();
    corrupt[28] ^= 1;
    state.receive(&corrupt, peer, t);
    assert_eq!(state.cloud.status.foreign, 1);
    assert_eq!(state.cloud.status.invalid, 2);
    let mut unknown = bytes.clone();
    unknown[10] = 250; // header type isn't in the data CRC
    state.receive(&unknown, peer, t);
    assert_eq!(state.cloud.status.skipped, 2);
    let mut filtered = LiveState::new(100, Some(other.ip())).unwrap();
    filtered.receive(&bytes, peer, t);
    assert_eq!(filtered.cloud.status.foreign, 1);
}

#[test]
fn file_and_udp_conversion_match_for_official_capture() {
    let bytes = include_bytes!("../../../fixtures/official-mid360-first-frame.lvx2");
    let mut r = Lvx2Reader::new(Cursor::new(bytes)).unwrap();
    let mut state = LiveState::new(20_000, None).unwrap();
    let now = Instant::now();
    for record in r.frames().next().unwrap().unwrap().packets {
        let bytes = livox_sim::encode_record(&record, 0).unwrap();
        state.receive(&bytes, "127.0.0.1:56300".parse().unwrap(), now);
    }
    let mut source = FileSource::open(&fixture(), None, 20_000, 5).unwrap();
    assert_eq!(
        decode(&source.advance().unwrap()).1,
        decode(&state.cloud.encode().unwrap()).1
    );
}
