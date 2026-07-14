use std::process::Command;

fn run(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ev"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

fn git(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .output()
        .unwrap()
}

fn fresh_git() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ev-vfy-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q"]);
    std::fs::write(dir.join("f.txt"), "x").unwrap();
    git_commit(&dir, "first");
    dir
}

fn git_commit(dir: &std::path::Path, msg: &str) {
    git(dir, &["add", "-A"]);
    git(dir, &["-c", "commit.gpgsign=false", "commit", "-qm", msg]);
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

/// The id of the most recently filed claim — the one the test just made.
fn claim_id(dir: &std::path::Path) -> String {
    let mut last = None;
    for v in ledger_events(dir) {
        if v["type"] == "claim" {
            last = Some(v["id"].as_str().unwrap().to_string());
        }
    }
    last.expect("no claim event in the ledger")
}

#[test]
fn verify_should_skip_self_evident_evidence_by_default_and_include_it_under_full() {
    let dir = fresh_git();
    std::fs::write(dir.join("a.txt"), "1\n").unwrap();
    git_commit(&dir, "one");
    assert!(run(&dir, &["init"]).status.success());
    std::fs::write(dir.join("b.txt"), "2\n").unwrap();
    git_commit(&dir, "two");

    // exhaust files self-evident commit: evidence for the window
    assert!(
        run(&dir, &["exhaust", "--since", "HEAD~1", "--session", "s1"])
            .status
            .success()
    );
    // an agent files a real, non-self-evident anchor
    assert!(run(&dir, &["claim", "real", "--by", "agent"])
        .status
        .success());
    let id = claim_id(&dir);
    assert!(run(&dir, &["evidence", &id, "file:a.txt::1"])
        .status
        .success());

    let default = run(&dir, &["verify", "--json"]);
    assert!(default.status.success());
    let d: serde_json::Value =
        serde_json::from_slice(&default.stdout).expect("verify --json must be valid json");
    let checks = d["checks"].as_array().unwrap();
    assert!(
        checks
            .iter()
            .all(|c| !c["ref"].as_str().unwrap().starts_with("commit:")),
        "self-evident exhaust evidence must not be replayed by default: {checks:?}"
    );
    assert_eq!(checks.len(), 1, "only the agent's anchor is a real check");

    let full = run(&dir, &["verify", "--json", "--full"]);
    assert!(full.status.success());
    let f: serde_json::Value = serde_json::from_slice(&full.stdout).unwrap();
    let fchecks = f["checks"].as_array().unwrap();
    assert!(
        fchecks
            .iter()
            .any(|c| c["ref"].as_str().unwrap().starts_with("commit:")),
        "--full must restore the old shape: {fchecks:?}"
    );
}

#[test]
fn the_verify_event_should_carry_base_drift_and_liveness() {
    let dir = fresh_git();
    std::fs::write(dir.join("a.txt"), "hello\n").unwrap();
    git_commit(&dir, "one");
    assert!(run(&dir, &["init"]).status.success());
    assert!(run(&dir, &["claim", "c", "--by", "agent"]).status.success());
    let id = claim_id(&dir);
    assert!(run(&dir, &["evidence", &id, "file:a.txt::hello"])
        .status
        .success());

    // move the world under the anchor
    std::fs::write(dir.join("a.txt"), "hello\nworld\n").unwrap();
    git_commit(&dir, "two");
    assert!(run(&dir, &["verify"]).status.success());

    let ev = ledger_events(&dir)
        .into_iter()
        .rfind(|e| e["type"] == "verify")
        .expect("verify must append an event");
    assert_eq!(ev["body"]["liveness"].as_str().unwrap(), "content");
    assert_eq!(
        ev["body"]["drift"].as_u64().unwrap(),
        1,
        "one commit touched the cited path"
    );
    assert!(
        ev["body"]["base"].as_str().is_some(),
        "the filing base must be recorded"
    );
}

#[test]
fn brief_json_should_expose_the_liveness_of_every_anchor() {
    let dir = fresh_git();
    std::fs::write(dir.join("a.txt"), "hello\n").unwrap();
    git_commit(&dir, "one");
    assert!(run(&dir, &["init"]).status.success());
    assert!(run(&dir, &["claim", "c", "--by", "agent"]).status.success());
    let id = claim_id(&dir);
    assert!(run(&dir, &["evidence", &id, "file:a.txt"]).status.success());

    let out = run(&dir, &["brief", "--json"]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("\"liveness\""),
        "brief --json must carry liveness: {text}"
    );
    assert!(text.contains("existence"));
}

#[test]
fn a_changed_line_should_read_changed_not_failed() {
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
    git_commit(&dir, "the world moves");

    let out = run(&dir, &["verify", "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let c = &v["checks"][0];
    assert_eq!(
        c["status"], "changed",
        "the cited text is gone, the file is not: {c}"
    );
}

#[test]
fn a_deleted_file_should_read_gone_not_unreachable() {
    let dir = fresh_git();
    std::fs::write(dir.join("a.txt"), "hello\n").unwrap();
    git_commit(&dir, "one");
    assert!(run(&dir, &["init"]).status.success());
    assert!(run(&dir, &["claim", "c", "--by", "agent"]).status.success());
    let id = claim_id(&dir);
    assert!(run(&dir, &["evidence", &id, "file:a.txt::hello"])
        .status
        .success());

    std::fs::remove_file(dir.join("a.txt")).unwrap();
    git_commit(&dir, "the file is deleted");

    let out = run(&dir, &["verify", "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let c = &v["checks"][0];
    assert_eq!(
        c["status"], "gone",
        "the container is gone, not merely unreadable: {c}"
    );
}

/// Append an evidence event by hand, as an older ev would have written it.
fn append_legacy_evidence(dir: &std::path::Path, claim: &str, eref: &str, status: &str) {
    let p = ledger_path(dir);
    let mut text = std::fs::read_to_string(&p).unwrap();
    let last: serde_json::Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
    let legacy = serde_json::json!({
        "v": last["v"], "id": "evd_01LEGACY0000000000000000",
        "ts": last["ts"], "writer": last["writer"],
        "seq": last["seq"].as_u64().unwrap() + 1,
        "actor": { "kind": "agent", "id": "legacy" },
        "type": "evidence",
        "body": { "claim": claim, "ref": eref, "status": status, "self_evident": false }
    });
    text.push_str(&serde_json::to_string(&legacy).unwrap());
    text.push('\n');
    std::fs::write(&p, text).unwrap();
}

/// The written event is frozen forever — but a READ is a measurement, not an echo. The
/// read path re-reads the anchor, so a legacy `failed` on a pointer ev can still follow is
/// superseded by what ev finds THERE AND THEN. That is not a reinterpretation of the event:
/// ev opened the file. The event itself is never rewritten, and the ledger still holds it.
#[test]
fn a_legacy_failed_status_is_superseded_by_a_live_reading_never_rewritten() {
    let dir = fresh_git();
    std::fs::write(dir.join("a.txt"), "hello\n").unwrap();
    git_commit(&dir, "one");
    assert!(run(&dir, &["init"]).status.success());
    assert!(run(&dir, &["claim", "c", "--by", "agent"]).status.success());
    let id = claim_id(&dir);
    append_legacy_evidence(&dir, &id, "file:a.txt::hello", "failed");

    let out = run(&dir, &["brief", "--json"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        v["open"][0]["evidence"][0]["status"].as_str(),
        Some("resolves"),
        "the anchor was re-read: the cited text is in the file, and ev says what it found: {v}"
    );

    // APPEND-ONLY. The read superseded nothing in the ledger: the event still says `failed`.
    let raw = std::fs::read_to_string(ledger_path(&dir)).unwrap();
    assert!(
        raw.contains("\"status\":\"failed\""),
        "a value written by an older ev must never be rewritten"
    );
}

/// Where ev CANNOT re-read the pointer, it does not guess: a ref no current grammar accepts
/// keeps the status the ledger recorded, and its cell is `legacy`.
#[test]
fn a_legacy_failed_status_on_an_unreadable_pointer_stays_failed() {
    let dir = fresh_git();
    assert!(run(&dir, &["init"]).status.success());
    assert!(run(&dir, &["claim", "c", "--by", "agent"]).status.success());
    let id = claim_id(&dir);
    // no scheme — no current grammar accepts it, so there is nothing for ev to re-read
    append_legacy_evidence(&dir, &id, "some/old/pointer", "failed");

    let out = run(&dir, &["brief", "--json"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let e = &v["open"][0]["evidence"][0];
    assert_eq!(
        e["status"].as_str(),
        Some("failed"),
        "an unreadable pointer keeps the recorded status — ev does not guess: {v}"
    );
    assert_eq!(
        e["cell"].as_str(),
        Some("legacy"),
        "and its cell says so out loud: {v}"
    );
}

/// `ev verify` must not silently drop a pointer it cannot parse. Dropping it is the
/// no-false-green failure in the one verb whose whole job is to report what it read:
/// the human sees clean output and never learns the pointer is unreadable.
#[test]
fn verify_should_print_a_line_for_an_unparseable_ref_rather_than_dropping_it() {
    let dir = fresh_git();
    assert!(run(&dir, &["init"]).status.success());
    assert!(run(&dir, &["claim", "old", "--by", "agent"])
        .status
        .success());
    let id = claim_id(&dir);

    // an older ledger's untyped pointer: no current ref grammar accepts it
    let p = ledger_path(&dir);
    let mut text = std::fs::read_to_string(&p).unwrap();
    let last: serde_json::Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
    let odd = serde_json::json!({
        "v": last["v"], "id": "evd_01UNPARSEABLE00000000000",
        "ts": last["ts"], "writer": last["writer"],
        "seq": last["seq"].as_u64().unwrap() + 1,
        "actor": { "kind": "agent", "id": "legacy" },
        "type": "evidence",
        "body": { "claim": id, "ref": "src/x.rs line 40", "status": "verified", "self_evident": false }
    });
    text.push_str(&serde_json::to_string(&odd).unwrap());
    text.push('\n');
    std::fs::write(&p, text).unwrap();

    let out = run(&dir, &["verify"]);
    assert!(out.status.success());
    let sout = String::from_utf8_lossy(&out.stdout);
    assert!(
        sout.contains("src/x.rs line 40"),
        "the unparseable pointer must be named, not dropped: {sout}"
    );
    assert!(
        sout.contains("unparseable"),
        "ev must say it cannot read the pointer, and guess no status: {sout}"
    );
    assert!(
        !sout.contains("resolves"),
        "ev must not carry the recorded status forward as a finding: {sout}"
    );

    let out = run(&dir, &["verify", "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let check = v["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["ref"] == "src/x.rs line 40")
        .expect("the unparseable pointer must appear in --json too");
    assert_eq!(check["liveness"], "unparseable");
    assert!(
        check.get("status").is_none(),
        "ev checked nothing here, so it reports no status"
    );
}
