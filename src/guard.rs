//! `ev guard "<selector>" <id> [<ground>]` — attach an existing test to a ground as a
//! data check (after the fact). Because `check` is hashed, this writes a NEW CHILD.
use crate::canonical::compute_id;
use crate::store::Store;
use crate::tick::{Check, Ground, Liveness, Tick};
use std::path::Path;

pub struct GuardArgs {
    pub selector: String,
    pub id: String,
    pub target: Option<String>, // ground claim or index; required if >1 unbound ground
    pub counter_test: String,
    pub platforms: Vec<String>,
    pub triggered_by: Vec<String>,
    pub surfaces: Vec<String>,
    pub verified_at_sha: Option<String>,
    pub blame: Option<String>,
    pub authority: Option<String>,
}

fn resolve_target(grounds: &[Ground], target: &Option<String>) -> Result<usize, String> {
    let unbound: Vec<usize> = grounds
        .iter()
        .enumerate()
        .filter(|(_, g)| g.check.is_none())
        .map(|(i, _)| i)
        .collect();
    match target {
        None => match unbound.as_slice() {
            [one] => Ok(*one),
            [] => Err("no unbound ground to guard".into()),
            _ => Err("more than one unbound ground — name the target (claim or index)".into()),
        },
        Some(t) => {
            if let Ok(idx) = t.parse::<usize>() {
                if idx < grounds.len() {
                    return Ok(idx);
                }
                return Err(format!("ground index {idx} out of range"));
            }
            let matches: Vec<usize> = grounds
                .iter()
                .enumerate()
                .filter(|(_, g)| g.claim == *t)
                .map(|(i, _)| i)
                .collect();
            match matches.as_slice() {
                [one] => Ok(*one),
                [] => Err(format!("no ground with claim {t:?}")),
                _ => Err(format!("ambiguous: multiple grounds with claim {t:?}")),
            }
        }
    }
}

