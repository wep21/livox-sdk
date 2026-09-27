use livox_lvx::{Lvx2Reader, LvxReader, ReaderOptions};
use std::io::Cursor;
fn main() {
    livox_parser_fuzz::run(
        &[
            include_bytes!("../../../fixtures/sdk1-writer.lvx"),
            include_bytes!("../../../fixtures/official-mid360-first-frame.lvx2"),
        ],
        |b| {
            let options = ReaderOptions {
                max_frame_bytes: 1024 * 1024,
            };
            if let Ok(mut r) = LvxReader::with_options(Cursor::new(b), options) {
                for f in r.frames() {
                    std::hint::black_box(f).ok();
                }
            }
            if let Ok(mut r) = Lvx2Reader::with_options(Cursor::new(b), options) {
                for f in r.frames() {
                    std::hint::black_box(f).ok();
                }
            }
        },
    );
}
