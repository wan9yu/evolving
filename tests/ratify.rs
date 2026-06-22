//! `ev ratify` + `ev pending` — the human bridge that closes the propose→ratify seam. ratify mints a
//! human-now, user-ruled CHILD of an agent proposal (copying its hashed payload) with a `ratifies`
//! edge; the proposal stays immutable. pending is the pull-only view of un-ratified proposals.
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ev() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ev"))
}
fn repo() -> std::path::PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "ev-ratify-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    assert!(ev()
        .arg("init")
        .current_dir(&p)
        .output()
        .unwrap()
        .status
        .success());
    p
}
fn run(repo: &std::path::Path, args: &[&str]) -> std::process::Output {
    ev().args(args).current_dir(repo).output().unwrap()
}
/// Propose a decision (as an agent) and return its tick id.
fn propose(repo: &std::path::Path) -> String {
    let out = ev()
        .args([
            "propose",
            "the cache is write-through",
            "--assume",
            "writes hit the DB first",
            "--reject",
            "redis: an extra op dependency",
        ])
        .env("EV_AGENT_ID", "agent:mac")
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "propose: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_string()
}
fn tick(repo: &std::path::Path, id: &str) -> serde_json::Value {
    let raw = std::fs::read_to_string(repo.join(".evolving/ticks").join(id)).unwrap();
    serde_json::from_str(&raw).unwrap()
}

#[test]
fn pending_lists_a_proposal_then_excludes_it_after_ratify() {
    let r = repo();
    let pid = propose(&r);

    // pending shows the un-ratified proposal
    let p1 = run(&r, &["pending"]);
    assert!(
        String::from_utf8_lossy(&p1.stdout).contains("the cache is write-through"),
        "pending lists the un-ratified proposal; was {:?}",
        String::from_utf8_lossy(&p1.stdout)
    );

    // ratify it
    let out = run(&r, &["ratify", &pid, "--blame", "Wang Yu"]);
    assert!(
        out.status.success(),
        "ratify: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // pending now excludes it (collapsed into its user-ruled child)
    let p2 = run(&r, &["pending"]);
    assert!(
        String::from_utf8_lossy(&p2.stdout).contains("no proposals awaiting"),
        "pending excludes the ratified proposal; was {:?}",
        String::from_utf8_lossy(&p2.stdout)
    );
}

#[test]
fn ratify_mints_a_user_ruled_human_now_child_with_the_ratifies_edge_and_keeps_the_proposal_immutable(
) {
    let r = repo();
    let pid = propose(&r);
    let before = std::fs::read_to_string(r.join(".evolving/ticks").join(&pid)).unwrap();

    let out = run(&r, &["ratify", &pid, "--blame", "Wang Yu"]);
    assert!(out.status.success());
    // "ratified <pid> → <child> (now user-ruled, human-now)"
    let s = String::from_utf8_lossy(&out.stdout);
    let child = s.split_whitespace().nth(3).unwrap().to_string();

    let v = tick(&r, &child);
    assert_eq!(v["authority"], "user-ruled");
    assert!(
        v.get("provenance").is_none(),
        "human-now is the absent provenance default (same as ev decide); was {:?}",
        v.get("provenance")
    );
    assert_eq!(v["ratifies"], pid, "the child carries the ratifies edge");
    assert_eq!(
        v["decision"], "the cache is write-through",
        "the child copies the proposal's hashed payload"
    );

    // the proposal itself is never rewritten
    let after = std::fs::read_to_string(r.join(".evolving/ticks").join(&pid)).unwrap();
    assert_eq!(before, after, "the proposal must stay immutable");
}

#[test]
fn ratified_decision_surfaces_in_brief_only_after_ratification() {
    let r = repo();
    let pid = propose(&r);
    // while a proposal, it is excluded from the boot-read (§五)
    assert!(
        String::from_utf8_lossy(&run(&r, &["brief"]).stdout).contains("no user-ruled"),
        "an un-ratified proposal must not surface in brief"
    );
    run(&r, &["ratify", &pid, "--blame", "Wang Yu"]);
    assert!(
        String::from_utf8_lossy(&run(&r, &["brief"]).stdout).contains("the cache is write-through"),
        "the ratified ruling surfaces in brief as user-ruled"
    );
}

#[test]
fn ratify_refuses_a_non_proposal() {
    let r = repo();
    // a human decision, not an agent proposal
    let out = ev()
        .args(["decide", "a human ruling", "--blame", "Wang Yu"])
        .current_dir(&r)
        .output()
        .unwrap();
    let did = String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_string();
    let bad = run(&r, &["ratify", &did, "--blame", "Wang Yu"]);
    assert!(
        !bad.status.success(),
        "ratify only ratifies an agent proposal"
    );
    assert!(String::from_utf8_lossy(&bad.stderr).contains("only ratifies an agent proposal"));
}

#[test]
fn ratify_requires_blame() {
    let r = repo();
    let pid = propose(&r);
    let out = run(&r, &["ratify", &pid]); // no --blame
    assert!(
        !out.status.success(),
        "ratify requires --blame (the ratifying human, never auto-filled)"
    );
}

#[test]
fn ratify_round_trips_clean_through_verify_and_show_surfaces_the_edge() {
    let r = repo();
    let pid = propose(&r);
    let out = run(&r, &["ratify", &pid, "--blame", "Wang Yu"]);
    let child = String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .nth(3)
        .unwrap()
        .to_string();
    assert!(
        run(&r, &["verify"]).status.success(),
        "the ratified chain must verify clean"
    );
    let show = run(&r, &["show", &child]);
    assert!(
        String::from_utf8_lossy(&show.stdout).contains(&format!("ratifies: {pid}")),
        "show surfaces the ratifies edge"
    );
}
