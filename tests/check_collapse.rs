//! `ev check` collapses a corrected lineage to its current decision — exactly as brief/list do — so a
//! correction that supersedes (or demotes) a decision takes effect at the GATE, and a stale corrected
//! tick neither prints a duplicate row nor gates.
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ev() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ev"))
}
fn repo() -> std::path::PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "ev-collapse-{}-{}",
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
fn run(repo: &std::path::Path, args: &[&str]) -> std::process::Output {
    ev().args(args).current_dir(repo).output().unwrap()
}
// A test-bound decision declaring a platform with no receipt -> not-run -> it GATES. Returns its id.
fn decide_bound(repo: &std::path::Path) -> String {
    let out = run(
        repo,
        &[
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
        ],
    );
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
fn check_should_not_gate_on_a_corrected_tick_when_the_correction_demotes_it_to_non_gating() {
    // given: a test-bound decision (default authority) whose check is not-run (it GATES today)
    let r = repo();
    let id = decide_bound(&r);
    assert!(
        !run(&r, &["check", "--exit-on-red"]).status.success(),
        "the bound not-run decision must gate before any correction"
    );

    // when: a human supersedes its provenance to agent-proposed — LOCK 3 makes an agent-proposed tick
    // structurally non-gating — minting a child that supersedes the original via a `supersedes` edge
    assert!(run(
        &r,
        &[
            "supersede",
            &id,
            "--provenance",
            "agent-proposed",
            "--blame",
            "You"
        ]
    )
    .status
    .success());

    // then: check collapses the lineage to the current (agent-proposed) tick, which cannot gate, so
    // the gate now PASSES. Before the collapse fix the stale parent kept gating.
    let out = run(&r, &["check", "--exit-on-red"]);
    assert!(
        out.status.success(),
        "a corrected (demoted) decision must stop gating; stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn check_should_show_a_corrected_decision_once_not_once_per_lineage_tick() {
    // given: a test-bound decision, corrected once via a non-gating tag change — so both ticks keep
    // the SAME verdict and only the collapse (not the verdict) decides the row count
    let r = repo();
    let id = decide_bound(&r);
    assert!(run(
        &r,
        &[
            "supersede",
            &id,
            "--authority",
            "agent-disposable",
            "--blame",
            "You"
        ]
    )
    .status
    .success());

    // when: check runs
    let out = run(&r, &["check"]);
    let s = String::from_utf8_lossy(&out.stdout);

    // then: the decision's ground appears in exactly ONE row (the current tick), not one per
    // lineage tick — before the fix the stale parent printed a duplicate row
    assert_eq!(
        s.matches("no Redis; multi-pod via existing DB").count(),
        1,
        "a corrected lineage collapses to one check row; check was {s:?}"
    );
}
