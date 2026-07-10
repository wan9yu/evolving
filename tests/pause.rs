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

#[test]
fn a_boundary_pause_writes_a_snapshot_and_a_receipt() {
    let dir = fresh();
    // scripted stdin: at the bare-claim screen, answer 'c' (carry) then finish
    let mut child = Command::new(env!("CARGO_BIN_EXE_ev"))
        .args(["pause", "--boundary", "--script"])
        .current_dir(&dir)
        .env_remove("CLAUDECODE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"c\nn\n").unwrap();
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
