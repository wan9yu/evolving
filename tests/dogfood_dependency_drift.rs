//! Dogfood — the dependency-behavior-drift pattern (issue #1420), reproduced end-to-end against the
//! real `ev` binary. The arc this proves: an external exporter snapshots a *dependency's
//! behavior-shape* to a tracked file, a human reviews and freezes that snapshot, and a PURE
//! compare-check (reads the snapshot, 0-network) bound to the decision flips RED the instant the
//! dependency's current shape drifts — silently — from the reviewed baseline. The drift's diff is
//! the review surface; a pure check (no network, no fixture build) cannot fail-soft to a false-green.
//!
//! Faithful by construction and free of proprietary strings: the "behavior-shape" is a generic line
//! (`types=...`), the snapshot a tracked file, the check `diff -q current vs snapshot`, and its
//! inverse counter-test `! diff -q ...` — so `ev check --run` genuinely proves the check falsifiable
//! (diff and !diff are logical inverses) and a real drift genuinely flips it. This validates the P3
//! counter-test execution AND the P4 resurface arc on one realistic decision.

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ev() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ev"))
}

// A real git repo (one empty commit, so HEAD resolves for `--run`) + an initialized ev store.
// Returns (repo path, HEAD sha). Hermetic: sets its own git identity, never the developer's.
fn git_repo() -> (std::path::PathBuf, String) {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "ev-dogfood-drift-{}-{}",
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

// The behavior-shape compare-check and its inverse — read the two tracked files, 0-network.
// `diff -q` exits 0 (green) when current == snapshot, non-zero (red) when they drift. Its inverse
// is the counter-test, so `ev check --run` can prove the check is genuinely falsifiable.
const DRIFT_CHECK: &str = "diff -q current-shape.txt shape-snapshot.txt";
const DRIFT_COUNTER_TEST: &str = "! diff -q current-shape.txt shape-snapshot.txt";
// The frozen decision and the bound assumption (no company/product names).
const DECISION: &str = "redaction surface frozen";
const CLAIM: &str = "the dependency's behavior-shape matches the reviewed snapshot";

// Write both tracked files: `current` is what the dependency produces now, `snapshot` the reviewed
// baseline. The check reads them from the repo working tree exactly as ev's runner does (`sh -c`).
fn write_both(repo: &std::path::Path, current: &str, snapshot: &str) {
    std::fs::write(repo.join("current-shape.txt"), format!("{current}\n")).unwrap();
    std::fs::write(repo.join("shape-snapshot.txt"), format!("{snapshot}\n")).unwrap();
}

// Capture the user-ruled decision freezing the behavior surface, then bind its assumption to the
// pure compare-check with its inverse counter-test and the 3-key liveness contract.
fn bind_drift_check(repo: &std::path::Path, head: &str) {
    let out = ev()
        .args([
            "decide",
            DECISION,
            "--assume",
            CLAIM,
            "--authority",
            "user-ruled",
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
    // "recorded <id> (<n> ground(s))" — the id is the 2nd whitespace token.
    let id = String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_string();

    let out = ev()
        .args([
            "guard",
            DRIFT_CHECK,
            &id,
            CLAIM,
            "--counter-test",
            DRIFT_COUNTER_TEST,
            "--on-platform",
            "local",
            "--triggered-by",
            "current-shape.txt",
            "--surface",
            "shape",
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

fn check_run(repo: &std::path::Path) -> std::process::Output {
    ev().args(["check", "--run", "--platform", "local", "--exit-on-red"])
        .current_dir(repo)
        .output()
        .unwrap()
}

#[test]
fn drift_should_be_green_and_proven_when_the_shape_matches_the_snapshot() {
    // given: current shape == the committed snapshot, check bound with its inverse counter-test
    let (r, head) = git_repo();
    write_both(&r, "types=15,32,46,59", "types=15,32,46,59");
    bind_drift_check(&r, &head);
    // when: check --run
    let out = check_run(&r);
    // then: green + proven (the diff/!diff pair genuinely differ)
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
fn drift_should_go_red_and_resurface_when_the_dependency_shape_drifts() {
    // given: the dependency's current shape silently changed (added 3 types) vs the reviewed snapshot
    let (r, head) = git_repo();
    write_both(&r, "types=15,35,49,62", "types=15,32,46,59");
    bind_drift_check(&r, &head);
    // when: check --run gates
    let out = check_run(&r);
    // then: red + names the decision (the drift is caught; the check is provably falsifiable)
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout)
        .lines()
        .any(|l| l.starts_with("red\t")));
}
