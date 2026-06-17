use crate::canonical::compute_id;
use crate::store::Store;
use crate::tick::{Check, Ground, Liveness, Tick};
use crate::verify::verify;
use std::path::Path;
use std::process::ExitCode;

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
fn latest_ran_at(receipts: &[crate::receipt::Receipt], reference: &str) -> Option<String> {
    receipts
        .iter()
        .filter(|r| r.test == reference)
        .map(|r| r.ran_at.clone())
        .max()
}

pub fn check(repo: &Path, exit_on_red: bool, run: bool, platform: &str) -> ExitCode {
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
                        match crate::runner::run_check(repo, reference, platform) {
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

    let origin = store.read_origin_sha();
    let selected = crate::selected::read(&store).unwrap_or(None);
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
        for g in &t.grounds {
            // Only Test-bound grounds appear in check; person re-checks and unbound grounds are excluded.
            let reference = match &g.check {
                Some(Check::Test { reference, .. }) => reference.clone(),
                _ => continue,
            };
            let receipts = crate::receipt::read_for(&store, &reference).unwrap_or_default();
            let v = verdict_for(g, &receipts, origin.as_deref(), selected.as_ref());
            if !matches!(v, Verdict::Green) {
                any_not_green = true;
            }
            let label = match &v {
                Verdict::Green => "green",
                Verdict::Red => "red",
                Verdict::GrayRed => "gray->red",
                Verdict::NotRun { .. } => "not-run",
                Verdict::Stale { .. } => "stale",
                Verdict::SilentlyUnbound => "silently-unbound",
                Verdict::NotApplicable => "n/a",
            };
            let detail = match &v {
                Verdict::NotRun { missing_platforms } => {
                    format!("missing: {}", missing_platforms.join(", "))
                }
                Verdict::Stale { reason } => reason.clone(),
                _ => latest_ran_at(&receipts, &reference)
                    .map(|ts| format!("ran {ts}"))
                    .unwrap_or_else(|| "no receipt".into()),
            };
            rows.push(format!("{label}\t{filename}\t{:?}\t({detail})", g.claim));
        }
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
