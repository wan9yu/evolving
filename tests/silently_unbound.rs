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
        "ev-l2-{}-{}",
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
// A test-bound ground: ref "pytest x" on platform "linux-ci", triggered by "pyproject.toml".
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
fn green_receipt(repo: &std::path::Path) {
    append(
        &Store::at(repo),
        &Receipt {
            test: "pytest x".into(),
            platform: "linux-ci".into(),
            commit: "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901".into(),
            ran_at: "2026-01-01T00:00:00Z".into(),
            result: "green".into(),
        },
    )
    .unwrap();
}
// Widen the staleness window so age-staleness never fires — isolates the L2 (silently-unbound)
// axis from the time-stale axis (this fixture uses a fixed past date, evaluated against the real clock).
fn disable_age_staleness(repo: &std::path::Path) {
    let s = Store::at(repo);
    let cfg = std::fs::read_to_string(s.config_path())
        .unwrap()
        .replace("staleness_days = 7", "staleness_days = 3650000");
    std::fs::write(s.config_path(), cfg).unwrap();
}
fn write_selected(repo: &std::path::Path, json: &str) {
    std::fs::write(repo.join(".evolving/results/selected.json"), json).unwrap();
}

#[test]
fn check_should_flag_silently_unbound_and_gate_when_a_touched_trigger_was_not_selected() {
    // given: a green-on-receipts binding, and a selected-list that changed its trigger but did not select it
    let r = repo();
    decide_bound(&r);
    disable_age_staleness(&r);
    green_receipt(&r);
    write_selected(
        &r,
        r#"{"commit":"d308afac1b2c3d4e5f60718293a4b5c6d7e8f901","changed":["pyproject.toml"],"selected":[]}"#,
    );

    // when: check runs with gating
    let out = ev()
        .args(["check", "--exit-on-red"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: it is silently-unbound and the gate fails
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("silently-unbound"));
}

#[test]
fn check_should_be_green_and_pass_when_the_touched_trigger_was_selected() {
    // given: the same binding, but the selected-list did select its ref
    let r = repo();
    decide_bound(&r);
    disable_age_staleness(&r);
    green_receipt(&r);
    write_selected(
        &r,
        r#"{"commit":"d308afac1b2c3d4e5f60718293a4b5c6d7e8f901","changed":["pyproject.toml"],"selected":["pytest x"]}"#,
    );

    // when: check runs with gating
    let out = ev()
        .args(["check", "--exit-on-red"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: it is green and the gate passes
    assert!(
        out.status.success(),
        "stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("green"));
}
