//! `ev check --run` proves falsifiability by also running the bound ground's counter-test: a
//! check is only trustworthy (green/red) when its counter-test produces the OPPOSITE result. When
//! check and counter-test agree (a vacuous binding that can never flip), the verdict is
//! `unproven` and the gate fails — a check that cannot be shown to flip is not a check.

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ev() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ev"))
}

// A git repo (one empty commit, so HEAD resolves for `--run`) + an ev store; returns (path, HEAD).
fn git_repo() -> (std::path::PathBuf, String) {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "ev-cte-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    for args in [
        ["init"].as_slice(),
        ["config", "user.email", "t@e.st"].as_slice(),
        ["config", "user.name", "Tester"].as_slice(),
        ["commit", "--allow-empty", "-m", "init"].as_slice(),
    ] {
        Command::new("git")
            .args(args)
            .current_dir(&p)
            .output()
            .unwrap();
    }
    let head = String::from_utf8(
        Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&p)
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string();
    assert!(ev()
        .arg("init")
        .current_dir(&p)
        .output()
        .unwrap()
        .status
        .success());
    (p, head)
}

// Decide a chosen ground, then guard it with a runnable `check` + its `counter` test on platform
// "local", verified at `head`. The pair's logical relationship (differ ⇒ proven, agree ⇒ unproven)
// is exactly what `check --run` evaluates.
fn decide_bound(repo: &std::path::Path, check: &str, counter: &str, head: &str) {
    let out = ev()
        .args(["decide", "d", "--assume", "c", "--blame", "Wang Yu"])
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "decide failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // "recorded <id> (<n> ground(s))" — the id is the 2nd whitespace token.
    let id = String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_string();

    let out = ev()
        .args([
            "guard",
            check,
            &id,
            "c",
            "--counter-test",
            counter,
            "--on-platform",
            "local",
            "--triggered-by",
            "f",
            "--surface",
            "s",
            "--verified-at-sha",
            head,
            "--blame",
            "Wang Yu",
        ])
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "guard failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn check_run_should_be_green_when_the_counter_test_produces_the_opposite() {
    // given: a binding whose check passes and whose counter-test fails (true vs false)
    let (r, head) = git_repo();
    decide_bound(&r, "true", "false", &head);

    // when: check --run executes both
    let out = ev()
        .args(["check", "--run", "--platform", "local", "--exit-on-red"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: it is green (proven falsifiable + passing)
    assert!(
        out.status.success(),
        "stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(String::from_utf8_lossy(&out.stdout)
        .lines()
        .any(|l| l.starts_with("green\t")));
}

#[test]
fn check_run_should_be_unproven_and_gate_when_the_counter_test_cannot_execute() {
    // given: a binding whose check passes but whose counter-test is a non-runnable selector (a
    // typo / missing binary → `sh` exits 127). A broken counter "fails" for the WRONG reason, which
    // would otherwise look like it flipped (green check vs failed counter → spuriously "proven").
    // (Honest limit: a command that *intentionally* exits 126/127 is indistinguishable from a missing
    // one and would also read unproven here — we err toward gating, never a false-green.)
    let (r, head) = git_repo();
    decide_bound(&r, "true", "this_is_not_a_real_command_xyz123", &head);

    // when: check --run executes both
    let out = ev()
        .args(["check", "--run", "--platform", "local", "--exit-on-red"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: unproven, NOT green — a counter-test that could not run never proves falsifiability, so
    // the binding cannot read as a proven green (no false-green); the gate fails
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !out.status.success(),
        "a non-runnable counter-test must gate, not pass; stdout: {stdout}"
    );
    assert!(
        stdout.lines().any(|l| l.starts_with("unproven\t")),
        "must read unproven (not green); stdout: {stdout}"
    );
    assert!(
        !stdout.lines().any(|l| l.starts_with("green\t")),
        "must NOT read green; stdout: {stdout}"
    );
}

#[test]
fn check_run_should_be_unproven_and_gate_when_the_counter_test_agrees_with_the_check() {
    // given: a vacuous binding — check and counter-test both pass (true vs true)
    let (r, head) = git_repo();
    decide_bound(&r, "true", "true", &head);

    // when: check --run executes both
    let out = ev()
        .args(["check", "--run", "--platform", "local", "--exit-on-red"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: unproven — the check can't be shown to flip; gate fails
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout)
        .lines()
        .any(|l| l.starts_with("unproven\t")));
}
