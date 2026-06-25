//! `ev supersede` — replace a prior ruling, end-to-end against the real binary. Two branches:
//! RE-TAG (id + tags, no new ruling — copies the hashed payload, fixes a standing tag) and OVERTURN
//! (id + a new ruling + `--assume` why — a fresh decision replaces the prior one). Both append a CHILD
//! carrying a `supersedes:<id>` edge; the target is never rewritten. The superseded tick leaves every
//! current view; `ev reopen <id>` marks it "superseded by".
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ev() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ev"))
}

fn repo() -> std::path::PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "ev-supersede-{}-{}",
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

/// The 2nd whitespace token of a "recorded <id> ..." / "re-tagged <id> ..." line — the child tick id.
fn child_id(stdout: &[u8]) -> String {
    String::from_utf8_lossy(stdout)
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_string()
}

/// Import one authority-omitted ruling and return (repo, the stale tick id).
fn repo_with_stale_ruling() -> (std::path::PathBuf, String) {
    let r = repo();
    let line = "{\"kind\":\"ev-decision-intake\",\"decision\":\"#247/#1458 Insights scope\",\"grounds\":[],\"blame\":\"Mac\",\"source_ref\":\"#247/#1458\",\"provenance\":\"imported\"}\n";
    let path = r.join("p.jsonl");
    std::fs::write(&path, line).unwrap();
    assert!(run(
        &r,
        &[
            "migrate",
            "--source",
            &format!("canonical:{}", path.display())
        ]
    )
    .status
    .success());
    let id = std::fs::read_dir(r.join(".evolving/ticks"))
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().into_string().unwrap())
        .find(|n| n.len() == 12)
        .expect("one tick");
    (r, id)
}

// ---- RE-TAG branch (id + tags, no new ruling) ----

#[test]
fn retag_should_surface_a_decision_in_brief_after_its_authority_is_set_to_user_ruled() {
    // given: a stale authority-omitted ruling that does NOT surface in brief
    let (r, id) = repo_with_stale_ruling();
    assert!(String::from_utf8_lossy(&run(&r, &["brief"]).stdout).contains("no user-ruled"));

    // when: a human re-tags its authority to user-ruled (no new ruling)
    let out = run(
        &r,
        &[
            "supersede",
            &id,
            "--authority",
            "user-ruled",
            "--blame",
            "You",
        ],
    );

    // then: it succeeds and the ruling now surfaces in the boot-read
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let brief = run(&r, &["brief"]);
    assert!(
        String::from_utf8_lossy(&brief.stdout).contains("#247/#1458 Insights scope"),
        "the re-tagged ruling must surface in brief"
    );
}

#[test]
fn retag_should_record_a_supersedes_backlink_to_the_target_and_surface_it() {
    // given: a stale ruling
    let (r, id) = repo_with_stale_ruling();

    // when: it is re-tagged
    let out = run(
        &r,
        &[
            "supersede",
            &id,
            "--authority",
            "user-ruled",
            "--blame",
            "You",
        ],
    );
    assert!(out.status.success());
    // a re-tag copies the parent's grounds verbatim — restating "(N ground(s))" would be a zero-entropy
    // echo of the parent (a count appears only at CREATION: decide/propose/overturn, never a re-tag).
    assert!(
        !String::from_utf8_lossy(&out.stdout).contains("ground(s)"),
        "a re-tag must not restate the inherited ground count: {:?}",
        String::from_utf8_lossy(&out.stdout)
    );
    let child = child_id(&out.stdout);

    // then: the child carries the explicit `supersedes:<target>` relation-overlay edge on disk
    let raw = std::fs::read_to_string(r.join(".evolving/ticks").join(&child)).unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        v.get("supersedes").and_then(|x| x.as_str()),
        Some(id.as_str()),
        "the child records which tick it supersedes"
    );

    // and: `ev show` carries the edge (pure JSON — the supersedes field lives in the tick)
    let show = run(&r, &["show", &child]);
    let v: serde_json::Value =
        serde_json::from_slice(&show.stdout).expect("ev show emits pure JSON");
    assert_eq!(v["supersedes"], id, "show must carry the supersedes edge");
}

