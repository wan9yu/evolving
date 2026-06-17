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
