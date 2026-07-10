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
        .env("HOME", dir)
        // hooks and exhaust verbs must not be blocked by the CLAUDECODE guard
        .env_remove("CLAUDECODE")
        .output()
        .unwrap()
}

fn run_with_stdin(dir: &std::path::Path, args: &[&str], stdin: &str) -> std::process::Output {
    use std::io::Write;
    let mut child = Command::new(env!("CARGO_BIN_EXE_ev"))
        .args(args)
        .current_dir(dir)
        .env("HOME", dir)
        .env_remove("CLAUDECODE")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn fresh() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ev-hook-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q"]);
    assert!(run(&dir, &["init"]).status.success());
    dir
}

#[test]
fn hook_install_writes_a_sessionstart_and_is_idempotent() {
    let dir = fresh();
    assert!(run(&dir, &["hook", "install"]).status.success());
    assert!(run(&dir, &["hook", "install"]).status.success());
    let settings = std::fs::read_to_string(dir.join(".claude/settings.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&settings).unwrap();
    let starts = v["hooks"]["SessionStart"].as_array().unwrap();
    // idempotent: exactly one ev entry
    let ev_entries = starts
        .iter()
        .filter(|e| serde_json::to_string(e).unwrap().contains("ev hook"))
        .count();
    assert_eq!(ev_entries, 1, "{settings}");
}

#[test]
fn session_end_marker_exits_zero_even_with_junk_stdin() {
    let dir = fresh();
    // a SessionEnd handler must never fail the session, even on bad input
    let out = run_with_stdin(&dir, &["hook", "session-end"], "not json at all!!!!");
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn sweep_files_one_claim_per_session_window() {
    // Two sessions each containing distinct commits; sweep must file exactly one claim
    // per session and the evidence sets must be disjoint (no commit appears in both).
    let dir = fresh();

    // commit A (session s1)
    std::fs::write(dir.join("a.txt"), "1").unwrap();
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-qm", "commit-A"]);
    let head_a = String::from_utf8(
        Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&dir)
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    let head_a = head_a.trim().to_string();

    // session-end s1 — records head=A
    let out = run_with_stdin(&dir, &["hook", "session-end"], r#"{"session_id":"s1"}"#);
    assert_eq!(out.status.code(), Some(0), "session-end s1 failed");

    // commit B (session s2)
    std::fs::write(dir.join("b.txt"), "2").unwrap();
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-qm", "commit-B"]);
    let head_b = String::from_utf8(
        Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&dir)
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    let head_b = head_b.trim().to_string();

    // session-end s2 — records head=B
    let out = run_with_stdin(&dir, &["hook", "session-end"], r#"{"session_id":"s2"}"#);
    assert_eq!(out.status.code(), Some(0), "session-end s2 failed");

    // session-start triggers the sweep (empty stdin = no session_id needed)
    let out = run_with_stdin(&dir, &["hook", "session-start"], "");
    assert_eq!(out.status.code(), Some(0), "session-start sweep failed");

    // brief --json: expect exactly 2 open claims
    let b = run(&dir, &["brief", "--json"]);
    let v: serde_json::Value =
        serde_json::from_slice(&b.stdout).expect("brief --json must be valid JSON");
    let open = v["open"].as_array().expect("open must be an array");
    assert_eq!(open.len(), 2, "expected 2 claims, got: {v}");

    // collect all evidence refs across both claims
    let all_refs: Vec<String> = open
        .iter()
        .flat_map(|c| {
            c["evidence"]
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .map(|e| e["ref"].as_str().unwrap_or("").to_string())
                .collect::<Vec<_>>()
        })
        .collect();

    // commit-A's sha must appear, commit-B's sha must appear, and no sha appears in both claims
    assert!(
        all_refs.iter().any(|r| r.contains(&head_a)),
        "commit A sha {head_a} not found in evidence"
    );
    assert!(
        all_refs.iter().any(|r| r.contains(&head_b)),
        "commit B sha {head_b} not found in evidence"
    );

    // disjointness: collect per-claim evidence sets
    let claim0_refs: std::collections::HashSet<String> = open[0]["evidence"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .map(|e| e["ref"].as_str().unwrap_or("").to_string())
        .collect();
    let claim1_refs: std::collections::HashSet<String> = open[1]["evidence"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .map(|e| e["ref"].as_str().unwrap_or("").to_string())
        .collect();
    let overlap: std::collections::HashSet<_> = claim0_refs.intersection(&claim1_refs).collect();
    assert!(
        overlap.is_empty(),
        "evidence sets must be disjoint but share: {overlap:?}"
    );
}
