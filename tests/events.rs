//! A local, append-only events log (results/events.jsonl): the埋点 for metrics. ev emits one
//! line per decision-shaping op (decide/guard/check/reopen). The log is a non-hashed cache —
//! deleting it never changes a tick id — so it is gitignored and best-effort.

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ev() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ev"))
}

fn repo() -> std::path::PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "ev-events-{}-{}",
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

// Record a decision; return its tick id (2nd whitespace token of "recorded <id> (...)").
fn decide(repo: &std::path::Path, text: &str) -> String {
    let out = ev()
        .args(["decide", text, "--assume", "a reason", "--blame", "Wang Yu"])
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "decide: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_string()
}

// Bind a test on linux-ci to the decision's lone unbound ground (writes a child).
fn guard(repo: &std::path::Path, parent: &str) {
    let out = ev()
        .args([
            "guard",
            "pytest x",
            parent,
            "a reason",
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
        "guard: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn decide_should_append_a_decide_event_when_a_decision_is_recorded() {
    // given: a fresh store
    let r = repo();
    // when: a decision is recorded
    let id = decide(&r, "freeze the schema");
    // then: results/events.jsonl has a decide event naming the tick
    let log = std::fs::read_to_string(r.join(".evolving/results/events.jsonl")).unwrap();
    assert!(log
        .lines()
        .any(|l| l.contains("\"op\":\"decide\"") && l.contains(&id)));
}

#[test]
fn check_should_append_a_check_event_with_the_verdict_when_evaluated() {
    // given: a decision with a test-bound ground and no receipt (not-run)
    let r = repo();
    let parent = decide(&r, "no Redis");
    guard(&r, &parent); // binds a test on linux-ci (helper)
                        // when: check evaluates
    ev().arg("check").current_dir(&r).output().unwrap();
    // then: a check event carries the not-run verdict
    let log = std::fs::read_to_string(r.join(".evolving/results/events.jsonl")).unwrap();
    assert!(log
        .lines()
        .any(|l| l.contains("\"op\":\"check\"") && l.contains("not-run")));
}

#[test]
fn decide_event_should_carry_source_ref_and_a_decision_age_bucket() {
    // given: a decision recorded with an opaque source_ref
    let r = repo();
    let out = ev()
        .args([
            "decide",
            "freeze the schema",
            "--assume",
            "a reason",
            "--source-ref",
            "R1",
            "--blame",
            "Wang Yu",
        ])
        .current_dir(&r)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "decide: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // then: the decide event carries the source_ref join key + a coarse decision-age bucket (fresh)
    let log = std::fs::read_to_string(r.join(".evolving/results/events.jsonl")).unwrap();
    let line = log
        .lines()
        .find(|l| l.contains("\"op\":\"decide\""))
        .expect("a decide event");
    assert!(
        line.contains("\"source_ref\":\"R1\""),
        "event missing source_ref: {line}"
    );
    assert!(
        line.contains("\"age\":\"fresh\""),
        "event missing age bucket: {line}"
    );
}

#[test]
fn check_should_emit_one_event_per_tick_not_per_bound_ground() {
    // given: ONE decision carrying TWO test-bound grounds
    let r = repo();
    let out = ev()
        .args([
            "decide",
            "two checks",
            "--assume",
            "claim a",
            "--assume-test",
            "pytest a",
            "--counter-test",
            "pytest a::flips",
            "--on-platform",
            "linux-ci",
            "--triggered-by",
            "f1",
            "--surface",
            "s1",
            "--assume",
            "claim b",
            "--assume-test",
            "pytest b",
            "--counter-test",
            "pytest b::flips",
            "--on-platform",
            "linux-ci",
            "--triggered-by",
            "f2",
            "--surface",
            "s2",
            "--verified-at-sha",
            "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901",
            "--blame",
            "Wang Yu",
        ])
        .current_dir(&r)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "decide: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // when: check evaluates the tick (both grounds not-run, no receipts)
    ev().arg("check").current_dir(&r).output().unwrap();

    // then: exactly ONE check event for the tick (de-quintupled — one decision, one check), not one
    // per bound ground (the old behavior emitted one per ground, all stamped the same tick).
    let log = std::fs::read_to_string(r.join(".evolving/results/events.jsonl")).unwrap();
    let checks = log
        .lines()
        .filter(|l| l.contains("\"op\":\"check\""))
        .count();
    assert_eq!(
        checks, 1,
        "expected one check event per tick, got {checks}; log:\n{log}"
    );
}