pub fn run(repo: &Path, a: GuardArgs) -> Result<Tick, String> {
    let store = Store::at(repo);
    let parent = store
        .read_tick(&a.id)
        .map_err(|e| format!("{e}"))?
        .ok_or(format!("no tick with id {}", a.id))?;
    let head = store
        .read_head()
        .map_err(|e| format!("reading HEAD: {e}"))?;
    if a.id != head {
        return Err(format!(
            "guard can only amend the current HEAD decision; {} is not HEAD ({})",
            a.id, head
        ));
    }
    let idx = resolve_target(&parent.grounds, &a.target)?;
    let g = &parent.grounds[idx];
    // R2: a human-rechecked (person) ground can never be force-bound to a test.
    if let Some(Check::Person { .. }) = g.check {
        return Err("a human-rechecked ground cannot carry a test (R2 hard error)".into());
    }
    // Validate authority FIRST so the rejected-road tripwire gate below reads a vetted value (and an
    // out-of-vocab authority fails loudly rather than silently failing the user-ruled comparison).
    if let Some(val) = &a.authority {
        crate::capture::validate_authority(val)?;
    }
    // 0.1.8: a rejected road (closed by a ruling) may be guarded with a falsifiable tripwire, but
    // ONLY when the binding declares --authority user-ruled (the human's deliberate closed-road call).
    // Mirrors capture.rs build_ground; the counter-test stays required below (no harvested tripwire).
    // provenance is hard-stamped human-now on the child (line below), so a guard can never create an
    // agent-proposed gating tripwire.
    if g.supports.starts_with("rejected:") && a.authority.as_deref() != Some("user-ruled") {
        return Err(
            "a rejected road can carry a tripwire test only when guarded with --authority user-ruled"
                .into(),
        );
    }
    if g.check.is_some() {
        return Err("ground already has a check".into());
    }
    if a.counter_test.trim().is_empty() {
        return Err("a test binding requires a counter-test (no vacuous binding)".into());
    }
    if a.platforms.is_empty() || a.triggered_by.is_empty() || a.surfaces.is_empty() {
        return Err(
            "a test binding requires at least one platform, triggered-by, and surface".into(),
        );
    }
    let verified_at_sha = crate::capture::resolve_sha(repo, &a.verified_at_sha)?;
    let blame = crate::capture::resolve_blame(repo, a.blame)?;

    let mut grounds = parent.grounds.clone();
    grounds[idx] = Ground {
        claim: grounds[idx].claim.clone(),
        supports: grounds[idx].supports.clone(),
        check: Some(Check::Test {
            reference: a.selector,
            verified_at_sha,
            counter_test: Some(a.counter_test),
            liveness: Liveness {
                platforms: a.platforms,
                triggered_by: a.triggered_by,
                surfaces: a.surfaces,
            },
        }),
    };
    let held_since = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|e| format!("timestamp: {e}"))?;
    let mut child = Tick {
        id: String::new(),
        parent_id: parent.id.clone(),
        observe: parent.observe.clone(),
        decision: parent.decision.clone(),
        grounds,
        status: "live".into(),
        held_since,
        blame,
        authority: a.authority,
        jurisdiction: parent.jurisdiction.clone(), // a sibling tag of the decision; inherited by the child
        source_ref: parent.source_ref.clone(), // the opaque source identity of the decision; inherited by the child
        // NOT inherited: provenance is a property of the authorship ACT, not the decision. A guard is a
        // fresh human act, hard-stamped human-now (the absent default) — the launder defense.
        provenance: None,
        corrects: None,
    };
    child.id = compute_id(&child);
    store
        .write_tick(&child)
        .map_err(|e| format!("writing tick: {e}"))?;
    Ok(child)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_with_unbound() -> (std::path::PathBuf, String) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "ev-guard-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Store::at(&p).init().unwrap();
        let args: Vec<String> = [
            "--assume",
            "schema stays frozen",
            "--assume",
            "team ok",
            "--revisit",
            "Q3",
            "--blame",
            "Wang Yu",
        ]
        .iter()
        .map(|x| x.to_string())
        .collect();
        let t = crate::capture::run(&p, Some("build our own retrieval"), &args).unwrap();
        (p, t.id)
    }
    fn args(selector: &str, id: &str, target: Option<&str>) -> GuardArgs {
        GuardArgs {
            selector: selector.into(),
            id: id.into(),
            target: target.map(|s| s.into()),
            counter_test: "pytest x::counter".into(),
            platforms: vec!["linux-ci".into()],
            triggered_by: vec!["f".into()],
            surfaces: vec!["s".into()],
            verified_at_sha: Some("d308afac1b2c3d4e5f60718293a4b5c6d7e8f901".into()),
            blame: Some("Wang Yu".into()),
            authority: None,
        }
    }

    #[test]
    fn guard_should_bind_a_named_unbound_ground_and_write_a_child_when_the_target_is_named() {
        // given: a HEAD tick with an unbound "schema stays frozen" ground
        let (p, id) = repo_with_unbound();

        // when: that named ground is guarded
        let child = run(
            &p,
            args(
                "pytest tests/test_schema_frozen.py",
                &id,
                Some("schema stays frozen"),
            ),
        )
        .expect("ok");

        // then: a child is written and the named ground now carries a test check
        assert_eq!(child.parent_id, id);
        let i = child
            .grounds
            .iter()
            .position(|g| g.claim == "schema stays frozen")
            .unwrap();
        assert!(matches!(child.grounds[i].check, Some(Check::Test { .. })));
    }

    #[test]
    fn guard_should_still_error_without_a_counter_test() {
        // given: the migrate-only harvested path now exists; pin that the guard path is UNCHANGED
        // — guard.rs:83-85 stays byte-for-byte strict, so an empty counter-test STILL errors here
        // (harvesting drops the counter-test ONLY on the migrate path, never on `ev guard`).
        let (p, id) = repo_with_unbound();
        let mut a = args("pytest x", &id, Some("schema stays frozen"));
        a.counter_test = "   ".into(); // an empty/whitespace counter-test is a vacuous binding

        // when: that ground is guarded with no real counter-test
        let e = run(&p, a);

        // then: it errors — no vacuous binding on the guard path
        assert!(e.is_err());
    }

    #[test]
    fn guard_should_refuse_the_target_when_the_ground_is_human_rechecked() {
        // given: a HEAD tick whose "team ok" ground is a human-rechecked (person) check
        let (p, id) = repo_with_unbound();

        // when: that person ground is guarded with a test
        let e = run(&p, args("pytest x", &id, Some("team ok")));

        // then: it is refused
        assert!(e.is_err());
    }

    #[test]
    fn guard_should_require_a_target_when_more_than_one_ground_is_unbound() {
        // given: a HEAD tick with two unbound grounds and no target named
        let (p, _id) = repo_with_unbound();
        let t2 = crate::capture::run(
            &p,
            Some("d2"),
            &["--assume", "a", "--assume", "b", "--blame", "Wang Yu"]
                .iter()
                .map(|x| x.to_string())
                .collect::<Vec<_>>(),
        )
        .unwrap();

        // when: the guard is run without naming a target
        let e = run(&p, args("pytest x", &t2.id, None));

        // then: it is refused
        assert!(e.is_err());
    }

    #[test]
    fn guard_should_refuse_the_target_when_it_is_not_head() {
        // given: two decisions in a chain, so the first is no longer HEAD
        let p = repo_with_unbound().0;
        let t1 = crate::capture::run(
            &p,
            Some("d1"),
            &["--assume", "a", "--blame", "Wang Yu"]
                .iter()
                .map(|x| x.to_string())
                .collect::<Vec<_>>(),
        )
        .unwrap();
        let _t2 = crate::capture::run(
            &p,
            Some("d2"),
            &["--assume", "b", "--blame", "Wang Yu"]
                .iter()
                .map(|x| x.to_string())
                .collect::<Vec<_>>(),
        )
        .unwrap();

        // when: the non-HEAD first tick is guarded
        let e = run(&p, args("pytest x", &t1.id, Some("a")));

        // then: it is refused
        assert!(e.is_err());
    }

    #[test]
    fn guard_should_refuse_a_rejected_road_tripwire_when_authority_is_absent() {
        // given: a HEAD tick whose only ground is a rejected road
        let p = repo_with_unbound().0;
        let t = crate::capture::run(
            &p,
            Some("d"),
            &["--reject", "x: y", "--blame", "Wang Yu"]
                .iter()
                .map(|x| x.to_string())
                .collect::<Vec<_>>(),
        )
        .unwrap();

        // when: that rejected ground is guarded with a test but NO --authority user-ruled
        let e = run(&p, args("pytest x", &t.id, Some("y")));

        // then: it is refused — a rejected-road tripwire needs --authority user-ruled
        assert!(e.is_err());
    }

    #[test]
    fn guard_should_bind_a_tripwire_to_a_rejected_road_when_authority_is_user_ruled() {
        // given: a HEAD tick whose only ground is a rejected road (closed by a human ruling)
        let p = repo_with_unbound().0;
        let t = crate::capture::run(
            &p,
            Some("d"),
            &["--reject", "x: y", "--blame", "Wang Yu"]
                .iter()
                .map(|x| x.to_string())
                .collect::<Vec<_>>(),
        )
        .unwrap();

        // when: that rejected ground is guarded with a falsifiable tripwire AND --authority user-ruled
        let mut a = args("pytest x", &t.id, Some("y"));
        a.authority = Some("user-ruled".into());
        let child = run(&p, a).expect("a user-ruled rejected-road tripwire binds");

        // then: a child is written and the closed road now carries a test tripwire
        assert_eq!(child.parent_id, t.id);
        let g = child
            .grounds
            .iter()
            .find(|g| g.supports.starts_with("rejected:"))
            .expect("a rejected road");
        assert!(matches!(g.check, Some(Check::Test { .. })));
        // and the child is a fresh human act (provenance human-now), so it can gate
        assert_eq!(child.provenance, None);
    }
}
