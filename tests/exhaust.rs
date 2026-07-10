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
    assert_eq!(v["open"][0]["self_evident"].as_bool().unwrap(), true);
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
