use std::process::Command;
fn run(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ev"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}
fn fresh_git() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ev-evd-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    let git = |a: &[&str]| {
        Command::new("git")
            .args(a)
            .current_dir(&dir)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .output()
            .unwrap()
    };
    git(&["init", "-q"]);
    std::fs::write(dir.join("f.txt"), "x").unwrap();
    git(&["add", "."]);
    git(&["commit", "-qm", "first"]);
    assert!(run(&dir, &["init"]).status.success());
    dir
}
#[test]
fn evidence_pointing_at_a_real_commit_verifies() {
    let dir = fresh_git();
    let head = String::from_utf8(
        Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&dir)
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    let head = head.trim();
    assert!(run(&dir, &["claim", "did it", "--source-ref", "s1"])
        .status
        .success());
    let out = run(&dir, &["brief", "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let cid = v["open"][0]["id"].as_str().unwrap().to_string();
    let out = run(&dir, &["evidence", &cid, &format!("commit:{head}")]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = run(&dir, &["brief", "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["open"][0]["state"].as_str().unwrap(), "anchored");
}
