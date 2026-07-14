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
fn claim_id(dir: &std::path::Path) -> String {
    let ledger = std::fs::read_dir(dir.join(".evolving/ledger"))
        .unwrap()
        .filter_map(|e| e.ok())
        .find(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
        .unwrap()
        .path();
    for line in std::fs::read_to_string(&ledger).unwrap().lines() {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        if v["type"] == "claim" {
            return v["id"].as_str().unwrap().to_string();
        }
    }
    panic!("no claim event in the ledger");
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

#[test]
fn evidence_should_refuse_a_line_number_ref_and_teach_the_content_anchor() {
    let dir = fresh_git();
    std::fs::write(dir.join("foo.rs"), "fn bar() {}\n").unwrap();
    assert!(run(&dir, &["init"]).status.success());
    let out = run(&dir, &["claim", "x", "--by", "agent"]);
    assert!(out.status.success());
    let id = claim_id(&dir);

    let out = run(&dir, &["evidence", &id, "file:foo.rs:1"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "a line-number ref must be refused"
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("anchors by content"),
        "the refusal must teach the grammar: {err}"
    );
    assert!(
        err.contains("file:foo.rs::"),
        "the refusal must show the correct form: {err}"
    );
}

/// A refused ref must cost the ledger nothing. `ev claim --evidence` writes the claim
/// and the evidence in two separate atomic batches, so a guard that fired after the
/// first would leave a bare claim behind on every attempt — and the guard fires on the
/// single likeliest typo this release exists to catch. Append-only means an orphan is
/// forever.
#[test]
fn a_refused_inline_evidence_ref_leaves_no_claim_in_the_ledger() {
    let dir = fresh_git();
    std::fs::write(dir.join("foo.rs"), "fn bar() {}\n").unwrap();

    let out = run(
        &dir,
        &[
            "claim",
            "typo",
            "--by",
            "agent",
            "--evidence",
            "file:foo.rs:42",
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "a line-number ref must be refused"
    );

    let ledger = std::fs::read_dir(dir.join(".evolving/ledger"))
        .unwrap()
        .filter_map(|e| e.ok())
        .find(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
        .unwrap()
        .path();
    let claims: Vec<serde_json::Value> = std::fs::read_to_string(&ledger)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .filter(|v| v["type"] == "claim" || v["type"] == "evidence")
        .collect();
    assert!(
        claims.is_empty(),
        "the refusal must write nothing at all: {claims:?}"
    );
}

#[test]
fn evidence_should_hint_when_an_anchor_can_only_fail_on_deletion() {
    let dir = fresh_git();
    std::fs::write(dir.join("foo.rs"), "fn bar() {}\n").unwrap();
    assert!(run(&dir, &["init"]).status.success());
    assert!(run(&dir, &["claim", "x", "--by", "agent"]).status.success());
    let id = claim_id(&dir);

    let out = run(&dir, &["evidence", &id, "file:foo.rs"]);
    assert!(
        out.status.success(),
        "an existence anchor is legal — hint, never refuse"
    );
    let sout = String::from_utf8_lossy(&out.stdout);
    assert!(sout.contains("resolves"));
    assert!(
        sout.contains("existence anchor"),
        "expected the advisory hint: {sout}"
    );
}

#[test]
fn evidence_should_not_hint_for_a_content_anchor() {
    let dir = fresh_git();
    std::fs::write(dir.join("foo.rs"), "fn bar() {}\n").unwrap();
    assert!(run(&dir, &["init"]).status.success());
    assert!(run(&dir, &["claim", "x", "--by", "agent"]).status.success());
    let id = claim_id(&dir);

    let out = run(&dir, &["evidence", &id, "file:foo.rs::fn bar()"]);
    assert!(out.status.success());
    let sout = String::from_utf8_lossy(&out.stdout);
    assert!(sout.contains("resolves"));
    assert!(
        !sout.contains("existence anchor"),
        "a content anchor needs no hint: {sout}"
    );
}

#[test]
fn verify_should_still_read_a_legacy_line_number_ref_without_erroring() {
    // A 0.2.1 ledger holds `file:<path>:150` evidence events. The attach guard
    // must never leak into the read path: `ev verify` reads them as gone (the
    // path `src/x.py:150` does not exist), not as an error.
    let dir = fresh_git();
    assert!(run(&dir, &["init"]).status.success());
    assert!(run(&dir, &["claim", "legacy", "--by", "agent"])
        .status
        .success());
    let id = claim_id(&dir);

    // hand-append the legacy event the way 0.2.1 wrote it
    let ledger = std::fs::read_dir(dir.join(".evolving/ledger"))
        .unwrap()
        .filter_map(|e| e.ok())
        .find(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
        .unwrap()
        .path();
    let mut text = std::fs::read_to_string(&ledger).unwrap();
    let last: serde_json::Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
    // `ts` rides the ledger's own tail: `state::fold` reads events in
    // (ts, writer, seq) order and attaches evidence only to a claim it has
    // already seen, so a hand-written past timestamp would sort ahead of the
    // claim and be dropped. The subject under test is the legacy *ref shape*.
    let legacy = serde_json::json!({
        "v": last["v"], "id": "evd_01LEGACY0000000000000000",
        "ts": last["ts"], "writer": last["writer"],
        "seq": last["seq"].as_u64().unwrap() + 1,
        "actor": { "kind": "agent", "id": "legacy" },
        "type": "evidence",
        "body": { "claim": id, "ref": "file:src/x.py:150", "status": "unreachable", "self_evident": false }
    });
    text.push_str(&serde_json::to_string(&legacy).unwrap());
    text.push('\n');
    std::fs::write(&ledger, text).unwrap();

    let out = run(&dir, &["verify", "--json"]);
    assert!(
        out.status.success(),
        "verify must not error on a legacy ref: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let sout = String::from_utf8_lossy(&out.stdout);
    assert!(
        sout.contains("file:src/x.py:150"),
        "the legacy ref must still be reported: {sout}"
    );
    assert!(sout.contains("gone"));
}

fn event_id(dir: &std::path::Path, etype: &str) -> String {
    let p = std::fs::read_dir(dir.join(".evolving/ledger"))
        .unwrap()
        .filter_map(|e| e.ok())
        .find(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
        .unwrap()
        .path();
    for line in std::fs::read_to_string(p).unwrap().lines() {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        if v["type"] == etype {
            return v["id"].as_str().unwrap().to_string();
        }
    }
    panic!("no {etype} event in the ledger");
}

#[test]
fn evidence_should_refuse_a_think_event_id() {
    let dir = fresh_git();
    std::fs::write(dir.join("f.txt"), "hello\n").unwrap();
    assert!(run(&dir, &["init"]).status.success());
    assert!(run(&dir, &["think", "a thought, not a claim"])
        .status
        .success());
    let thk = event_id(&dir, "thought");

    let out = run(&dir, &["evidence", &thk, "file:f.txt::hello"]);
    assert_eq!(out.status.code(), Some(1), "a think id is not a claim id");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("not a claim"),
        "the refusal must name the type error: {err}"
    );
}

#[test]
fn evidence_should_still_accept_a_real_claim_id() {
    let dir = fresh_git();
    std::fs::write(dir.join("f.txt"), "hello\n").unwrap();
    assert!(run(&dir, &["init"]).status.success());
    assert!(run(&dir, &["think", "a thought"]).status.success());
    assert!(run(&dir, &["claim", "a claim", "--by", "agent"])
        .status
        .success());
    let cid = event_id(&dir, "claim");

    let out = run(&dir, &["evidence", &cid, "file:f.txt::hello"]);
    assert!(
        out.status.success(),
        "a claim id must still work: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn evidence_should_refuse_a_content_anchor_whose_text_is_absent() {
    let dir = fresh_git();
    std::fs::write(dir.join("f.txt"), "hello\n").unwrap();
    assert!(run(&dir, &["init"]).status.success());
    assert!(run(&dir, &["claim", "x", "--by", "agent"]).status.success());
    let cid = event_id(&dir, "claim");

    let out = run(&dir, &["evidence", &cid, "file:f.txt::NEVER_EXISTED"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "an anchor on absent text carries no signal"
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("not in f.txt"),
        "the refusal must name the file: {err}"
    );
    assert!(
        err.contains("exists now"),
        "the refusal must teach the rule: {err}"
    );
}

#[test]
fn evidence_should_refuse_an_empty_pass_line() {
    let dir = fresh_git();
    std::fs::write(dir.join("f.txt"), "hello\n").unwrap();
    assert!(run(&dir, &["init"]).status.success());
    assert!(run(&dir, &["claim", "x", "--by", "agent"]).status.success());
    let cid = event_id(&dir, "claim");

    let out = run(&dir, &["evidence", &cid, "file:f.txt::"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "an empty pass-line matches every line"
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("empty"), "the refusal must say why: {err}");
}

#[test]
fn evidence_should_accept_a_content_anchor_whose_text_is_present() {
    let dir = fresh_git();
    std::fs::write(dir.join("f.txt"), "hello world\n").unwrap();
    assert!(run(&dir, &["init"]).status.success());
    assert!(run(&dir, &["claim", "x", "--by", "agent"]).status.success());
    let cid = event_id(&dir, "claim");

    let out = run(&dir, &["evidence", &cid, "file:f.txt::hello"]);
    assert!(
        out.status.success(),
        "present text must be accepted: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("resolves"));
}

#[test]
fn verify_should_still_read_an_anchor_that_attach_would_now_refuse() {
    // A 0.2.2 ledger can hold `file:p::TEXT_NOT_THERE` and `file:p::` — 0.2.3 refuses
    // both at attach, but the guard must never leak into the READ path.
    let dir = fresh_git();
    std::fs::write(dir.join("f.txt"), "hello\n").unwrap();
    assert!(run(&dir, &["init"]).status.success());
    assert!(run(&dir, &["claim", "legacy", "--by", "agent"])
        .status
        .success());
    let cid = event_id(&dir, "claim");

    let ledger = std::fs::read_dir(dir.join(".evolving/ledger"))
        .unwrap()
        .filter_map(|e| e.ok())
        .find(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
        .unwrap()
        .path();
    let mut text = std::fs::read_to_string(&ledger).unwrap();
    let last: serde_json::Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
    let legacy = serde_json::json!({
        "v": last["v"], "id": "evd_01LEGACY0000000000000000",
        "ts": last["ts"], "writer": last["writer"],
        "seq": last["seq"].as_u64().unwrap() + 1,
        "actor": { "kind": "agent", "id": "legacy" },
        "type": "evidence",
        "body": { "claim": cid, "ref": "file:f.txt::TEXT_NOT_THERE",
                  "status": "failed", "self_evident": false }
    });
    text.push_str(&serde_json::to_string(&legacy).unwrap());
    text.push('\n');
    std::fs::write(&ledger, text).unwrap();

    let out = run(&dir, &["verify", "--json"]);
    assert!(
        out.status.success(),
        "verify must not error on a legacy anchor: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("file:f.txt::TEXT_NOT_THERE"));
}
