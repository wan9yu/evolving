use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ev() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ev"))
}
fn repo() -> std::path::PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "ev-whyreopen-{}-{}",
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
// Decide #555-like: a chosen test-bound ground (ref "pytest x") + a rejected road. Returns its id.
fn decide_555(repo: &std::path::Path) -> String {
    let out = ev()
        .args([
            "decide",
            "restore-safety counter DB-backed; reject Redis",
            "--assume",
            "Argus introduces no Redis; multi-pod via existing DB",
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
            "--reject",
            "Redis: a new infra dependency",
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

#[test]
fn why_should_name_the_decision_when_the_selector_is_bound() {
    // given: a decision binding the selector "pytest x"
    let r = repo();
    let id = decide_555(&r);

    // when: why looks the selector up
    let out = ev()
        .args(["why", "pytest x"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: it names the decision (exit 0) and prints the decision id
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains(&id));
}

#[test]
fn why_should_fail_when_the_selector_guards_nothing() {
    // given: a store with one decision
    let r = repo();
    decide_555(&r);

    // when: why looks up an unbound selector
    let out = ev()
        .args(["why", "pytest nonexistent"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: it exits non-zero
    assert!(!out.status.success());
}

#[test]
fn reopen_should_present_the_decision_object_when_the_id_exists() {
    // given: decision #555 with a test-bound ground (no receipts) and a rejected road
    let r = repo();
    let id = decide_555(&r);

    // when: the decision is reopened
    let out = ev().args(["reopen", &id]).current_dir(&r).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);

    // then: it presents the decision, the ground's current verdict (not-run), and the road-not-taken
    assert!(out.status.success());
    assert!(stdout.contains("reject Redis"));
    assert!(stdout.contains("not-run"));
    assert!(stdout.contains("rejected:Redis"));
    assert!(stdout.contains("a new infra dependency"));
}

#[test]
fn reopen_should_fail_when_the_id_does_not_exist() {
    // given: a store with one decision
    let r = repo();
    decide_555(&r);

    // when: a non-existent id is reopened
    let out = ev()
        .args(["reopen", "deadbeefdead"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: it exits non-zero
    assert!(!out.status.success());
}
