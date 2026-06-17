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
fn resolve_blame(repo: &Path, blame_override: Option<String>) -> Result<String, String> {
    if let Some(b) = blame_override {
        if b.trim().is_empty() { return Err("--blame must be non-empty".into()); }
        return Ok(b);
    }
    let out = Command::new("git").arg("config").arg("user.name").current_dir(repo).output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if name.is_empty() {
        return Err("no author: pass --blame, or set git config user.name".into());
    }
    Ok(name)
}

/// Build a Ground from a draft. (Test bindings added in P2.2 — here a draft with a
/// test_ref/liveness is rejected so the parser stays honest until then.)
fn build_ground(d: DraftGround, _sha: &Option<String>) -> Result<Ground, String> {
    if d.claim.is_empty() {
        return Err("ground claim is empty".into());
    }
    if d.revisit.is_some() && d.test_ref.is_some() {
        return Err("a ground cannot be both --revisit and --assume-test (R2)".into());
    }
    if d.test_ref.is_some() || d.counter_test.is_some()
        || !d.platforms.is_empty() || !d.triggered_by.is_empty() || !d.surfaces.is_empty()
    {
        return Err("test bindings are not supported yet (P2.2)".into());
    }
    let check = d.revisit.map(|when| Check::Person { reference: when });
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
        grounds.push(build_ground(d, &sha_override)?);
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
}
