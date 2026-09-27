use std::process::Command;
fn fixture(name: &str) -> String {
    format!("{}/../../fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}
#[test]
fn scans_both_official_formats_offline() {
    for (name, format, count) in [
        (
            "official-mid360-first-frame.lvx2",
            "format: LVX 2.0",
            "CartesianHigh: 104 packets",
        ),
        (
            "official-tele15-first-frame.lvx",
            "format: LVX 1.1",
            "Imu: 10 packets",
        ),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_lvx-info"))
            .arg(fixture(name))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains(format));
        assert!(text.contains("frames: 1"));
        assert!(text.contains(count));
    }
}
#[test]
fn header_only_and_error_exit() {
    let output = Command::new(env!("CARGO_BIN_EXE_lvx-info"))
        .args([
            fixture("official-mid360-first-frame.lvx2"),
            "--header-only".into(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("frames: not scanned"));
    let output = Command::new(env!("CARGO_BIN_EXE_lvx-info"))
        .args([
            fixture("official-mid360-first-frame.lvx2"),
            "--max-frame-bytes".into(),
            "24".into(),
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("exceeds configured limit"));
}