#[test]
fn retag_should_collapse_a_chain_to_its_tip() {
    // given: a stale ruling, re-tagged once (authority), then the child re-tagged again (provenance)
    let (r, id) = repo_with_stale_ruling();
    let c1 = run(
        &r,
        &[
            "supersede",
            &id,
            "--authority",
            "user-ruled",
            "--blame",
            "You",
        ],
    );
    assert!(c1.status.success());
    let child1 = child_id(&c1.stdout);
    let c2 = run(
        &r,
        &[
            "supersede",
            &child1,
            "--provenance",
            "human-now",
            "--blame",
            "You",
        ],
    );
    assert!(
        c2.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&c2.stderr)
    );

    // then: the chain collapses to its TIP — the decision appears exactly once in list (3 ticks, 1
    // entry), while `ev log` still shows the full lineage
    let l = String::from_utf8_lossy(&run(&r, &["list"]).stdout).to_string();
    assert_eq!(
        l.matches("#247/#1458 Insights scope").count(),
        1,
        "a re-tag chain collapses to one current entry; list was {l:?}"
    );
    let ticks = String::from_utf8_lossy(&run(&r, &["log"]).stdout)
        .lines()
        .filter(|line| line.contains("#247/#1458 Insights scope"))
        .count();
    assert_eq!(ticks, 3, "log keeps the full lineage");
}

#[test]
fn brief_should_still_collapse_edgeless_identical_ticks_via_content_equality() {
    // given: two ticks with the SAME hashed payload but NO supersedes edge (the legacy shape — two
    // canonical imports under different source_refs, the LATER one user-ruled). Migrate never writes
    // a supersedes edge, so the collapse must fall back to content-equality.
    let r = repo();
    let body = "{\"kind\":\"ev-decision-intake\",\"decision\":\"legacy dup decision\",\"grounds\":[],\"blame\":\"Mac\",\"source_ref\":\"R-a\",\"provenance\":\"imported\"}\n\
{\"kind\":\"ev-decision-intake\",\"decision\":\"legacy dup decision\",\"grounds\":[],\"blame\":\"Mac\",\"authority\":\"user-ruled\",\"source_ref\":\"R-b\",\"provenance\":\"imported\"}\n";
    let path = r.join("legacy.jsonl");
    std::fs::write(&path, body).unwrap();
    assert!(run(
        &r,
        &[
            "migrate",
            "--source",
            &format!("canonical:{}", path.display())
        ]
    )
    .status
    .success());

    // then: the content-equality fallback still collapses them to the latest
    let b = String::from_utf8_lossy(&run(&r, &["brief"]).stdout).to_string();
    assert_eq!(
        b.matches("legacy dup decision").count(),
        1,
        "edge-less identical ticks collapse via content-equality; brief was {b:?}"
    );
}

#[test]
fn retag_should_keep_the_target_tick_immutable_and_append_a_child() {
    let (r, id) = repo_with_stale_ruling();
    let before = std::fs::read_to_string(r.join(".evolving/ticks").join(&id)).unwrap();
    assert!(run(
        &r,
        &[
            "supersede",
            &id,
            "--authority",
            "user-ruled",
            "--blame",
            "You"
        ]
    )
    .status
    .success());
    let after = std::fs::read_to_string(r.join(".evolving/ticks").join(&id)).unwrap();
    assert_eq!(before, after, "the target must never be rewritten in place");
    let count = std::fs::read_dir(r.join(".evolving/ticks"))
        .unwrap()
        .filter(|e| e.as_ref().unwrap().path().is_file())
        .count();
    assert_eq!(count, 2, "a child is appended (target + child)");
}

#[test]
fn retag_should_round_trip_clean_through_verify() {
    let (r, id) = repo_with_stale_ruling();
    assert!(run(
        &r,
        &[
            "supersede",
            &id,
            "--authority",
            "user-ruled",
            "--blame",
            "You"
        ]
    )
    .status
    .success());
    assert!(
        run(&r, &["verify"]).status.success(),
        "the superseded chain must verify clean"
    );
}

