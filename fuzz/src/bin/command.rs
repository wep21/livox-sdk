use livox_protocol::{CommandPacket, CommandPacketOwned, crc16, crc32};
fn main() {
    livox_parser_fuzz::run(
        &[
            include_bytes!("../../../fixtures/sdk2-command.bin"),
            include_bytes!("../../../fixtures/sdk2-command-empty.bin"),
            &[1, 0, 0, 0, 0x1a, 0, 1, 0, 1],
        ],
        |b| {
            let _ = livox_protocol::control::decode_set(b);
            let _ = livox_protocol::control::decode_get(b);
            if let Ok(p) = CommandPacket::decode(b) {
                let owned = CommandPacketOwned {
                    header: p.header.clone(),
                    payload: p.payload.to_vec(),
                };
                let mut out = Vec::new();
                owned.encode(&mut out).unwrap();
                assert_eq!(out, b);
            }
            // Repair checksums too, so mutations explore fields beyond CRC rejection.
            if b.len() >= 24 && b.len() <= 1400 {
                let mut b = b.to_vec();
                let len = b.len() as u16;
                b[2..4].copy_from_slice(&len.to_le_bytes());
                let h = crc16(&b[..18]);
                b[18..20].copy_from_slice(&h.to_le_bytes());
                let c = crc32(&b[24..]);
                b[20..24].copy_from_slice(&c.to_le_bytes());
                let _ = CommandPacket::decode(&b);
            }
        },
    );
}
