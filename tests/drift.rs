use std::process::Command;

fn run(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ev"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

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

#[test]
fn a_file_anchor_reports_drift_after_the_cited_path_changes() {
    let dir = std::env::temp_dir().join(format!("ev-drift-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q"]);
    std::fs::write(dir.join("f.txt"), "the invariant\n").unwrap();
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-qm", "one"]);
    assert!(run(&dir, &["init"]).status.success());

    // file the claim + a file anchor: base records the world-state at filing
    assert!(run(&dir, &["claim", "x", "--source-ref", "s1"])
        .status
        .success());
    let b = run(&dir, &["brief", "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&b.stdout).unwrap();
    let cid = v["open"][0]["id"].as_str().unwrap().to_string();
    assert!(run(&dir, &["evidence", &cid, "file:f.txt"])
        .status
        .success());

    // no drift yet: the cited path is exactly as the anchor saw it
    let out = run(&dir, &["verify", &cid]);
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(!s.contains("drift"), "no drift expected yet: {s}");

    // the world moves under the anchor: the cited path changes in one commit
    std::fs::write(dir.join("f.txt"), "the invariant, rewritten\n").unwrap();
    git(&dir, &["add", "f.txt"]);
    git(&dir, &["commit", "-qm", "two"]);

    let out = run(&dir, &["verify", &cid]);
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        s.contains("drift") && s.contains("1 commit"),
        "drift of 1 commit expected: {s}"
    );

    // the base is a visible fact in the brief's evidence
    let b = run(&dir, &["brief", "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&b.stdout).unwrap();
    assert!(
        v["open"][0]["evidence"][0]["base"].is_string(),
        "evidence should carry its filing base: {v}"
    );
}
