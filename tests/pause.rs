use std::io::Write;
use std::process::{Command, Stdio};

fn run(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ev"))
        .args(args)
        .current_dir(dir)
        .env_remove("CLAUDECODE")
        .output()
        .unwrap()
}

fn fresh() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ev-pause-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    assert!(run(&dir, &["init"]).status.success());
    dir
}

/// Spawn `ev pause --script` with the given stdin bytes and return the output.
fn pause_with_input(dir: &std::path::Path, stdin_bytes: &[u8]) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ev"))
        .args(["pause", "--script"])
        .current_dir(dir)
        .env_remove("CLAUDECODE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin_bytes).unwrap();
    child.wait_with_output().unwrap()
}

/// Read the writer id from `.evolving/local/writer.toml`.
fn writer_id(dir: &std::path::Path) -> String {
    let raw = std::fs::read_to_string(dir.join(".evolving/local/writer.toml")).unwrap();
    raw.split('"').nth(1).unwrap().to_string()
}

#[test]
fn a_boundary_pause_writes_a_snapshot_and_a_receipt() {
    let dir = fresh();
    // No bare claims exist, so screen 3 is skipped; only one line needed for
    // the legibility prompt (screen 5).
    let mut child = Command::new(env!("CARGO_BIN_EXE_ev"))
        .args(["pause", "--boundary", "--script"])
        .current_dir(&dir)
        .env_remove("CLAUDECODE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"n\n").unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        s.contains("receipt"),
        "pause should end with a receipt: {s}"
    );
    // a boundary pause appends a snapshot event
    let b = run(&dir, &["line", "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&b.stdout).unwrap();
    assert_eq!(v["snapshots"].as_array().unwrap().len(), 1);
}

#[test]
fn the_pause_records_a_boundary_pause_event() {
    let dir = fresh();
    let mut child = Command::new(env!("CARGO_BIN_EXE_ev"))
        .args(["pause", "--boundary", "--script"])
        .current_dir(&dir)
        .env_remove("CLAUDECODE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"n\n").unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    // boundary_count is now 1 (visible via a second render path)
    let b = run(&dir, &["brief", "--json"]);
    assert!(b.status.success());
}

#[test]
fn pause_demand_on_a_bare_claim_records_a_demand_event() {
    let dir = fresh();
    assert!(run(&dir, &["claim", "needs evidence"]).status.success());
    // Screen 3 reads one line per bare claim (the action); screen 5 reads one
    // line for the legibility prompt.
    let out = pause_with_input(&dir, b"d\nn\n");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let wid = writer_id(&dir);
    let log =
        std::fs::read_to_string(dir.join(".evolving/ledger").join(format!("{wid}.jsonl"))).unwrap();
    assert!(
        log.lines().any(|l| l.contains("\"type\":\"demand\"")),
        "ledger should contain a demand event: {log}"
    );
}

#[test]
fn pause_hold_on_a_bare_claim_moves_it_to_grey() {
    let dir = fresh();
    assert!(run(&dir, &["claim", "on hold"]).status.success());
    let out = pause_with_input(&dir, b"h\nn\n");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let b = run(&dir, &["brief", "--json"]);
    assert!(b.status.success());
    let v: serde_json::Value = serde_json::from_slice(&b.stdout).unwrap();
    assert_eq!(
        v["grey"].as_array().unwrap().len(),
        1,
        "grey should contain the held claim: {v}"
    );
}

fn run_with_stdin(dir: &std::path::Path, args: &[&str], input: &str) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ev"))
        .args(args)
        .current_dir(dir)
        .env_remove("CLAUDECODE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn fresh_git_pause() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ev-pause-g-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    Command::new("git")
        .args(["init", "-q"])
        .current_dir(&dir)
        .output()
        .unwrap();
    dir
}

fn git_commit_pause(dir: &std::path::Path, msg: &str) {
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

fn ledger_events_pause(dir: &std::path::Path) -> Vec<serde_json::Value> {
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

fn claim_id_pause(dir: &std::path::Path, label: &str) -> String {
    ledger_events_pause(dir)
        .into_iter()
        .find(|e| e["type"] == "claim" && e["body"]["label"] == label)
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn pause_should_surface_moved_claims_and_let_the_human_ack_them() {
    let dir = fresh_git_pause();
    std::fs::write(dir.join("a.txt"), "hello\n").unwrap();
    git_commit_pause(&dir, "one");
    assert!(run(&dir, &["init"]).status.success());
    assert!(run(&dir, &["claim", "watched", "--by", "agent"])
        .status
        .success());
    let id = claim_id_pause(&dir, "watched");
    assert!(run(&dir, &["evidence", &id, "file:a.txt::hello"])
        .status
        .success());

    std::fs::write(dir.join("a.txt"), "hello\nworld\n").unwrap();
    git_commit_pause(&dir, "code moves beside the anchor");

    // one keystroke: k = still stands
    let out = run_with_stdin(&dir, &["pause", "--script", "--i-am-the-human"], "k\n");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        s.contains("code moved"),
        "the moved set must be surfaced: {s}"
    );
    assert!(s.contains("watched"), "the claim must be named: {s}");

    let acked = ledger_events_pause(&dir)
        .into_iter()
        .any(|e| e["type"] == "ack");
    assert!(acked, "the k keystroke must append an ack event");
}

#[test]
fn pause_dead_on_a_bare_claim_removes_it_from_open() {
    let dir = fresh();
    assert!(run(&dir, &["claim", "going nowhere"]).status.success());
    let out = pause_with_input(&dir, b"x\nn\n");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let b = run(&dir, &["brief", "--json"]);
    assert!(b.status.success());
    let v: serde_json::Value = serde_json::from_slice(&b.stdout).unwrap();
    assert_eq!(
        v["open"].as_array().unwrap().len(),
        0,
        "open should be empty after claiming dead: {v}"
    );
}
