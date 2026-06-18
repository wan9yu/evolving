use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ev() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ev"))
}
fn repo() -> std::path::PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "ev-brief-{}-{}",
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
/// Record a decision, passing through any extra flags (e.g. --authority, --reject).
fn decide(repo: &std::path::Path, text: &str, extra: &[&str]) -> String {
    let mut args: Vec<&str> = vec!["decide", text, "--assume", "a reason", "--blame", "Wang Yu"];
    args.extend_from_slice(extra);
    let out = ev().args(&args).current_dir(repo).output().unwrap();
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

#[test]
fn brief_should_show_a_user_ruled_decision_and_its_rejected_road_when_one_is_recorded() {
    // given: a user-ruled decision that records a road it rejected
    let r = repo();
    decide(
        &r,
        "keep the slice locked",
        &[
            "--authority",
            "user-ruled",
            "--reject",
            "v1.9: re-milestoned without sign-off",
        ],
    );

    // when: ev brief runs
    let out = ev().arg("brief").current_dir(&r).output().unwrap();

    // then: it exits 0 and names the decision, its authority, and the rejected road
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("keep the slice locked"));
    assert!(stdout.contains("[user-ruled]"));
    assert!(stdout.contains("rejected v1.9"));
}

#[test]
fn brief_should_omit_a_decision_that_is_not_user_ruled_when_briefing() {
    // given: a plain decision with no declared authority
    let r = repo();
    decide(&r, "a disposable decision", &[]);

    // when: ev brief runs
    let out = ev().arg("brief").current_dir(&r).output().unwrap();

    // then: it exits 0 and shows none (the decision is not user-ruled)
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.contains("a disposable decision"));
    assert!(stdout.contains("no user-ruled decisions"));
}

#[test]
fn brief_should_fail_when_there_is_no_store() {
    // given: a bare directory with no .evolving store
    let p = std::env::temp_dir().join(format!("ev-brief-nostore-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();

    // when: ev brief runs
    let out = ev().arg("brief").current_dir(&p).output().unwrap();

    // then: it errors and exits non-zero
    assert!(!out.status.success());
}
