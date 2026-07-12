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

/// A bare git repo: no commits, no `ev init`. The baseline tests need to observe
/// what `ev init` itself writes, so they must run it themselves.
fn fresh_git() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ev-exh-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q"]);
    dir
}

fn git_commit(dir: &std::path::Path, msg: &str) {
    git(dir, &["add", "-A"]);
    git(dir, &["-c", "commit.gpgsign=false", "commit", "-qm", msg]);
}

fn head_sha(dir: &std::path::Path) -> String {
    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn ledger_path(dir: &std::path::Path) -> std::path::PathBuf {
    std::fs::read_dir(dir.join(".evolving/ledger"))
        .unwrap()
        .filter_map(|e| e.ok())
        .find(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
        .unwrap()
        .path()
}

fn ledger_events(dir: &std::path::Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(ledger_path(dir))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[test]
fn init_should_record_a_baseline_at_the_current_head() {
    let dir = fresh_git();
    std::fs::write(dir.join("a.txt"), "1\n").unwrap();
    git_commit(&dir, "pre-existing history");
    let head = head_sha(&dir);

    assert!(run(&dir, &["init"]).status.success());

    let baseline = ledger_events(&dir)
        .into_iter()
        .find(|e| e["type"] == "session" && e["body"]["marker"] == "baseline")
        .expect("init must record a baseline marker");
    assert_eq!(baseline["body"]["head"].as_str().unwrap(), head);
}

#[test]
fn init_should_record_root_as_the_baseline_in_an_empty_repo() {
    let dir = fresh_git(); // git init, no commits
    assert!(run(&dir, &["init"]).status.success());

    let baseline = ledger_events(&dir)
        .into_iter()
        .find(|e| e["type"] == "session" && e["body"]["marker"] == "baseline")
        .expect("init must record a baseline even with no HEAD");
    assert_eq!(baseline["body"]["head"].as_str().unwrap(), "ROOT");
}

#[test]
fn init_run_twice_records_exactly_one_baseline_marker() {
    // A re-run of `ev init` is harmless for every sibling write (write_if_absent,
    // ensure_line). It must be equally harmless for the baseline: a second
    // marker at a later HEAD would jump the watermark forward and drop the
    // commits made between the two `init` runs from every future sweep.
    let dir = fresh_git();
    std::fs::write(dir.join("a.txt"), "1\n").unwrap();
    git_commit(&dir, "pre-existing history");

    assert!(run(&dir, &["init"]).status.success());
    assert!(run(&dir, &["init"]).status.success());

    let markers: Vec<_> = ledger_events(&dir)
        .into_iter()
        .filter(|e| e["type"] == "session" && e["body"]["marker"] == "baseline")
        .collect();
    assert_eq!(
        markers.len(),
        1,
        "init run twice must record exactly one baseline marker: {markers:?}"
    );
}

#[test]
fn exhaust_should_refuse_when_the_ledger_has_no_baseline() {
    // A ledger written by 0.2.1 has no baseline marker. Filing ROOT..HEAD would
    // record pre-existing commits as this session's output — refuse, do not guess.
    let dir = fresh_git();
    std::fs::write(dir.join("a.txt"), "1\n").unwrap();
    git_commit(&dir, "pre-existing history");
    assert!(run(&dir, &["init"]).status.success());

    // strip the baseline marker to simulate a 0.2.1 ledger
    let p = ledger_path(&dir);
    let kept: Vec<String> = std::fs::read_to_string(&p)
        .unwrap()
        .lines()
        .filter(|l| {
            let v: serde_json::Value = serde_json::from_str(l).unwrap();
            v["body"]["marker"] != "baseline"
        })
        .map(|s| s.to_string())
        .collect();
    std::fs::write(&p, kept.join("\n") + "\n").unwrap();

    let out = run(&dir, &["exhaust", "--since", "ROOT", "--session", "s1"]);
    assert_eq!(out.status.code(), Some(1), "no baseline must be a refusal");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("ev baseline"),
        "the refusal must name the remedy: {err}"
    );
}

#[test]
fn baseline_should_write_the_marker_so_exhaust_proceeds() {
    let dir = fresh_git();
    std::fs::write(dir.join("a.txt"), "1\n").unwrap();
    git_commit(&dir, "pre-existing history");
    assert!(run(&dir, &["init"]).status.success());
    let head = head_sha(&dir);

    // strip the baseline `init` wrote, so exhaust starts out refusing —
    // the same simulated-0.2.1-ledger state as the refusal test above.
    let p = ledger_path(&dir);
    let kept: Vec<String> = std::fs::read_to_string(&p)
        .unwrap()
        .lines()
        .filter(|l| {
            let v: serde_json::Value = serde_json::from_str(l).unwrap();
            v["body"]["marker"] != "baseline"
        })
        .map(|s| s.to_string())
        .collect();
    let stripped = if kept.is_empty() {
        String::new()
    } else {
        kept.join("\n") + "\n"
    };
    std::fs::write(&p, stripped).unwrap();
    let before = run(&dir, &["exhaust", "--since", "ROOT", "--session", "s1"]);
    assert_eq!(
        before.status.code(),
        Some(1),
        "exhaust must refuse before a baseline exists"
    );

    // `ev baseline` writes the marker...
    let out = run(&dir, &["baseline"]);
    assert!(out.status.success());
    let markers: Vec<_> = ledger_events(&dir)
        .into_iter()
        .filter(|e| e["body"]["marker"] == "baseline")
        .collect();
    assert_eq!(markers.len(), 1);
    assert_eq!(
        markers.last().unwrap()["body"]["head"].as_str().unwrap(),
        head
    );

    // ...and exhaust now proceeds where it previously refused.
    let after = run(&dir, &["exhaust", "--since", "ROOT", "--session", "s1"]);
    assert!(
        after.status.success(),
        "{}",
        String::from_utf8_lossy(&after.stderr)
    );
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
