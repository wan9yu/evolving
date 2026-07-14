use std::process::Command;

fn run(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ev"))
        .args(args)
        .current_dir(dir)
        .env_remove("CLAUDECODE") // the human-provenance guard refuses closure verbs under it
        .output()
        .unwrap()
}

fn fresh_git() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ev-ack-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    Command::new("git")
        .args(["init", "-q"])
        .current_dir(&dir)
        .output()
        .unwrap();
    dir
}

fn git_commit(dir: &std::path::Path, msg: &str) {
    let git = |a: &[&str]| {
        Command::new("git")
            .args(a)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .output()
            .unwrap()
    };
    git(&["add", "-A"]);
    git(&["-c", "commit.gpgsign=false", "commit", "-m", msg]);
}

fn ledger_events(dir: &std::path::Path) -> Vec<serde_json::Value> {
    let p = std::fs::read_dir(dir.join(".evolving/ledger"))
        .unwrap()
        .filter_map(|e| e.ok())
        .find(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
        .unwrap()
        .path();
    std::fs::read_to_string(p)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn claim_id(dir: &std::path::Path) -> String {
    ledger_events(dir)
        .into_iter()
        .find(|e| e["type"] == "claim")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn head_sha(dir: &std::path::Path) -> String {
    let o = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .output()
        .unwrap();
    String::from_utf8_lossy(&o.stdout).trim().to_string()
}

#[test]
fn ack_should_record_the_head_the_human_looked_at() {
    let dir = fresh_git();
    std::fs::write(dir.join("a.txt"), "hello\n").unwrap();
    git_commit(&dir, "one");
    assert!(run(&dir, &["init"]).status.success());
    assert!(run(&dir, &["claim", "c", "--by", "agent"]).status.success());
    let id = claim_id(&dir);
    assert!(run(&dir, &["evidence", &id, "file:a.txt::hello"])
        .status
        .success());

    std::fs::write(dir.join("b.txt"), "x\n").unwrap();
    git_commit(&dir, "the world moves");
    let head = head_sha(&dir);

    let out = run(&dir, &["ack", &id, "--i-am-the-human"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let ev = ledger_events(&dir)
        .into_iter()
        .rfind(|e| e["type"] == "ack")
        .expect("ack must append an event");
    assert_eq!(ev["body"]["claim"].as_str().unwrap(), id);
    assert_eq!(
        ev["body"]["head"].as_str().unwrap(),
        head,
        "the ack records the HEAD the human looked at"
    );
}

#[test]
fn ack_should_refuse_a_non_claim_id() {
    let dir = fresh_git();
    std::fs::write(dir.join("a.txt"), "hello\n").unwrap();
    git_commit(&dir, "one");
    assert!(run(&dir, &["init"]).status.success());
    assert!(run(&dir, &["think", "a thought"]).status.success());
    let thk = ledger_events(&dir)
        .into_iter()
        .find(|e| e["type"] == "thought")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let out = run(&dir, &["ack", &thk, "--i-am-the-human"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "ack attaches to a claim, like every disposition"
    );
}
