use crate::canonical::compute_id;
use crate::store::Store;
use crate::tick::{Check, Ground, Liveness, Tick};
use crate::verify::verify;
use std::path::Path;
use std::process::ExitCode;

/// Whether a triggering change landed after this ground's most recent run. Uses the latest
/// receipt's commit + the binding's triggered_by paths. False when there is no receipt, no
/// Test binding, or git can't tell (None ⇒ not evaluated).
fn triggered_since(
    repo: &std::path::Path,
    ground: &crate::tick::Ground,
    receipts: &[crate::receipt::Receipt],
) -> bool {
    use crate::tick::Check;
    let triggered_by = match &ground.check {
        Some(Check::Test { liveness, .. }) => &liveness.triggered_by,
        _ => return false,
    };
    let latest = receipts.iter().max_by(|a, b| a.ran_at.cmp(&b.ran_at));
    match latest {
        Some(r) => crate::liveness::changed_since(repo, &r.commit, triggered_by).unwrap_or(false),
        None => false,
    }
}

pub fn init(repo: &Path) -> ExitCode {
    let store = Store::at(repo);
    match store.init() {
        Ok(true) => {
            println!("created .evolving/  (content-addressed chain + results cache)");
            ExitCode::SUCCESS
        }
        Ok(false) => {
            println!(".evolving/ already exists (no-op)");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: could not create .evolving/: {e}");
            ExitCode::FAILURE
        }
    }
}
pub fn show(repo: &Path, id: &str) -> ExitCode {
    let store = Store::at(repo);
    let path = store.ticks_dir().join(id);
    if !path.is_file() {
        eprintln!("error: no tick with id {id}");
        return ExitCode::FAILURE;
    }
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            // print as-is (the on-disk pretty JSON: hashed payload + bookkeeping).
            println!("{text}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: reading {id}: {e}");
            ExitCode::FAILURE
        }
    }
}
pub fn decide(repo: &Path, decision: &str, args: &[String]) -> ExitCode {
    match crate::capture::run(repo, decision, args) {
        Ok(t) => {
            println!("recorded {} ({} ground(s))", t.id, t.grounds.len());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

pub fn guard(repo: &Path, a: crate::guard::GuardArgs) -> ExitCode {
    match crate::guard::run(repo, a) {
        Ok(t) => {
            println!("bound; wrote child {}", t.id);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

pub fn verify_cmd(repo: &Path, self_test: bool) -> ExitCode {
    if self_test {
        return self_test_golden();
    }
    let store = Store::at(repo);
    match verify(&store) {
        Ok(v) if v.is_empty() => {
            println!("✓ chain intact: every id == hash(payload), lineage forward-only");
            println!("✓ every tick validates against the closed schema (R1) and check shape (R2)");
            ExitCode::SUCCESS
        }
        Ok(v) => {
            for line in &v {
                println!("✗ {line}");
            }
            eprintln!("{} violation(s)", v.len());
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("error: reading store: {e}");
            ExitCode::FAILURE
        }
    }
}

/// The latest ran_at across a ref's receipts (for the display line), if any.
/// `receipts` is already scoped to one ref by `read_for`, so no filtering is needed.
fn latest_ran_at(receipts: &[crate::receipt::Receipt]) -> Option<String> {
    receipts.iter().map(|r| r.ran_at.clone()).max()
}

/// The evaluation context for one `ev check` / `ev reopen` invocation: the staleness reference
/// (resolved per policy by the caller), the selected-list, the wall clock, and the staleness
/// window. The I/O assembly lives here in the command layer so `verdict::verdict_for` stays pure.
fn live_ctx(
    store: &Store,
    staleness_days: u64,
    live_origin_sha: Option<String>,
    attest: Option<Vec<String>>,
) -> crate::verdict::Ctx {
    crate::verdict::Ctx {
        live_origin_sha,
        selected: crate::selected::read(store).unwrap_or(None),
        now_unix: time::OffsetDateTime::now_utc().unix_timestamp(),
        staleness_secs: staleness_days as i64 * 86_400,
        attest,
    }
}

pub fn check(
    repo: &Path,
    exit_on_red: bool,
    run: bool,
    platform: &str,
    offline: bool,
    attest: Vec<String>,
) -> ExitCode {
    use crate::verdict::{verdict_for, Verdict};
    let store = Store::at(repo);
    if !store.exists() {
        eprintln!("error: no .evolving/ store here — run `ev init` first");
        return ExitCode::FAILURE;
    }
    let files = match store.read_all() {
        Ok(f) => f,
        Err(e) => {
            eprintln!("error: reading store: {e}");
            return ExitCode::FAILURE;
        }
    };
    let config = crate::config::read(&store);

    // --run pass: for every live Test-bound ground that declares this platform, run the
    // bound ref locally and append a receipt for it (one local run = one platform receipt).
    if run {
        for (_filename, raw) in &files {
            let t = match crate::tick::from_value(raw) {
                Ok(t) => t,
                Err(_) => continue,
            };
            if t.status != "live" {
                continue;
            }
            for g in &t.grounds {
                if let Some(Check::Test {
                    reference,
                    liveness,
                    ..
                }) = &g.check
                {
                    if liveness.platforms.iter().any(|p| p == platform) {
                        match crate::runner::run_check(
                            repo,
                            reference,
                            platform,
                            config.green_exit_code,
                        ) {
                            Ok(rc) => {
                                if let Err(e) = crate::receipt::append(&store, &rc) {
                                    eprintln!(
                                        "warning: could not write receipt for {reference:?}: {e}"
                                    );
                                }
                            }
                            Err(e) => eprintln!("warning: run failed for {reference:?}: {e}"),
                        }
                    }
                }
            }
        }
    }

    let live_origin = crate::staleness::resolve(repo, &store, &config.staleness_ref, offline);
    let attest = if attest.is_empty() {
        None
    } else {
        Some(attest)
    };
    let ctx = live_ctx(&store, config.staleness_days, live_origin, attest);
    let mut rows: Vec<String> = Vec::new();
    let mut any_not_green = false;

    for (filename, raw) in &files {
        let t = match crate::tick::from_value(raw) {
            Ok(t) => t,
            Err(_) => continue, // ev verify owns schema errors; check skips unparsable ticks
        };
        if t.status != "live" {
            continue;
        }
        let mut verdicts = Vec::with_capacity(t.grounds.len());
        for g in &t.grounds {
            // Receipts are read only for Test-bound grounds; person/unbound need none.
            let receipts = match &g.check {
                Some(Check::Test { reference, .. }) => {
                    crate::receipt::read_for(&store, reference).unwrap_or_default()
                }
                _ => Vec::new(),
            };
            // verdict_for returns NotApplicable for any non-Test ground.
            let ts = triggered_since(repo, g, &receipts);
            let v = verdict_for(g, &receipts, &ctx, ts);
            if !matches!(v, Verdict::Green | Verdict::NotApplicable | Verdict::Exempt) {
                any_not_green = true;
            }
            // Only Test-bound grounds appear in the printed set and the gate.
            if matches!(&g.check, Some(Check::Test { .. })) {
                let detail = match &v {
                    Verdict::NotRun { missing_platforms } => {
                        format!("missing: {}", missing_platforms.join(", "))
                    }
                    Verdict::Stale { reason } => reason.clone(),
                    _ => latest_ran_at(&receipts)
                        .map(|ts| format!("ran {ts}"))
                        .unwrap_or_else(|| "no receipt".into()),
                };
                rows.push(format!(
                    "{}\t{filename}\t{:?}\t({detail})",
                    v.label(),
                    g.claim
                ));
            }
            verdicts.push((g, v));
        }
        // The per-host verdict-cache read contract for this tick (a hook reads it without shelling check).
        let _ = crate::state::write_state(
            &store,
            &t.id,
            &verdicts,
            &config.staleness_ref,
            ctx.live_origin_sha.as_deref(),
        );
    }

    if rows.is_empty() {
        println!("no test-bound grounds to check");
    } else {
        for r in &rows {
            println!("{r}");
        }
    }
    if exit_on_red && any_not_green {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

pub fn why(repo: &Path, selector: &str) -> ExitCode {
    let store = Store::at(repo);
    if !store.exists() {
        eprintln!("error: no .evolving/ store here — run `ev init` first");
        return ExitCode::FAILURE;
    }
    let files = match store.read_all() {
        Ok(f) => f,
        Err(e) => {
            eprintln!("error: reading store: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut found = false;
    for (filename, raw) in &files {
        let t = match crate::tick::from_value(raw) {
            Ok(t) => t,
            Err(_) => continue,
        };
        if t.status != "live" {
            continue;
        }
        for g in &t.grounds {
            if let Some(Check::Test { reference, .. }) = &g.check {
                if reference.as_str() == selector {
                    found = true;
                    println!(
                        "{filename}\t{:?}\tguards: {:?} ({})",
                        t.decision, g.claim, g.supports
                    );
                }
            }
        }
    }
    if !found {
        eprintln!("{selector:?} guards nothing");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// List every decision in the ledger: id, status, decision (sorted by id, deterministic).
pub fn list(repo: &Path) -> ExitCode {
    let store = Store::at(repo);
    if !store.exists() {
        eprintln!("error: no .evolving/ store here — run `ev init` first");
        return ExitCode::FAILURE;
    }
    let files = match store.read_all() {
        Ok(f) => f,
        Err(e) => {
            eprintln!("error: reading store: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut rows: Vec<(String, String, String)> = files
        .iter()
        .map(|(name, raw)| match crate::tick::from_value(raw) {
            Ok(t) => (name.clone(), t.status, t.decision),
            Err(_) => (name.clone(), "?".into(), "<unparseable>".into()),
        })
        .collect();
    rows.sort();
    if rows.is_empty() {
        println!("no decisions yet");
        return ExitCode::SUCCESS;
    }
    for (id, status, decision) in &rows {
        println!("{id}\t{status}\t{decision:?}");
    }
    ExitCode::SUCCESS
}

/// Show the decision lineage from HEAD back to genesis (newest first).
pub fn log(repo: &Path) -> ExitCode {
    let store = Store::at(repo);
    if !store.exists() {
        eprintln!("error: no .evolving/ store here — run `ev init` first");
        return ExitCode::FAILURE;
    }
    let mut id = match store.read_head() {
        Ok(h) => h,
        Err(e) => {
            eprintln!("error: reading HEAD: {e}");
            return ExitCode::FAILURE;
        }
    };
    if id.is_empty() {
        println!("no decisions yet");
        return ExitCode::SUCCESS;
    }
    let mut seen = std::collections::HashSet::new();
    while !id.is_empty() {
        if !seen.insert(id.clone()) {
            break; // cycle guard (a content-addressed chain can't cycle, but never loop)
        }
        match store.read_tick(&id) {
            Ok(Some(t)) => {
                println!("{}\t{}\t{:?}", t.id, t.status, t.decision);
                id = t.parent_id;
            }
            Ok(None) => {
                eprintln!("warning: {id} not found (broken lineage)");
                break;
            }
            Err(e) => {
                eprintln!("error: reading {id}: {e}");
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}

pub fn reopen(repo: &Path, id: &str) -> ExitCode {
    let store = Store::at(repo);
    let tick = match store.read_tick(id) {
        Ok(Some(t)) => t,
        Ok(None) => {
            eprintln!("error: no tick with id {id}");
            return ExitCode::FAILURE;
        }
        Err(e) => {
            eprintln!("error: reading {id}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let config = crate::config::read(&store);
    let live_origin = crate::staleness::resolve(repo, &store, &config.staleness_ref, true);
    let ctx = live_ctx(&store, config.staleness_days, live_origin, None);

    println!("decision {}: {:?}", tick.id, tick.decision);
    if !tick.observe.is_empty() {
        println!("observe: {:?}", tick.observe);
    }
    for g in &tick.grounds {
        match &g.check {
            Some(Check::Test {
                reference,
                verified_at_sha,
                ..
            }) => {
                let receipts = crate::receipt::read_for(&store, reference).unwrap_or_default();
                let ts = triggered_since(repo, g, &receipts);
                let v = crate::verdict::verdict_for(g, &receipts, &ctx, ts);
                let now = v.label();
                let short = &verified_at_sha[..verified_at_sha.len().min(8)];
                println!(
                    "  [{}] {:?} — test {:?} frozen@{short} now: {now}",
                    g.supports, g.claim, reference
                );
            }
            Some(Check::Person { reference }) => {
                println!("  [{}] {:?} — person {:?}", g.supports, g.claim, reference);
            }
            None => {
                println!("  [{}] {:?}", g.supports, g.claim);
            }
        }
    }
    ExitCode::SUCCESS
}

/// Reproduce the two frozen golden vectors; non-zero if either id drifts.
fn self_test_golden() -> ExitCode {
    let genesis = Tick {
        id: String::new(),
        parent_id: "".into(),
        observe: "evaluating retrieval backend".into(),
        decision: "freeze the retrieval schema for v2".into(),
        grounds: vec![
            Ground {
                claim: "team still wants a frozen schema".into(),
                supports: "chosen".into(),
                check: Some(Check::Person {
                    reference: "Q3 infra review".into(),
                }),
            },
            Ground {
                claim: "pgvector would lock our schema".into(),
                supports: "rejected:pgvector".into(),
                check: None,
            },
        ],
        status: "live".into(),
        held_since: "".into(),
        blame: "Wang Yu".into(),
    };
    let case1 = Tick {
        id: String::new(),
        parent_id: "7b21f0a4c8de".into(),
        observe: "multi-pod restore-safety counter — chat-room R2289→R2290".into(),
        decision: "restore-safety counter DB-backed; reject Redis".into(),
        grounds: vec![
            Ground {
                claim: "Argus introduces no Redis; multi-pod coord via existing DB".into(),
                supports: "chosen".into(),
                check: Some(Check::Test {
                    reference: "pytest tests/test_redis_absent.py".into(),
                    verified_at_sha: "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901".into(),
                    counter_test:
                        "pytest tests/test_redis_absent.py::test_redis_injection_flips_red".into(),
                    liveness: Liveness {
                        platforms: vec!["linux-ci".into()],
                        triggered_by: vec!["pyproject.toml".into()],
                        surfaces: vec!["pyproject-deps".into()],
                    },
                }),
            },
            Ground {
                claim: "team still wants 0-Redis posture".into(),
                supports: "chosen".into(),
                check: Some(Check::Person {
                    reference: "Q3 infra review".into(),
                }),
            },
            Ground {
                claim: "Redis would add a new infra dependency".into(),
                supports: "rejected:Redis".into(),
                check: None,
            },
        ],
        status: "live".into(),
        held_since: "".into(),
        blame: "Wang Yu".into(),
    };
    let mut ok = true;
    for (name, t, want) in [
        ("genesis", &genesis, "e2b337f53a1f"),
        ("case1", &case1, "638c47b0c9dd"),
    ] {
        let got = compute_id(t);
        let pass = got == want;
        ok &= pass;
        println!(
            "{} {name}: {got} (want {want})",
            if pass { "✓" } else { "✗" }
        );
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
