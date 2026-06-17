use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ev() -> Command { Command::new(env!("CARGO_BIN_EXE_ev")) }
fn repo() -> std::path::PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!("ev-cap-cli-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    assert!(ev().arg("init").current_dir(&p).output().unwrap().status.success());
    p
}
fn run(repo: &std::path::Path, args: &[&str]) -> std::process::Output {
    ev().args(args).current_dir(repo).output().unwrap()
}

#[test]
fn capture_then_verify_round_trips_clean() {
    let r = repo();
    let out = run(&r, &[
        "decide", "build our own retrieval; reject pgvector",
        "--observe", "evaluating retrieval backend",
        "--assume", "team has bandwidth long-term", "--revisit", "Q3 review",
        "--reject", "pgvector: would lock our schema",
        "--blame", "Wang Yu",
    ]);
    assert!(out.status.success(), "decide failed: {}", String::from_utf8_lossy(&out.stderr));
    assert!(run(&r, &["verify"]).status.success(), "verify should be clean");
}

#[test]
fn force_binding_a_test_to_a_human_ground_is_a_hard_error() {
    let r = repo();
    let out = run(&r, &[
        "decide", "d",
        "--assume", "team can maintain this", "--revisit", "Q3",
        "--assume-test", "pytest x", "--counter-test", "ct",
        "--on-platform", "linux-ci", "--triggered-by", "f", "--surface", "s",
        "--verified-at-sha", "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901",
    ]);
    assert!(!out.status.success()); // R2: never both
}

#[test]
fn a_system_subject_self_evolve_decision_warns_at_decide_and_fails_verify() {
    let r = repo();
    let out = run(&r, &["decide", "the retrieval system will self-evolve its schema", "--blame", "Wang Yu"]);
    assert!(out.status.success()); // decide warns, does not fail
    assert!(String::from_utf8_lossy(&out.stderr).to_lowercase().contains("self-evolve"));
    let v = run(&r, &["verify"]);
    assert!(!v.status.success()); // verify asserts R3
}
