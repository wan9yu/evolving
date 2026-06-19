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
        "ev-resurface-{}-{}",
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
// Decide a tick whose chosen ground is test-bound to `pytest x` on platform `linux-ci`.
fn decide_bound(repo: &std::path::Path) -> String {
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
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_string()
}
fn rcpt(ran_at: &str, result: &str) -> Receipt {
    Receipt {
        test: "pytest x".into(),
        platform: "linux-ci".into(),
        commit: "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901".into(),
        ran_at: ran_at.into(),
        result: result.into(),
        falsifiable: None,
    }
}
// Widen the staleness window so age-staleness never fires — isolates the green/red/gray axes
// from the time-stale axis (these fixtures use fixed past dates, evaluated against the real clock).
fn disable_age_staleness(repo: &std::path::Path) {
    let s = Store::at(repo);
    let cfg = std::fs::read_to_string(s.config_path())
        .unwrap()
        .replace("staleness_days = 7", "staleness_days = 3650000");
    std::fs::write(s.config_path(), cfg).unwrap();
}

#[test]
fn check_should_report_not_run_and_gate_when_no_receipt_exists() {
    // given: a test-bound decision with no receipts
    let r = repo();
    decide_bound(&r);

    // when: check runs (plain, then gating)
    let plain = ev().arg("check").current_dir(&r).output().unwrap();
    let gated = ev()
        .args(["check", "--exit-on-red"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: plain reports not-run and exits 0; --exit-on-red exits non-zero
    assert!(plain.status.success());
    assert!(String::from_utf8_lossy(&plain.stdout).contains("not-run"));
    assert!(!gated.status.success());
}

#[test]
fn check_should_report_green_and_pass_when_a_green_receipt_covers_the_platform() {
    // given: a test-bound decision with a green receipt for its platform
    let r = repo();
    decide_bound(&r);
    disable_age_staleness(&r);
    append(&Store::at(&r), &rcpt("2026-01-01T00:00:00Z", "green")).unwrap();

    // when: check runs with gating
    let out = ev()
        .args(["check", "--exit-on-red"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: it reports green and exits 0
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("green"));
}

#[test]
fn check_should_go_red_and_gate_when_the_latest_receipt_is_red() {
    // given: a green receipt followed by a later red one for the same binding
    let r = repo();
    decide_bound(&r);
    disable_age_staleness(&r);
    let s = Store::at(&r);
    append(&s, &rcpt("2026-01-01T00:00:00Z", "green")).unwrap();
    append(&s, &rcpt("2026-02-01T00:00:00Z", "red")).unwrap();

    // when: check runs with gating
    let out = ev()
        .args(["check", "--exit-on-red"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: the latest (red) decides and the gate fails
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("red"));
}

#[test]
fn check_should_promote_gray_to_red_when_the_receipt_is_gray() {
    // given: a test-bound decision whose only receipt is gray
    let r = repo();
    decide_bound(&r);
    disable_age_staleness(&r);
    append(&Store::at(&r), &rcpt("2026-01-01T00:00:00Z", "gray")).unwrap();

    // when: check runs with gating
    let out = ev()
        .args(["check", "--exit-on-red"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: gray is promoted to red and gates
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("gray->red"));
}

#[test]
fn deleting_results_should_leave_the_tick_id_unchanged_when_the_chain_is_rebuilt() {
    // given: a test-bound decision and a written receipt
    let r = repo();
    let id = decide_bound(&r);
    append(&Store::at(&r), &rcpt("2026-01-01T00:00:00Z", "green")).unwrap();

    // when: the entire results/ cache is deleted
    std::fs::remove_dir_all(r.join(".evolving/results")).unwrap();

    // then: the tick still shows under the same id (the hashed/cached split)
    let shown = ev().args(["show", &id]).current_dir(&r).output().unwrap();
    assert!(shown.status.success());
    assert!(String::from_utf8_lossy(&shown.stdout).contains(&id));
}
