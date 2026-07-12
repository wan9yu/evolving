use std::process::Command;

fn run(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ev"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

fn git(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .output()
        .unwrap()
}

fn fresh_git() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ev-vfy-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q"]);
    std::fs::write(dir.join("f.txt"), "x").unwrap();
    git_commit(&dir, "first");
    dir
}

fn git_commit(dir: &std::path::Path, msg: &str) {
    git(dir, &["add", "-A"]);
    git(dir, &["-c", "commit.gpgsign=false", "commit", "-qm", msg]);
}

fn ledger_path(dir: &std::path::Path) -> std::path::PathBuf {
    std::fs::read_dir(dir.join(".evolving/ledger"))
        .unwrap()
        .filter_map(|e| e.ok())
        .find(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
        .unwrap()
        .path()
}

fn ledger_events(dir: &std::path::Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(ledger_path(dir))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

/// The id of the most recently filed claim — the one the test just made.
fn claim_id(dir: &std::path::Path) -> String {
    let mut last = None;
    for v in ledger_events(dir) {
        if v["type"] == "claim" {
            last = Some(v["id"].as_str().unwrap().to_string());
        }
    }
    last.expect("no claim event in the ledger")
}

#[test]
fn verify_should_skip_self_evident_evidence_by_default_and_include_it_under_full() {
    let dir = fresh_git();
    std::fs::write(dir.join("a.txt"), "1\n").unwrap();
    git_commit(&dir, "one");
    assert!(run(&dir, &["init"]).status.success());
    std::fs::write(dir.join("b.txt"), "2\n").unwrap();
    git_commit(&dir, "two");

    // exhaust files self-evident commit: evidence for the window
    assert!(
        run(&dir, &["exhaust", "--since", "HEAD~1", "--session", "s1"])
            .status
            .success()
    );
    // an agent files a real, non-self-evident anchor
    assert!(run(&dir, &["claim", "real", "--by", "agent"])
        .status
        .success());
    let id = claim_id(&dir);
    assert!(run(&dir, &["evidence", &id, "file:a.txt::1"])
        .status
        .success());

    let default = run(&dir, &["verify", "--json"]);
    assert!(default.status.success());
    let d: serde_json::Value =
        serde_json::from_slice(&default.stdout).expect("verify --json must be valid json");
    let checks = d["checks"].as_array().unwrap();
    assert!(
        checks
            .iter()
            .all(|c| !c["ref"].as_str().unwrap().starts_with("commit:")),
        "self-evident exhaust evidence must not be replayed by default: {checks:?}"
    );
    assert_eq!(checks.len(), 1, "only the agent's anchor is a real check");

    let full = run(&dir, &["verify", "--json", "--full"]);
    assert!(full.status.success());
    let f: serde_json::Value = serde_json::from_slice(&full.stdout).unwrap();
    let fchecks = f["checks"].as_array().unwrap();
    assert!(
        fchecks
            .iter()
            .any(|c| c["ref"].as_str().unwrap().starts_with("commit:")),
        "--full must restore the old shape: {fchecks:?}"
    );
}

#[test]
fn the_verify_event_should_carry_base_drift_and_liveness() {
    let dir = fresh_git();
    std::fs::write(dir.join("a.txt"), "hello\n").unwrap();
    git_commit(&dir, "one");
    assert!(run(&dir, &["init"]).status.success());
    assert!(run(&dir, &["claim", "c", "--by", "agent"]).status.success());
    let id = claim_id(&dir);
    assert!(run(&dir, &["evidence", &id, "file:a.txt::hello"])
        .status
        .success());

    // move the world under the anchor
    std::fs::write(dir.join("a.txt"), "hello\nworld\n").unwrap();
    git_commit(&dir, "two");
    assert!(run(&dir, &["verify"]).status.success());

    let ev = ledger_events(&dir)
        .into_iter()
        .rfind(|e| e["type"] == "verify")
        .expect("verify must append an event");
    assert_eq!(ev["body"]["liveness"].as_str().unwrap(), "content");
    assert_eq!(
        ev["body"]["drift"].as_u64().unwrap(),
        1,
        "one commit touched the cited path"
    );
    assert!(
        ev["body"]["base"].as_str().is_some(),
        "the filing base must be recorded"
    );
}

#[test]
fn brief_json_should_expose_the_liveness_of_every_anchor() {
    let dir = fresh_git();
    std::fs::write(dir.join("a.txt"), "hello\n").unwrap();
    git_commit(&dir, "one");
    assert!(run(&dir, &["init"]).status.success());
    assert!(run(&dir, &["claim", "c", "--by", "agent"]).status.success());
    let id = claim_id(&dir);
    assert!(run(&dir, &["evidence", &id, "file:a.txt"]).status.success());

    let out = run(&dir, &["brief", "--json"]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("\"liveness\""),
        "brief --json must carry liveness: {text}"
    );
    assert!(text.contains("existence"));
}
