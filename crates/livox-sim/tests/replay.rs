use livox_lvx::Lvx2Reader;
use livox_protocol::DataPacket;
use livox_sim::{encode_record, replay_deadline};
use std::io::Cursor;
const MID: &[u8] = include_bytes!("../../../fixtures/official-mid360-first-frame.lvx2");
#[test]
fn reconstructed_udp_preserves_record() {
    let mut r = Lvx2Reader::new(Cursor::new(MID)).unwrap();
    let f = r.frames().next().unwrap().unwrap();
    let p = &f.packets[0];
    let out = encode_record(p, 5000).unwrap();
    let decoded = DataPacket::decode(&out).unwrap();
    assert_eq!(decoded.header.timestamp, p.timestamp);
    assert_eq!(decoded.header.dot_num, 96);
    assert_eq!(decoded.header.time_interval, 4750);
    assert_eq!(&out[36..], p.payload_bytes());
    assert!(encode_record(p, u64::MAX).is_err());
    assert!(encode_record(p, 1).is_err());
}
#[test]
fn replay_clock_checked_math() {
    assert_eq!(
        replay_deadline(100, 1_000_000_100, 2.).unwrap().as_millis(),
        500
    );
    assert!(replay_deadline(100, 99, 1.).is_err());
    for speed in [0., -1., f64::NAN, f64::INFINITY, f64::MIN_POSITIVE] {
        assert!(replay_deadline(0, u64::MAX, speed).is_err());
    }
}
