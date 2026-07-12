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
    // must never leak into the read path: `ev verify` reads them as unreachable.
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
    assert!(sout.contains("unreachable"));
}
