use ev::receipt::{append, Receipt};
use ev::store::Store;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ev() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ev"))
}
fn repo() -> std::path::PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "ev-age-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    assert!(ev()
        .arg("init")
        .current_dir(&p)
        .output()
        .unwrap()
        .status
        .success());
    p
}
fn decide_bound(repo: &std::path::Path) {
    let out = ev()
        .args([
            "decide",
            "no-Redis posture",
            "--assume",
            "no Redis; multi-pod via existing DB",
            "--assume-test",
            "pytest x",
            "--counter-test",
            "pytest x::flips",
            "--on-platform",
            "linux-ci",
            "--triggered-by",
            "pyproject.toml",
            "--surface",
            "pyproject-deps",
            "--verified-at-sha",
            "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901",
            "--blame",
            "Wang Yu",
        ])
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "decide failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn check_should_flag_stale_and_gate_when_the_only_receipt_is_older_than_the_window() {
    // given: a green receipt from the distant past and a 1-day staleness window
    let r = repo();
    decide_bound(&r);
    let s = Store::at(&r);
    let cfg = std::fs::read_to_string(s.config_path())
        .unwrap()
        .replace("staleness_days = 7", "staleness_days = 1");
    std::fs::write(s.config_path(), cfg).unwrap();
    append(
        &s,
        &Receipt {
            test: "pytest x".into(),
            platform: "linux-ci".into(),
            commit: "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901".into(),
            ran_at: "2020-01-01T00:00:00Z".into(),
            result: "green".into(),
            falsifiable: None,
        },
    )
    .unwrap();

    // when: check runs with gating
    let out = ev()
        .args(["check", "--exit-on-red"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: it is stale and the gate fails (never green)
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("stale"));
}
