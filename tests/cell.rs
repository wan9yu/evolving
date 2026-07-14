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
    let dir = std::env::temp_dir().join(format!("ev-cell-{}", ulid::Ulid::new()));
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

fn cells(dir: &std::path::Path) -> Vec<(String, String)> {
    let out = run(dir, &["verify", "--json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    v["checks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["ref"].as_str().unwrap().to_string(),
                c["cell"].as_str().unwrap_or("(none)").to_string(),
            )
        })
        .collect()
}

#[test]
fn an_untouched_anchor_is_still() {
    let dir = fresh_git();
    std::fs::write(dir.join("a.txt"), "hello\n").unwrap();
    git_commit(&dir, "one");
    assert!(run(&dir, &["init"]).status.success());
    assert!(run(&dir, &["claim", "c", "--by", "agent"]).status.success());
    let id = claim_id(&dir);
    assert!(run(&dir, &["evidence", &id, "file:a.txt::hello"])
        .status
        .success());

    assert_eq!(cells(&dir)[0].1, "still");
}

#[test]
fn code_moving_beside_the_anchor_is_neighborhood_moved() {
    // The addition-fix case: the cited line stands, the file around it changed.
    let dir = fresh_git();
    std::fs::write(dir.join("a.txt"), "hello\n").unwrap();
    git_commit(&dir, "one");
    assert!(run(&dir, &["init"]).status.success());
    assert!(run(&dir, &["claim", "c", "--by", "agent"]).status.success());
    let id = claim_id(&dir);
    assert!(run(&dir, &["evidence", &id, "file:a.txt::hello"])
        .status
        .success());

    std::fs::write(dir.join("a.txt"), "hello\nworld\n").unwrap(); // added BESIDE it
    git_commit(&dir, "an addition fix");

    assert_eq!(cells(&dir)[0].1, "neighborhood-moved");
}

#[test]
fn the_cited_line_changing_is_anchor_changed() {
    let dir = fresh_git();
    std::fs::write(dir.join("a.txt"), "hello\n").unwrap();
    git_commit(&dir, "one");
    assert!(run(&dir, &["init"]).status.success());
    assert!(run(&dir, &["claim", "c", "--by", "agent"]).status.success());
    let id = claim_id(&dir);
    assert!(run(&dir, &["evidence", &id, "file:a.txt::hello"])
        .status
        .success());

    std::fs::write(dir.join("a.txt"), "goodbye\n").unwrap();
    git_commit(&dir, "a change fix");

    assert_eq!(cells(&dir)[0].1, "anchor-changed");
}

#[test]
fn ack_should_clear_neighborhood_moved_until_the_world_moves_again() {
    // THE RATCHET TEST. Without ack, a claim that lands in neighborhood-moved stays
    // there forever and no human can clear it — a permanent red carries no information.
    let dir = fresh_git();
    std::fs::write(dir.join("a.txt"), "hello\n").unwrap();
    git_commit(&dir, "one");
    assert!(run(&dir, &["init"]).status.success());
    assert!(run(&dir, &["claim", "c", "--by", "agent"]).status.success());
    let id = claim_id(&dir);
    assert!(run(&dir, &["evidence", &id, "file:a.txt::hello"])
        .status
        .success());

    std::fs::write(dir.join("a.txt"), "hello\nworld\n").unwrap();
    git_commit(&dir, "the world moves");
    assert_eq!(cells(&dir)[0].1, "neighborhood-moved", "the flag must rise");

    assert!(run(&dir, &["ack", &id, "--i-am-the-human"])
        .status
        .success());
    assert_eq!(
        cells(&dir)[0].1,
        "still",
        "the human looked — the flag clears"
    );

    std::fs::write(dir.join("a.txt"), "hello\nworld\nagain\n").unwrap();
    git_commit(&dir, "the world moves again");
    assert_eq!(
        cells(&dir)[0].1,
        "neighborhood-moved",
        "and rises again on new movement"
    );
}
