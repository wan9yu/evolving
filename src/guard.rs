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
}

fn resolve_target(grounds: &[Ground], target: &Option<String>) -> Result<usize, String> {
    let unbound: Vec<usize> = grounds.iter().enumerate().filter(|(_, g)| g.check.is_none()).map(|(i, _)| i).collect();
    match target {
        None => match unbound.as_slice() {
            [one] => Ok(*one),
            [] => Err("no unbound ground to guard".into()),
            _ => Err("more than one unbound ground — name the target (claim or index)".into()),
        },
        Some(t) => {
            if let Ok(idx) = t.parse::<usize>() {
                if idx < grounds.len() { return Ok(idx); }
                return Err(format!("ground index {idx} out of range"));
            }
            let matches: Vec<usize> = grounds.iter().enumerate().filter(|(_, g)| g.claim == *t).map(|(i, _)| i).collect();
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
    let parent = store.read_tick(&a.id).map_err(|e| format!("{e}"))?
        .ok_or(format!("no tick with id {}", a.id))?;
    let idx = resolve_target(&parent.grounds, &a.target)?;
    let g = &parent.grounds[idx];
    // R2: a human-rechecked (person) ground can never be force-bound to a test.
    if let Some(Check::Person { .. }) = g.check {
        return Err("a human-rechecked ground cannot carry a test (R2 hard error)".into());
    }
    if g.check.is_some() {
        return Err("ground already has a check".into());
    }
    if a.counter_test.trim().is_empty() {
        return Err("a test binding requires a counter-test (no vacuous binding)".into());
    }
    if a.platforms.is_empty() || a.triggered_by.is_empty() || a.surfaces.is_empty() {
        return Err("a test binding requires at least one platform, triggered-by, and surface".into());
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
            counter_test: a.counter_test,
            liveness: Liveness { platforms: a.platforms, triggered_by: a.triggered_by, surfaces: a.surfaces },
        }),
    };
    let mut child = Tick {
        id: String::new(),
        parent_id: parent.id.clone(),
        observe: parent.observe.clone(),
        decision: parent.decision.clone(),
        grounds,
        status: "live".into(),
        held_since: String::new(),
        blame,
    };
    child.id = compute_id(&child);
    store.write_tick(&child).map_err(|e| format!("writing tick: {e}"))?;
    Ok(child)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_with_unbound() -> (std::path::PathBuf, String) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!("ev-guard-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Store::at(&p).init().unwrap();
        let args: Vec<String> = ["--assume", "schema stays frozen", "--assume", "team ok", "--revisit", "Q3", "--blame", "Wang Yu"]
            .iter().map(|x| x.to_string()).collect();
        let t = crate::capture::run(&p, "build our own retrieval", &args).unwrap();
        (p, t.id)
    }
    fn args(selector: &str, id: &str, target: Option<&str>) -> GuardArgs {
        GuardArgs {
            selector: selector.into(), id: id.into(), target: target.map(|s| s.into()),
            counter_test: "pytest x::counter".into(),
            platforms: vec!["linux-ci".into()], triggered_by: vec!["f".into()], surfaces: vec!["s".into()],
            verified_at_sha: Some("d308afac1b2c3d4e5f60718293a4b5c6d7e8f901".into()),
            blame: Some("Wang Yu".into()),
        }
    }

    #[test]
    fn guard_binds_a_named_unbound_ground_and_writes_a_child() {
        let (p, id) = repo_with_unbound();
        let child = run(&p, args("pytest tests/test_schema_frozen.py", &id, Some("schema stays frozen"))).expect("ok");
        assert_eq!(child.parent_id, id);
        let i = child.grounds.iter().position(|g| g.claim == "schema stays frozen").unwrap();
        assert!(matches!(child.grounds[i].check, Some(Check::Test { .. })));
    }

    #[test]
    fn guard_refuses_to_force_bind_a_human_rechecked_ground() {
        let (p, id) = repo_with_unbound();
        let e = run(&p, args("pytest x", &id, Some("team ok")));
        assert!(e.is_err());
    }

    #[test]
    fn guard_requires_a_target_when_more_than_one_ground_is_unbound() {
        let (p, _id) = repo_with_unbound();
        let t2 = crate::capture::run(&p, "d2", &["--assume", "a", "--assume", "b", "--blame", "Wang Yu"].iter().map(|x| x.to_string()).collect::<Vec<_>>()).unwrap();
        let e = run(&p, args("pytest x", &t2.id, None));
        assert!(e.is_err());
    }
}
