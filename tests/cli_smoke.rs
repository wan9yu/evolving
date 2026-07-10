use std::process::Command;

#[test]
fn version_flag_prints_semver() {
    let out = Command::new(env!("CARGO_BIN_EXE_ev"))
        .arg("--version")
        .output()
        .expect("binary runs");
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success());
    assert!(s.contains("0.2.0"), "version output was: {s}");
}
