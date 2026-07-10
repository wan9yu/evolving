use std::io::Write;
use std::process::{Command, Stdio};

fn ev(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ev"))
        .args(args)
        .current_dir(dir)
        .env_remove("CLAUDECODE")
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

fn writer_id(dir: &std::path::Path) -> String {
    let raw = std::fs::read_to_string(dir.join(".evolving/local/writer.toml")).unwrap();
    raw.split('"').nth(1).unwrap().to_string()
}

// T1 — line does not double-count after a boundary
#[test]
fn t1_line_does_not_double_count_after_boundary() {
    let dir = std::env::temp_dir().join(format!("ev-rf-t1-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();

    // Init git repo then ev
    git(&dir, &["init", "-q"]);
    assert!(ev(&dir, &["init"]).status.success());

    // Make a commit so we have a real SHA
    std::fs::write(dir.join("work.txt"), "done").unwrap();
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-qm", "implement the feature"]);

    // Get HEAD sha
    let sha_out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&dir)
        .output()
        .unwrap();
    let sha = String::from_utf8_lossy(&sha_out.stdout).trim().to_string();

    // File a claim with commit evidence
    let evidence_ref = format!("commit:{sha}");
    assert!(
        ev(
            &dir,
            &["claim", "feature done", "--evidence", &evidence_ref]
        )
        .status
        .success(),
        "claim with evidence must succeed"
    );

    // Find the claim id
    let brief_out = ev(&dir, &["brief", "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&brief_out.stdout).unwrap();
    let cid = v["open"][0]["id"].as_str().unwrap().to_string();

    // Close the claim
    assert!(
        ev(&dir, &["close", &cid, "--i-am-the-human"])
            .status
            .success(),
        "close must succeed"
    );

    // Write a boundary via pause --boundary --script with "n" for legibility
    let mut child = Command::new(env!("CARGO_BIN_EXE_ev"))
        .args(["pause", "--boundary", "--script", "--i-am-the-human"])
        .current_dir(&dir)
        .env_remove("CLAUDECODE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"n\n").unwrap();
    let pause_out = child.wait_with_output().unwrap();
    assert!(
        pause_out.status.success(),
        "pause boundary failed: {}",
        String::from_utf8_lossy(&pause_out.stderr)
    );

    // Check ev line --json: closed_with_evidence must be 1, not 2
    let line_out = ev(&dir, &["line", "--json"]);
    assert!(line_out.status.success());
    let lv: serde_json::Value = serde_json::from_slice(&line_out.stdout).unwrap();
    let count = lv["indicators"][0]["closed_with_evidence"]
        .as_u64()
        .expect("closed_with_evidence must be a number");
    assert_eq!(
        count, 1,
        "closed_with_evidence must be 1 after one boundary, got {count} (double-count bug)"
    );
}

// T2 — retire frees a ceiling slot
#[test]
fn t2_retire_frees_a_ceiling_slot() {
    let dir = std::env::temp_dir().join(format!("ev-rf-t2-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    assert!(ev(&dir, &["init"]).status.success());

    // Declare 4 indicators (filling the ceiling)
    for name in &["i1", "i2", "i3", "i4"] {
        let out = ev(&dir, &["indicator", "declare", name, "--i-am-the-human"]);
        assert!(
            out.status.success(),
            "declaring {name} must succeed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    // Declaring a 5th must fail
    let out5 = ev(&dir, &["indicator", "declare", "i5", "--i-am-the-human"]);
    assert_eq!(
        out5.status.code(),
        Some(1),
        "5th indicator must be refused at ceiling"
    );

    // Find the id of the first indicator from the ledger
    let wid = writer_id(&dir);
    let ledger_path = dir.join(".evolving/ledger").join(format!("{wid}.jsonl"));
    let ledger_text = std::fs::read_to_string(&ledger_path).unwrap();
    let first_indicator_id = ledger_text
        .lines()
        .find_map(|line| {
            let v: serde_json::Value = serde_json::from_str(line).ok()?;
            if v.get("type").and_then(|t| t.as_str()) == Some("indicator") {
                v.get("id")
                    .and_then(|id| id.as_str())
                    .map(|s| s.to_string())
            } else {
                None
            }
        })
        .expect("must find at least one indicator event in ledger");

    // Retire that indicator
    let retire_out = ev(
        &dir,
        &[
            "indicator",
            "retire",
            &first_indicator_id,
            "--i-am-the-human",
        ],
    );
    assert!(
        retire_out.status.success(),
        "retire must succeed: {}",
        String::from_utf8_lossy(&retire_out.stderr)
    );

    // Now declaring the 5th must succeed (ceiling slot freed)
    let out5_retry = ev(&dir, &["indicator", "declare", "i5", "--i-am-the-human"]);
    assert!(
        out5_retry.status.success(),
        "5th indicator must succeed after retire, got: {}",
        String::from_utf8_lossy(&out5_retry.stderr)
    );
}
