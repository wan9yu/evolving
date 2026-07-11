use std::process::Command;
fn git(dir: &std::path::Path, args: &[&str]) {
    Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .output()
        .unwrap();
}
fn run(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ev"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}
fn repo() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ev-exh-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q"]);
    assert!(run(&dir, &["init"]).status.success());
    dir
}

#[test]
fn a_single_commit_window_files_one_self_evident_claim_labeled_by_the_subject() {
    let dir = repo();
    let start = String::from_utf8(
        Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&dir)
            .output()
            .unwrap()
            .stdout,
    )
    .ok();
    let _ = start; // no commits yet
    std::fs::write(dir.join("a.txt"), "1").unwrap();
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-qm", "tighten the redaction boundary"]);
    // exhaust the window HEAD~1..HEAD (here: the root commit)
    let out = run(
        &dir,
        &["exhaust", "--since", "ROOT", "--session", "sess-42"],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let b = run(&dir, &["brief", "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&b.stdout).unwrap();
    assert_eq!(
        v["open"][0]["label"].as_str().unwrap(),
        "tighten the redaction boundary"
    );
    assert!(v["open"][0]["self_evident"].as_bool().unwrap());
}

#[test]
fn exhausting_the_same_session_twice_is_idempotent() {
    let dir = repo();
    std::fs::write(dir.join("a.txt"), "1").unwrap();
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-qm", "one"]);
    assert!(
        run(&dir, &["exhaust", "--since", "ROOT", "--session", "s1"])
            .status
            .success()
    );
    assert!(
        run(&dir, &["exhaust", "--since", "ROOT", "--session", "s1"])
            .status
            .success()
    );
    let b = run(&dir, &["brief", "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&b.stdout).unwrap();
    assert_eq!(
        v["open"].as_array().unwrap().len(),
        1,
        "same session must not double-file"
    );
}

#[test]
fn an_empty_window_files_nothing() {
    let dir = repo();
    std::fs::write(dir.join("a.txt"), "1").unwrap();
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-qm", "one"]);
    let head = String::from_utf8(
        Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&dir)
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    let out = run(
        &dir,
        &["exhaust", "--since", head.trim(), "--session", "s-empty"],
    );
    assert!(out.status.success());
    let b = run(&dir, &["brief", "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&b.stdout).unwrap();
    assert_eq!(v["open"].as_array().unwrap().len(), 0);
}

#[test]
fn a_killed_sweep_orphan_is_repaired_on_the_next_pass() {
    // a kill between the claim write and the evidence write leaves a bare
    // exhaust claim; the next pass must attach the evidence it never got,
    // not skip the session forever — and not file a second claim.
    let dir = repo();
    std::fs::write(dir.join("a.txt"), "1").unwrap();
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-qm", "one"]);
    // simulate the orphan: the claim exists (session source_ref), no evidence
    assert!(run(
        &dir,
        &["claim", "orphaned by a kill", "--source-ref", "session:s9"]
    )
    .status
    .success());
    let b = run(&dir, &["brief", "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&b.stdout).unwrap();
    assert_eq!(v["open"][0]["evidence"].as_array().unwrap().len(), 0);

    assert!(
        run(&dir, &["exhaust", "--since", "ROOT", "--session", "s9"])
            .status
            .success()
    );
    let b = run(&dir, &["brief", "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&b.stdout).unwrap();
    assert_eq!(
        v["open"].as_array().unwrap().len(),
        1,
        "repair must not file a second claim: {v}"
    );
    assert!(
        !v["open"][0]["evidence"].as_array().unwrap().is_empty(),
        "the orphan should now carry its evidence: {v}"
    );

    // and a repaired session stays idempotent
    assert!(
        run(&dir, &["exhaust", "--since", "ROOT", "--session", "s9"])
            .status
            .success()
    );
    let b = run(&dir, &["brief", "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&b.stdout).unwrap();
    let n = v["open"][0]["evidence"].as_array().unwrap().len();
    assert!(
        run(&dir, &["exhaust", "--since", "ROOT", "--session", "s9"])
            .status
            .success()
    );
    let b = run(&dir, &["brief", "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&b.stdout).unwrap();
    assert_eq!(
        v["open"][0]["evidence"].as_array().unwrap().len(),
        n,
        "a repaired session must not accumulate duplicate evidence: {v}"
    );
}
