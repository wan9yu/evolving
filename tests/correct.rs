//! `ev correct` — the new-child correction of a stale non-hashed tag, end-to-end against the real
//! binary. A correction appends a CHILD carrying the corrected tag; the target tick is never rewritten
//! (immutability), and `brief` then surfaces the corrected decision.
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ev() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ev"))
}

fn repo() -> std::path::PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "ev-correct-{}-{}",
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

#[test]
fn correct_should_surface_a_decision_in_brief_after_its_authority_is_corrected_to_user_ruled() {
    // given: a stale authority-omitted ruling that does NOT surface in brief
    let (r, id) = repo_with_stale_ruling();
    assert!(String::from_utf8_lossy(&run(&r, &["brief"]).stdout).contains("no user-ruled"));

    // when: a human corrects its authority to user-ruled
    let out = run(
        &r,
        &[
            "correct",
            &id,
            "--authority",
            "user-ruled",
            "--blame",
            "You",
        ],
    );

    // then: it succeeds and the ruling now surfaces in the boot-read (the gateway #247/#1458 fix)
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let brief = run(&r, &["brief"]);
    assert!(
        String::from_utf8_lossy(&brief.stdout).contains("#247/#1458 Insights scope"),
        "the corrected ruling must surface in brief"
    );
}

#[test]
fn correct_should_record_a_corrects_backlink_to_the_target_and_surface_it() {
    // given: a stale ruling
    let (r, id) = repo_with_stale_ruling();

    // when: it is corrected
    let out = run(
        &r,
        &[
            "correct",
            &id,
            "--authority",
            "user-ruled",
            "--blame",
            "You",
        ],
    );
    assert!(out.status.success());
    let child = String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_string();

    // then: the child carries the explicit `corrects:<target>` relation-overlay edge on disk
    let raw = std::fs::read_to_string(r.join(".evolving/ticks").join(&child)).unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        v.get("corrects").and_then(|x| x.as_str()),
        Some(id.as_str()),
        "the child records which tick it corrects"
    );

    // and: `ev show` surfaces the edge so the correction is traceable
    let show = run(&r, &["show", &child]);
    assert!(
        String::from_utf8_lossy(&show.stdout).contains(&format!("corrects: {id}")),
        "show must surface the corrects edge"
    );
}

