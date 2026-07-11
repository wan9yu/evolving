use std::process::Command;

#[test]
fn version_flag_prints_semver() {
    let out = Command::new(env!("CARGO_BIN_EXE_ev"))
        .arg("--version")
        .output()
        .expect("binary runs");
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success());
    // no version literal here — the test asserts the binary reports the
    // crate's own version, whatever it currently is.
    assert!(
        s.contains(env!("CARGO_PKG_VERSION")),
        "version output was: {s}"
    );
}
