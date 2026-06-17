//! `ev decide` — walk the trailing args left-to-right into a draft, validate, append a child.
use crate::canonical::compute_id;
use crate::store::Store;
use crate::tick::{Check, Ground, Tick};
use std::path::Path;
use std::process::Command;

#[derive(Default)]
struct DraftGround {
    claim: String,
    supports: String, // "chosen" | "rejected:<opt>"
    revisit: Option<String>,
    test_ref: Option<String>,
    counter_test: Option<String>,
    platforms: Vec<String>,
    triggered_by: Vec<String>,
    surfaces: Vec<String>,
}

fn need(args: &[String], i: usize, flag: &str) -> Result<String, String> {
    args.get(i + 1).cloned().ok_or(format!("{flag} requires a value"))
}

fn last<'a>(g: &'a mut [DraftGround], flag: &str) -> Result<&'a mut DraftGround, String> {
    g.last_mut().ok_or(format!("{flag} has no preceding --assume/--reject ground"))
}

/// Resolve the declared author: --blame, else `git config user.name`.
pub(crate) fn resolve_blame(repo: &Path, blame_override: Option<String>) -> Result<String, String> {
    if let Some(b) = blame_override {
        let b = b.trim();
        if b.is_empty() { return Err("--blame must be non-empty".into()); }
        return Ok(b.to_string());
    }
    let out = Command::new("git").arg("config").arg("user.name").current_dir(repo).output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if name.is_empty() {
        return Err("no author: pass --blame, or set git config user.name".into());
    }
    Ok(name)
}

pub(crate) fn resolve_sha(repo: &Path, sha_override: &Option<String>) -> Result<String, String> {
    let sha = match sha_override {
        Some(s) => s.trim().to_string(),
        None => {
            let out = std::process::Command::new("git").args(["rev-parse", "HEAD"]).current_dir(repo).output()
                .map_err(|e| format!("cannot run git: {e}"))?;
            if !out.status.success() {
                return Err("cannot resolve verified_at_sha (not a git repo?) — pass --verified-at-sha".into());
            }
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        }
    };
    let ok = sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if !ok {
        return Err(format!("verified_at_sha must be 40 lowercase hex: {sha}"));
    }
    Ok(sha)
}

fn t_grounds_text(grounds: &[Ground]) -> Vec<String> {
    grounds.iter().map(|g| g.claim.clone()).collect()
}

fn build_ground(repo: &Path, d: DraftGround, sha_override: &Option<String>) -> Result<Ground, String> {
    use crate::tick::Liveness;
    if d.claim.is_empty() {
        return Err("ground claim is empty".into());
    }
    if d.revisit.is_some() && d.test_ref.is_some() {
        return Err("a ground cannot be both --revisit and --assume-test (R2)".into());
    }
    let has_test_fields = d.counter_test.is_some()
        || !d.platforms.is_empty() || !d.triggered_by.is_empty() || !d.surfaces.is_empty();
    let check = match (d.test_ref, d.revisit) {
        (Some(reference), _) => {
            let counter_test = d.counter_test.ok_or("a test binding requires --counter-test (no vacuous binding)".to_string())?;
            if d.platforms.is_empty() || d.triggered_by.is_empty() || d.surfaces.is_empty() {
                return Err("a test binding requires at least one --on-platform, --triggered-by, and --surface".into());
            }
            let verified_at_sha = resolve_sha(repo, sha_override)?;
            Some(Check::Test {
                reference,
                verified_at_sha,
                counter_test,
                liveness: Liveness { platforms: d.platforms, triggered_by: d.triggered_by, surfaces: d.surfaces },
            })
        }
        (None, Some(when)) => {
            if has_test_fields {
                return Err("--counter-test/--on-platform/--triggered-by/--surface require --assume-test".into());
            }
            Some(Check::Person { reference: when })
        }
        (None, None) => {
            if has_test_fields {
                return Err("--counter-test/--on-platform/--triggered-by/--surface require --assume-test".into());
            }
            None
        }
    };
    Ok(Ground { claim: d.claim, supports: d.supports, check })
}

