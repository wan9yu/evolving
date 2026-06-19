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
fn brief_should_accept_a_limit_flag_when_one_is_passed() {
    // given: a user-ruled decision in the store
    let r = repo();
    decide(&r, "keep the slice locked", &["--authority", "user-ruled"]);

    // when: ev brief runs with --limit
    let out = ev()
        .args(["brief", "--limit", "1"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: the flag is accepted (exits 0) and the decision still shows
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("keep the slice locked"));
}

#[test]
fn brief_should_order_user_ruled_decisions_most_recent_first_when_multiple_exist() {
    // given: two user-ruled decisions recorded in order — A first, then B (B is later)
    let r = repo();
    decide(&r, "decision A", &["--authority", "user-ruled"]);
    decide(&r, "decision B", &["--authority", "user-ruled"]);

    // when: ev brief runs
    let out = ev().arg("brief").current_dir(&r).output().unwrap();

    // then: B's text appears before A's (most-recent-first by held_since)
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let b = stdout.find("decision B").expect("B shown");
    let a = stdout.find("decision A").expect("A shown");
    assert!(b < a, "B should appear before A:\n{stdout}");
}

#[test]
fn brief_should_cap_and_note_the_remainder_when_over_the_limit() {
    // given: three user-ruled decisions
    let r = repo();
    decide(&r, "decision one", &["--authority", "user-ruled"]);
    decide(&r, "decision two", &["--authority", "user-ruled"]);
    decide(&r, "decision three", &["--authority", "user-ruled"]);

    // when: ev brief runs with --limit 2
    let out = ev()
        .args(["brief", "--limit", "2"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: exactly two decisions show plus a remainder footer pointing at `ev list`
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let shown = ["decision one", "decision two", "decision three"]
        .iter()
        .filter(|d| stdout.contains(**d))
        .count();
    assert_eq!(shown, 2, "exactly two decisions shown:\n{stdout}");
    assert!(stdout.contains("1 more"), "remainder count:\n{stdout}");
    assert!(stdout.contains("ev list"), "points at ev list:\n{stdout}");
}

#[test]
fn brief_should_show_all_when_limit_is_zero() {
    // given: three user-ruled decisions
    let r = repo();
    decide(&r, "decision one", &["--authority", "user-ruled"]);
    decide(&r, "decision two", &["--authority", "user-ruled"]);
    decide(&r, "decision three", &["--authority", "user-ruled"]);

    // when: ev brief runs with --limit 0
    let out = ev()
        .args(["brief", "--limit", "0"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: all three show and there is no remainder footer
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let shown = ["decision one", "decision two", "decision three"]
        .iter()
        .filter(|d| stdout.contains(**d))
        .count();
    assert_eq!(shown, 3, "all three shown:\n{stdout}");
    assert!(!stdout.contains("more user-ruled"), "no footer:\n{stdout}");
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