#[test]
fn retag_should_refuse_a_no_op_when_the_tag_is_already_set() {
    let (r, id) = repo_with_stale_ruling();
    let out = run(
        &r,
        &[
            "supersede",
            &id,
            "--authority",
            "user-ruled",
            "--blame",
            "You",
        ],
    );
    let child = child_id(&out.stdout);
    // re-tag the CHILD to the SAME authority it already carries
    let noop = run(
        &r,
        &[
            "supersede",
            &child,
            "--authority",
            "user-ruled",
            "--blame",
            "You",
        ],
    );
    assert!(!noop.status.success(), "a no-op re-tag must be refused");
    assert!(String::from_utf8_lossy(&noop.stderr).contains("nothing to re-tag"));
}

#[test]
fn retag_should_require_at_least_one_tag() {
    let (r, id) = repo_with_stale_ruling();
    // no new ruling AND no tag → nothing to do
    let out = run(&r, &["supersede", &id, "--blame", "You"]);
    assert!(!out.status.success(), "a re-tag needs at least one tag");
}

#[test]
fn supersede_should_fail_on_an_unknown_tick_id() {
    let (r, _id) = repo_with_stale_ruling();
    let out = run(
        &r,
        &[
            "supersede",
            "deadbeefdead",
            "--authority",
            "user-ruled",
            "--blame",
            "You",
        ],
    );
    assert!(!out.status.success(), "an unknown tick id must fail");
}

// ---- OVERTURN branch (id + a new ruling + --assume why) ----

/// Record a user-ruled decision and return (repo, its id).
fn repo_with_live_ruling(text: &str) -> (std::path::PathBuf, String) {
    let r = repo();
    let out = run(
        &r,
        &[
            "decide",
            text,
            "--assume",
            "the original reason",
            "--authority",
            "user-ruled",
            "--blame",
            "You",
        ],
    );
    assert!(
        out.status.success(),
        "decide: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let id = child_id(&out.stdout);
    (r, id)
}

#[test]
fn overturn_should_replace_a_prior_ruling_and_drop_it_from_current_views() {
    // given: a live user-ruled ruling
    let (r, old) = repo_with_live_ruling("http handling = buffer");

    // when: a human overturns it with a NEW ruling + a reason
    let sup = run(
        &r,
        &[
            "supersede",
            &old,
            "http handling = stream",
            "--assume",
            "buffering OOMs at scale",
            "--authority",
            "user-ruled",
            "--blame",
            "You",
        ],
    );
    assert!(
        sup.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&sup.stderr)
    );
    let new = child_id(&sup.stdout);

    // then: the new ruling carries supersedes:<old>, and list shows the NEW, not the overturned old
    let raw = std::fs::read_to_string(r.join(".evolving/ticks").join(&new)).unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        v.get("supersedes").and_then(|x| x.as_str()),
        Some(old.as_str())
    );
    let l = String::from_utf8_lossy(&run(&r, &["list"]).stdout).to_string();
    assert!(
        l.contains("http handling = stream"),
        "the new ruling must surface: {l}"
    );
    assert!(
        !l.contains("http handling = buffer"),
        "the overturned ruling must leave the current view: {l}"
    );
}

#[test]
fn overturn_should_require_a_reason() {
    let (r, old) = repo_with_live_ruling("use buffering");
    // a new ruling with NO --assume is refused — a supersession must say why
    let sup = run(&r, &["supersede", &old, "use streaming", "--blame", "You"]);
    assert!(
        !sup.status.success(),
        "an overturn with no reason must be refused"
    );
    assert!(
        String::from_utf8_lossy(&sup.stderr).contains("--assume"),
        "the refusal must name --assume: {}",
        String::from_utf8_lossy(&sup.stderr)
    );
}

#[test]
fn reopen_should_mark_a_ruling_that_has_been_superseded() {
    // given: a ruling overturned by a new one
    let (r, old) = repo_with_live_ruling("use buffering");
    let sup = run(
        &r,
        &[
            "supersede",
            &old,
            "use streaming",
            "--assume",
            "scale",
            "--blame",
            "You",
        ],
    );
    assert!(
        sup.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&sup.stderr)
    );
    let new = child_id(&sup.stdout);

    // when: the OVERTURNED ruling is reopened directly (a cold reader holding its id)
    let ro = String::from_utf8_lossy(&run(&r, &["reopen", &old]).stdout).to_string();

    // then: it is marked "superseded by <new>" — its staleness is visible on inspection
    assert!(
        ro.contains("superseded by") && ro.contains(&new),
        "reopen must mark the overturned ruling superseded: {ro}"
    );
}
