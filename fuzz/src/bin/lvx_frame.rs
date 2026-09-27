use livox_lvx::{lvx2, parse_frame};
fn main() {
    let sdk = include_bytes!("../../../fixtures/sdk1-writer.lvx");
    let mid = include_bytes!("../../../fixtures/official-mid360-first-frame.lvx2");
    livox_parser_fuzz::run(&[&sdk[88..sdk.len() - 24], &mid[92..]], |b| {
        let _ = parse_frame(b, 88, &[7], 1024 * 1024);
        let _ = lvx2::parse_frame(b, 92, &[738306240], 1024 * 1024);
        // Repair boundaries to reach individual record parsing after mutation.
        if b.len() >= 24 {
            for (position, v2) in [(88u64, false), (92, true)] {
                let mut b = b.to_vec();
                let end = position + b.len() as u64;
                b[..8].copy_from_slice(&position.to_le_bytes());
                b[8..16].copy_from_slice(&end.to_le_bytes());
                if v2 {
                    let _ = lvx2::parse_frame(&b, position, &[738306240], 1024 * 1024);
                } else {
                    let _ = parse_frame(&b, position, &[7], 1024 * 1024);
                }
            }
        }
    });
}
