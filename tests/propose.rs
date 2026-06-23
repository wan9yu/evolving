//! `ev propose` — the AGENT door. Always `agent-proposed` + `agent-disposable`, unbound, and its
//! blame NEVER falls through to git config (the gateway hole). Inert until a human ratifies it.
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ev() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ev"))
}
fn repo() -> std::path::PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "ev-propose-{}-{}",
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
    ev().args(args)
        .env_remove("EV_AGENT_ID")
        .current_dir(repo)
        .output()
        .unwrap()
}
/// The created tick's on-disk JSON (parsed from the `proposed <id> …` confirmation).
fn tick_of(repo: &std::path::Path, stdout: &str) -> serde_json::Value {
    let id = stdout
        .split_whitespace()
        .nth(1)
        .expect("an id in the output");
    let raw = std::fs::read_to_string(repo.join(".evolving/ticks").join(id)).unwrap();
    serde_json::from_str(&raw).unwrap()
}

#[test]
fn propose_should_stamp_agent_proposed_agent_disposable_and_default_blame_to_agent() {
    // given/when: an agent proposes with no --blame and no EV_AGENT_ID
    let r = repo();
    let out = run(
        &r,
        &[
            "propose",
            "the cache is write-through",
            "--assume",
            "writes hit the DB first",
        ],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        s.contains("proposed ") && s.contains("agent-proposed"),
        "the confirmation names it agent-proposed; was {s:?}"
    );

    // then: the tick is agent-proposed + agent-disposable, and blame is the generic "agent" (NOT git)
    let v = tick_of(&r, &s);
    assert_eq!(v["provenance"], "agent-proposed");
    assert_eq!(v["authority"], "agent-disposable");
    assert_eq!(
        v["blame"], "agent",
        "blame defaults to 'agent' — never the human's git config"
    );
}

#[test]
fn propose_should_take_blame_from_ev_agent_id_when_no_blame_flag() {
    // given: the runner declares the agent identity via EV_AGENT_ID
    let r = repo();
    let out = ev()
        .args(["propose", "d", "--assume", "c"])
        .env("EV_AGENT_ID", "agent:gateway-mac")
        .current_dir(&r)
        .output()
        .unwrap();
    assert!(out.status.success());
    let v = tick_of(&r, &String::from_utf8_lossy(&out.stdout));
    assert_eq!(v["blame"], "agent:gateway-mac");
    assert_eq!(v["provenance"], "agent-proposed");
}

#[test]
fn propose_should_prefer_an_explicit_blame_over_env_and_default() {
    // given: both an explicit --blame and EV_AGENT_ID
    let r = repo();
    let out = ev()
        .args(["propose", "d", "--assume", "c", "--blame", "agent:uspi"])
        .env("EV_AGENT_ID", "agent:other")
        .current_dir(&r)
        .output()
        .unwrap();
    assert!(out.status.success());
    let v = tick_of(&r, &String::from_utf8_lossy(&out.stdout));
    assert_eq!(
        v["blame"], "agent:uspi",
        "explicit --blame wins over EV_AGENT_ID"
    );
}

#[test]
fn propose_should_refuse_binding_and_authority_flags_as_unbound() {
    // a proposal is unbound + agent-authored: the binding flags and human-only --authority are refused
    let r = repo();
    for bad in [
        vec!["propose", "d", "--assume", "c", "--assume-test", "pytest x"],
        vec!["propose", "d", "--authority", "user-ruled"],
        vec!["propose", "d", "--assume", "c", "--revisit", "Q3"],
    ] {
        let out = run(&r, &bad);
        assert!(!out.status.success(), "propose must refuse {bad:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("UNBOUND"),
            "a clear refusal for {bad:?}; stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[test]
fn proposed_decision_should_not_surface_in_brief() {
    // the §五 guarantee: an agent proposal never reaches the boot-read until a human ratifies it
    let r = repo();
    assert!(run(
        &r,
        &[
            "propose",
            "the cache is write-through",
            "--reject",
            "redis: an extra op dependency"
        ]
    )
    .status
    .success());
    let brief = run(&r, &["brief"]);
    let b = String::from_utf8_lossy(&brief.stdout);
    assert!(
        b.contains("no user-ruled"),
        "an agent proposal must not surface in brief; was {b:?}"
    );
}

#[test]
fn propose_json_should_emit_the_citable_envelope() {
    // given/when: --json (the machine sibling the runner records to cite at ratify)
    let r = repo();
    let out = ev()
        .args(["propose", "d", "--assume", "c", "--json"])
        .env_remove("EV_AGENT_ID")
        .current_dir(&r)
        .output()
        .unwrap();
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(s.trim()).expect("valid json envelope");
    assert_eq!(v["kind"], "ev-proposed");
    assert_eq!(v["provenance"], "agent-proposed");
    assert_eq!(v["authority"], "agent-disposable");
    assert_eq!(v["id"].as_str().unwrap().len(), 12, "a citable 12-hex id");
}

#[test]
fn propose_should_round_trip_clean_through_verify() {
    // a proposed tick is a first-class tick on the chain — it must verify clean
    let r = repo();
    assert!(run(&r, &["propose", "d", "--assume", "c"]).status.success());
    assert!(
        run(&r, &["verify"]).status.success(),
        "a proposed tick must verify clean"
    );
}

#[test]
fn propose_should_be_idempotent_on_a_repeated_source_ref() {
    // given: a proposal carrying an explicit source_ref (the runner's round / work-unit key)
    let r = repo();
    let first = run(
        &r,
        &[
            "propose",
            "the cache is write-through",
            "--assume",
            "writes hit the DB first",
            "--source-ref",
            "round-7",
        ],
    );
    assert!(first.status.success());

    // when: the SAME source_ref is proposed again (a memoryless agent re-proposing the round, even with
    // reworded text)
    let second = run(
        &r,
        &[
            "propose",
            "the cache is write-through (reworded)",
            "--assume",
            "writes hit the DB first",
            "--source-ref",
            "round-7",
        ],
    );

    // then: it is a no-op — the source_ref already names a pending proposal, so no duplicate piles up,
    // and `ev pending` still shows exactly one
    assert!(second.status.success());
    assert!(
        String::from_utf8_lossy(&second.stdout)
            .to_lowercase()
            .contains("already proposed"),
        "the repeat should report an idempotent no-op; was {:?}",
        String::from_utf8_lossy(&second.stdout)
    );
    let pend = run(&r, &["pending"]);
    let proposals = String::from_utf8_lossy(&pend.stdout)
        .lines()
        .filter(|l| l.contains("live"))
        .count();
    assert_eq!(
        proposals,
        1,
        "exactly one proposal should remain for the source_ref (idempotent); pending:\n{}",
        String::from_utf8_lossy(&pend.stdout)
    );
}

#[test]
fn pending_should_surface_the_source_ref_for_triage_on_the_rich_row() {
    // given: a proposal with a source_ref in the (un-triaged) pending queue
    let r = repo();
    run(
        &r,
        &[
            "propose",
            "the cache is write-through",
            "--assume",
            "c",
            "--source-ref",
            "round-7",
        ],
    );

    // when: `ev pending` renders for a human (rich path; the plain/scriptable path stays byte-stable)
    let out = ev()
        .args(["pending", "--color", "always"])
        .env_remove("EV_AGENT_ID")
        .current_dir(&r)
        .output()
        .unwrap();

    // then: the row surfaces the source_ref so a piling queue is scannable (the dogfood friction was a
    // pending view that hid held_since/source_ref)
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        s.contains("round-7"),
        "the rich pending row should surface the source_ref for triage; was {s:?}"
    );
}
