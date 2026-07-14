//! The pair must be measured at ONE instant. A cell that joins a status the ledger
//! recorded months ago with a drift counted just now is not a fact about the world —
//! it is two facts about two different worlds, printed as one sentence.

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

fn fresh_git(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ev-{tag}-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q"]);
    dir
}

fn commit(dir: &std::path::Path, msg: &str) {
    git(dir, &["add", "-A"]);
    git(dir, &["-c", "commit.gpgsign=false", "commit", "-qm", msg]);
}

fn brief(dir: &std::path::Path) -> serde_json::Value {
    let out = ev(dir, &["brief", "--json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
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

/// A repo whose claim is anchored to a line of `a.rs`, filed and recorded `resolves`.
fn anchored(tag: &str) -> (std::path::PathBuf, String) {
    let dir = fresh_git(tag);
    std::fs::write(dir.join("a.rs"), "fn parse() {}\n").unwrap();
    commit(&dir, "one");
    assert!(ev(&dir, &["init"]).status.success());
    assert!(ev(&dir, &["claim", "the parser is fixed", "--by", "agent"])
        .status
        .success());
    let cid = brief(&dir)["open"][0]["id"].as_str().unwrap().to_string();
    let out = ev(&dir, &["evidence", &cid, "file:a.rs::fn parse("]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        brief(&dir)["open"][0]["evidence"][0]["status"].as_str(),
        Some("resolves"),
        "the filing records resolves"
    );
    (dir, cid)
}

/// FINDING 1 — the read path must re-read the anchor, not trust a status the ledger
/// recorded at filing time. `ev verify` is a manual verb; nothing runs it here, exactly
/// as nothing runs it in a real repo between one pause and the next.
#[test]
fn a_deleted_file_reads_file_gone_at_the_pause_without_a_manual_verify() {
    let (dir, cid) = anchored("gone");

    // The world moves: the cited file is DELETED. No `ev verify` is run.
    std::fs::remove_file(dir.join("a.rs")).unwrap();
    commit(&dir, "delete the parser");

    let v = brief(&dir);
    let e = &v["open"][0]["evidence"][0];
    assert_eq!(
        e["cell"].as_str(),
        Some("file-gone"),
        "the container is gone; the cell must say so: {v}"
    );
    assert_ne!(
        e["cell"].as_str(),
        Some("neighborhood-moved"),
        "`the line stands; code moved beside it` is an assertion about a line ev never read: {v}"
    );
    assert_eq!(
        e["status"].as_str(),
        Some("gone"),
        "status and cell must be halves of ONE reading: {v}"
    );

    // And the remedy path must not launder it green: an ack says the HUMAN looked, it
    // does not make an absent file present.
    assert!(ev(&dir, &["ack", &cid, "--i-am-the-human"])
        .status
        .success());
    let v = brief(&dir);
    assert_eq!(
        v["open"][0]["evidence"][0]["cell"].as_str(),
        Some("file-gone"),
        "an ack must never turn a gone anchor `still`: {v}"
    );
}

/// FINDING 2 — the ack reference is preferred when it CAN BE COUNTED AGAINST, not merely
/// when it is present. The ledger is shared across clones: an ack taken on a branch that
/// was squash-merged and deleted names a sha that resolves nowhere. Falling back to the
/// pinned `base` is not a re-base — `base` is the original pin and the larger count.
#[test]
fn an_ack_sha_that_resolves_nowhere_falls_back_to_the_pinned_base() {
    use evolving::verify::{drift_since, EvRef, Seen};

    let dir = fresh_git("ghost-ack");
    std::fs::write(dir.join("f.txt"), "the invariant\n").unwrap();
    commit(&dir, "one");
    let base = String::from_utf8_lossy(
        &Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&dir)
            .output()
            .unwrap()
            .stdout,
    )
    .trim()
    .to_string();

    std::fs::write(dir.join("f.txt"), "the invariant, rewritten\n").unwrap();
    commit(&dir, "two");

    let r = EvRef::parse("file:f.txt").unwrap();
    // a sha of the right shape that no clone carries — the squash-merged branch
    let ghost = "0123456789abcdef0123456789abcdef01234567";
    assert_eq!(
        drift_since(&dir, Some(ghost), Some(&base), &r, &Seen::new()),
        Some(1),
        "an unresolvable ack must fall back to the pinned base, not disarm the ratchet"
    );
    // with no base to fall back to, ev asserts nothing — that is correct.
    assert_eq!(drift_since(&dir, Some(ghost), None, &r, &Seen::new()), None);
}

/// FINDING 3 — the movement census must not drop the claims it could not measure. A
/// census that silently narrows its own denominator is the undercount doctor exists to
/// expose.
#[test]
fn the_movement_census_counts_the_claims_it_could_not_place() {
    let (dir, _cid) = anchored("census");
    // a second claim anchored to a commit: — immutable, no path, so no cell exists for it
    let sha = String::from_utf8_lossy(
        &Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&dir)
            .output()
            .unwrap()
            .stdout,
    )
    .trim()
    .to_string();
    assert!(ev(
        &dir,
        &[
            "claim",
            "shipped",
            "--by",
            "agent",
            "--evidence",
            &format!("commit:{sha}"),
        ]
    )
    .status
    .success());

    let out = ev(&dir, &["doctor"]);
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        s.contains("2 claims"),
        "the census denominator must be every claim carrying evidence: {s}"
    );
    assert!(
        s.contains("unmeasured 1"),
        "the claim ev could not place on the movement map must be counted out loud: {s}"
    );
}

/// FINDING 4 — `k` (still stands → ack) is offered only where an ack can clear the cell.
/// `Cell::of` ignores drift for a changed or gone anchor, so no number of acks moves them:
/// offering the key invites the human to press it forever.
#[test]
fn the_pause_does_not_offer_ack_on_an_anchor_ack_cannot_clear() {
    let (dir, _cid) = anchored("no-k");
    std::fs::remove_file(dir.join("a.rs")).unwrap();
    commit(&dir, "delete the parser");

    let mut child = Command::new(env!("CARGO_BIN_EXE_ev"))
        .args(["pause", "--script"])
        .current_dir(&dir)
        .env_remove("CLAUDECODE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // the human presses `k` anyway — the key must not be honoured
    child.stdin.take().unwrap().write_all(b"k\nn\n").unwrap();
    let out = child.wait_with_output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        s.contains("the cited file is gone"),
        "the claim stays visible: {s}"
    );
    assert!(
        !s.contains("[k] still stands"),
        "a key that structurally cannot clear the cell must not be offered: {s}"
    );
    assert!(
        s.contains("ev evidence"),
        "the human must be told the anchor has to be re-filed: {s}"
    );
    assert!(
        !ledger_events(&dir).iter().any(|e| e["type"] == "ack"),
        "no ack may be written for a cell an ack cannot clear"
    );
}
