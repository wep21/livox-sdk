use livox_protocol::{DataPacket, DataPacketOwned, DataPayload, crc32};
fn check(b: &[u8]) {
    if let Ok(p) = DataPacket::decode(b) {
        if b.len() <= 65507 {
            let owned = DataPacketOwned {
                header: p.header.clone(),
                payload: b[36..].to_vec(),
            };
            let mut out = Vec::new();
            owned.encode(&mut out).unwrap();
            assert_eq!(out, b);
        }
        match p.payload {
            DataPayload::CartesianHigh(x) => {
                for p in x.iter() {
                    std::hint::black_box(p);
                }
            }
            DataPayload::CartesianLow(x) => {
                for p in x.iter() {
                    std::hint::black_box(p);
                }
            }
            DataPayload::Spherical(x) => {
                for p in x.iter() {
                    std::hint::black_box(p);
                }
            }
            _ => {}
        }
    }
}
fn main() {
    livox_parser_fuzz::run(
        &[
            include_bytes!("../../../fixtures/sdk2-high.bin"),
            include_bytes!("../../../fixtures/sdk2-imu.bin"),
        ],
        |b| {
            check(b);
            if b.len() >= 36 && b.len() <= 65535 {
                let mut b = b.to_vec();
                let len = b.len() as u16;
                b[1..3].copy_from_slice(&len.to_le_bytes());
                let c = crc32(&b[28..]);
                b[24..28].copy_from_slice(&c.to_le_bytes());
                check(&b);
            }
        },
    );
}