pub fn run(repo: &Path, decision: &str, args: &[String]) -> Result<Tick, String> {
    if decision.trim().is_empty() {
        return Err("decision text is empty".into());
    }
    let mut observe = String::new();
    let mut blame_override: Option<String> = None;
    let mut sha_override: Option<String> = None;
    let mut drafts: Vec<DraftGround> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let flag = args[i].clone();
        match flag.as_str() {
            "--observe" => { observe = need(args, i, &flag)?; }
            "--blame" => { blame_override = Some(need(args, i, &flag)?); }
            "--verified-at-sha" => { sha_override = Some(need(args, i, &flag)?); }
            "--reject" => {
                let v = need(args, i, &flag)?;
                let (opt, why) = v.split_once(':').ok_or("--reject expects \"<option>: <why>\"".to_string())?;
                let (opt, why) = (opt.trim(), why.trim());
                if opt.is_empty() || why.is_empty() {
                    return Err("--reject needs non-empty <option> and <why>".into());
                }
                drafts.push(DraftGround { claim: why.into(), supports: format!("rejected:{opt}"), ..Default::default() });
            }
            "--assume" => {
                let claim = need(args, i, &flag)?;
                drafts.push(DraftGround { claim, supports: "chosen".into(), ..Default::default() });
            }
            "--revisit" => { last(&mut drafts, &flag)?.revisit = Some(need(args, i, &flag)?); }
            "--assume-test" => { last(&mut drafts, &flag)?.test_ref = Some(need(args, i, &flag)?); }
            "--counter-test" => { last(&mut drafts, &flag)?.counter_test = Some(need(args, i, &flag)?); }
            "--on-platform" => { let v = need(args, i, &flag)?; last(&mut drafts, &flag)?.platforms.push(v); }
            "--triggered-by" => { let v = need(args, i, &flag)?; last(&mut drafts, &flag)?.triggered_by.push(v); }
            "--surface" => { let v = need(args, i, &flag)?; last(&mut drafts, &flag)?.surfaces.push(v); }
            other => return Err(format!("decide: unknown flag {other}")),
        }
        i += 2;
    }
    let blame = resolve_blame(repo, blame_override)?;
    let mut grounds = Vec::new();
    for d in drafts {
        grounds.push(build_ground(repo, d, &sha_override)?);
    }
    for field in std::iter::once(decision.to_string()).chain(std::iter::once(observe.clone()))
        .chain(t_grounds_text(&grounds))
    {
        for verb in crate::lint::r3_self_evolve(&field) {
            eprintln!("warning: \"{verb}\" should take a human subject, not the system (best-effort lint; a re-wording evades it)");
        }
    }
    let store = Store::at(repo);
    if !store.exists() {
        return Err("no .evolving/ store here — run `ev init` first".into());
    }
    let parent_id = store.read_head().map_err(|e| format!("reading HEAD: {e}"))?;
    let mut t = Tick {
        id: String::new(),
        parent_id,
        observe,
        decision: decision.to_string(),
        grounds,
        status: "live".into(),
        held_since: String::new(),
        blame,
    };
    t.id = compute_id(&t);
    store.write_tick(&t).map_err(|e| format!("writing tick: {e}"))?;
    Ok(t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tick::Check;

    fn repo() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!("ev-capture-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Store::at(&p).init().unwrap();
        p
    }
    fn s(v: &[&str]) -> Vec<String> { v.iter().map(|x| x.to_string()).collect() }

    #[test]
    fn decide_records_a_chosen_ground_a_revisit_and_a_rejected_road() {
        let r = repo();
        let t = run(&r, "build our own retrieval; reject pgvector", &s(&[
            "--observe", "evaluating backend",
            "--assume", "team has bandwidth long-term", "--revisit", "Q3 review",
            "--reject", "pgvector: would lock our schema",
            "--blame", "Wang Yu",
        ])).expect("ok");
        assert_eq!(t.grounds.len(), 2);
        assert!(matches!(t.grounds[0].check, Some(Check::Person { .. })));
        assert_eq!(t.grounds[1].supports, "rejected:pgvector");
        assert_eq!(t.blame, "Wang Yu");
        assert_eq!(Store::at(&r).read_head().unwrap(), t.id);
    }

    #[test]
    fn decide_stores_a_padded_blame_trimmed() {
        let r = repo();
        let t = run(&r, "d", &s(&["--assume", "c", "--blame", "  Wang Yu  "])).expect("ok");
        assert_eq!(t.blame, "Wang Yu");
    }

    #[test]
    fn decide_refuses_a_ground_that_is_both_revisit_and_assume_test() {
        let r = repo();
        let e = run(&r, "d", &s(&["--assume", "c", "--revisit", "Q3", "--assume-test", "pytest x"]));
        assert!(e.is_err());
    }

    #[test]
    fn decide_errors_without_a_store() {
        let p = std::env::temp_dir().join(format!("ev-nostore-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        assert!(run(&p, "d", &s(&["--blame", "x"])).is_err());
    }

    #[test]
    fn decide_builds_a_self_verifying_test_binding() {
        let r = repo();
        let t = run(&r, "restore-safety counter DB-backed; reject Redis", &s(&[
            "--assume", "Argus introduces no Redis; multi-pod coord via existing DB",
            "--assume-test", "pytest tests/test_redis_absent.py",
            "--counter-test", "pytest tests/test_redis_absent.py::test_redis_injection_flips_red",
            "--on-platform", "linux-ci", "--triggered-by", "pyproject.toml", "--surface", "pyproject-deps",
            "--verified-at-sha", "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901",
            "--reject", "Redis: a new infra dependency",
        ])).expect("ok");
        match &t.grounds[0].check {
            Some(Check::Test { reference, counter_test, liveness, verified_at_sha }) => {
                assert_eq!(reference, "pytest tests/test_redis_absent.py");
                assert!(counter_test.contains("flips_red"));
                assert_eq!(liveness.platforms, vec!["linux-ci".to_string()]);
                assert_eq!(verified_at_sha.len(), 40);
            }
            _ => panic!("expected a test check"),
        }
    }

    #[test]
    fn decide_rejects_a_test_binding_without_a_counter_test() {
        let r = repo();
        let e = run(&r, "d", &s(&[
            "--assume", "c", "--assume-test", "pytest x",
            "--on-platform", "linux-ci", "--triggered-by", "f", "--surface", "s",
            "--verified-at-sha", "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901",
        ]));
        assert!(e.is_err());
    }

    #[test]
    fn decide_rejects_a_test_binding_with_no_verified_at_sha_and_no_git() {
        let r = repo();
        let e = run(&r, "d", &s(&[
            "--assume", "c", "--assume-test", "pytest x", "--counter-test", "ct",
            "--on-platform", "linux-ci", "--triggered-by", "f", "--surface", "s",
        ]));
        assert!(e.is_err());
    }
}