#[test]
fn correct_should_collapse_a_correction_chain_to_its_tip() {
    // given: a stale ruling, corrected once (authority), then the child corrected again (provenance)
    // — a chain stale <- child1 <- child2, each carrying a corrects edge to its predecessor
    let (r, id) = repo_with_stale_ruling();
    let c1 = run(
        &r,
        &[
            "correct",
            &id,
            "--authority",
            "user-ruled",
            "--blame",
            "You",
        ],
    );
    assert!(c1.status.success());
    let child1 = String::from_utf8_lossy(&c1.stdout)
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_string();
    let c2 = run(
        &r,
        &[
            "correct",
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

    // when: the inventory is read
    let list = run(&r, &["list"]);
    let l = String::from_utf8_lossy(&list.stdout);

    // then: the chain collapses to its TIP — the decision appears exactly once (3 ticks, 1 entry),
    // while `ev log` still shows the full lineage (all three ticks)
    assert_eq!(
        l.matches("#247/#1458 Insights scope").count(),
        1,
        "a correction chain collapses to one current entry; list was {l:?}"
    );
    let log = run(&r, &["log"]);
    let ticks = String::from_utf8_lossy(&log.stdout)
        .lines()
        .filter(|line| line.contains("#247/#1458 Insights scope"))
        .count();
    assert_eq!(
        ticks, 3,
        "log keeps the full lineage (stale + child1 + child2)"
    );
}

#[test]
fn brief_should_still_collapse_edgeless_identical_ticks_via_content_equality() {
    // given: two ticks with the SAME hashed payload but NO corrects edge (the pre-0.1.10 / legacy
    // shape — here two canonical imports under different source_refs, the LATER one user-ruled).
    // Migrate never writes a corrects edge, so the collapse must fall back to content-equality.
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

    // when: the boot-read runs (two edge-less ticks share a decision_identity)
    let brief = run(&r, &["brief"]);

    // then: the content-equality fallback still collapses them to the latest — the decision surfaces
    // exactly once (backward-compat: a pre-0.1.10 corrective child with no edge still collapses)
    let b = String::from_utf8_lossy(&brief.stdout);
    assert_eq!(
        b.matches("legacy dup decision").count(),
        1,
        "edge-less identical ticks collapse via content-equality; brief was {b:?}"
    );
}

#[test]
fn correct_should_keep_the_target_tick_immutable_and_append_a_child() {
    // given: a stale ruling
    let (r, id) = repo_with_stale_ruling();
    let before = std::fs::read_to_string(r.join(".evolving/ticks").join(&id)).unwrap();

    // when: it is corrected
    assert!(run(
        &r,
        &[
            "correct",
            &id,
            "--authority",
            "user-ruled",
            "--blame",
            "You"
        ]
    )
    .status
    .success());

    // then: the TARGET tick is byte-unchanged (never rewritten), and a NEW child tick exists
    let after = std::fs::read_to_string(r.join(".evolving/ticks").join(&id)).unwrap();
    assert_eq!(
        before, after,
        "the corrected target must never be rewritten in place"
    );
    let count = std::fs::read_dir(r.join(".evolving/ticks"))
        .unwrap()
        .filter(|e| e.as_ref().unwrap().path().is_file())
        .count();
    assert_eq!(count, 2, "a corrective child is appended (target + child)");
}

#[test]
fn correct_should_round_trip_clean_through_verify() {
    // given: a corrected ruling
    let (r, id) = repo_with_stale_ruling();
    assert!(run(
        &r,
        &[
            "correct",
            &id,
            "--authority",
            "user-ruled",
            "--blame",
            "You"
        ]
    )
    .status
    .success());

    // when/then: the chain (target + corrective child) verifies clean
    assert!(
        run(&r, &["verify"]).status.success(),
        "the corrected chain must verify clean"
    );
}

#[test]
fn correct_should_refuse_a_no_op_when_the_tag_is_already_set() {
    // given: a ruling corrected to user-ruled (the child now carries user-ruled)
    let (r, id) = repo_with_stale_ruling();
    let out = run(
        &r,
        &[
            "correct",
            &id,
            "--authority",
            "user-ruled",
            "--blame",
            "You",
        ],
    );
    let child = String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_string();

    // when: the CHILD is corrected to the SAME authority it already carries
    let noop = run(
        &r,
        &[
            "correct",
            &child,
            "--authority",
            "user-ruled",
            "--blame",
            "You",
        ],
    );

    // then: it is refused (nothing to correct) and writes no tick
    assert!(!noop.status.success(), "a no-op correction must be refused");
    assert!(String::from_utf8_lossy(&noop.stderr).contains("nothing to correct"));
}

#[test]
fn correct_should_require_at_least_one_tag() {
    // given: a ruling
    let (r, id) = repo_with_stale_ruling();

    // when: correct is run with no tag to change
    let out = run(&r, &["correct", &id, "--blame", "You"]);

    // then: it is refused
    assert!(!out.status.success(), "correct needs at least one tag");
}

#[test]
fn correct_should_fail_on_an_unknown_tick_id() {
    // given: a store
    let (r, _id) = repo_with_stale_ruling();

    // when: correct names a tick that does not exist
    let out = run(
        &r,
        &[
            "correct",
            "deadbeefdead",
            "--authority",
            "user-ruled",
            "--blame",
            "You",
        ],
    );

    // then: it fails (no such tick)
    assert!(!out.status.success(), "an unknown tick id must fail");
}
