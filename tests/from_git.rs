//! `ev decide --from-git <commit>` — seed a decision from a commit's ENVELOPE
//! (subject + author + Refs), never inferring grounds from the body.
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ev() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ev"))
}

/// A fresh git repo with one commit (subject + body) and an initialized ev store.
fn repo_with_commit(subject: &str, body: &str) -> std::path::PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "ev-fromgit-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    for args in [
        ["init"].as_slice(),
        ["config", "user.email", "t@e.st"].as_slice(),
        ["config", "user.name", "Tester"].as_slice(),
    ] {
        assert!(Command::new("git")
            .args(args)
            .current_dir(&p)
            .output()
            .unwrap()
            .status
            .success());
    }
    let message = format!("{subject}\n\n{body}");
    assert!(Command::new("git")
        .args(["commit", "--allow-empty", "-m", &message])
        .current_dir(&p)
        .output()
        .unwrap()
        .status
        .success());
    assert!(ev()
        .arg("init")
        .current_dir(&p)
        .output()
        .unwrap()
        .status
        .success());
    p
}

/// The id printed by a successful `recorded <id> (<n> ground(s))` line.
fn recorded_id(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_string()
}

#[test]
fn decide_should_take_the_decision_from_the_commit_subject_when_from_git_is_used() {
    // given: a commit whose subject is the decision and whose body carries a Refs line
    let r = repo_with_commit("freeze v1.8; reject v1.9", "Refs #1194");

    // when: a decision is seeded from that commit, with human-authored grounds
    let out = ev()
        .args([
            "decide",
            "--from-git",
            "HEAD",
            "--assume",
            "team agreed",
            "--reject",
            "v1.9: re-milestoned without sign-off",
            "--authority",
            "user-ruled",
            "--blame",
            "Wang Yu",
        ])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: it succeeds, the decision is the commit subject, and the Refs land in observe
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let id = recorded_id(&out);
    let raw = std::fs::read_to_string(r.join(".evolving/ticks").join(&id)).unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["decision"], "freeze v1.8; reject v1.9");
    assert!(
        v["observe"].as_str().unwrap().contains("Refs #1194"),
        "observe was {:?}",
        v["observe"]
    );
}

#[test]
fn decide_should_still_take_an_explicit_positional_decision_when_from_git_is_absent() {
    // given: an initialized repo (the commit is irrelevant to a positional decision)
    let r = repo_with_commit("ignored subject", "Refs #9");

    // when: a normal decision with an explicit positional text is recorded
    let out = ev()
        .args([
            "decide",
            "explicit text",
            "--assume",
            "y",
            "--revisit",
            "Q3",
            "--blame",
            "Z",
        ])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: it succeeds and records the explicit positional decision
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let id = recorded_id(&out);
    let raw = std::fs::read_to_string(r.join(".evolving/ticks").join(&id)).unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["decision"], "explicit text");
}

#[test]
fn decide_should_fail_when_both_a_positional_decision_and_from_git_are_given() {
    // given: a commit to seed from
    let r = repo_with_commit("subject", "Refs #1");

    // when: both a positional decision and --from-git are supplied
    let out = ev()
        .args([
            "decide",
            "explicit text",
            "--from-git",
            "HEAD",
            "--assume",
            "y",
            "--blame",
            "Z",
        ])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: the decision source is ambiguous and it is refused
    assert!(!out.status.success());
}

#[test]
fn decide_should_fail_when_the_commit_cannot_be_read() {
    // given: a repo with no commit matching the given rev
    let r = repo_with_commit("subject", "Refs #1");

    // when: --from-git names an unresolvable commit
    let out = ev()
        .args([
            "decide",
            "--from-git",
            "deadbeef",
            "--assume",
            "y",
            "--blame",
            "Z",
        ])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: it cannot read the commit and exits non-zero
    assert!(!out.status.success());
}
