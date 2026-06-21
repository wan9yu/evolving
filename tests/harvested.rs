//! Harvested test bindings: a migrate-only `Check::Test { counter_test: None }` carries FULL
//! liveness but no proof of falsifiability. `ev check` must surface that debt HONESTLY — annotate
//! the row, print a trailing debt line, and append a `harvested` event — while the verdict engine
//! reads it like any other binding: a passing harvested test is Green (pass-green policy, NOT
//! blocked), a failing one GATES on a real red (NOT Unproven, NOT a false-red). There is NO CLI
//! path that builds a harvested binding in 0.1.1 (that is `ev migrate`, a later task), so these
//! tests build the binding through the library constructor and drive the real `ev` binary over it.

use ev::store::Store;
use ev::tick::{Ground, Tick};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ev() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ev"))
}

/// The verdict label of the single check row: rows are `{label}\t{file}\t{claim}\t({detail})`, so the
/// label is the first tab-delimited token of the one line that carries tabs (debt/note lines have
/// none). Lets a test assert on the verdict itself rather than on a substring of the whole output.
fn row_label(stdout: &str) -> Option<&str> {
    stdout
        .lines()
        .find(|l| l.contains('\t'))
        .and_then(|l| l.split('\t').next())
}

/// A git repo (one empty commit) with an ev store; returns (path, HEAD-sha). The harvested binding
/// is verified-at that HEAD so it is never sha-stale during the run.
fn git_repo() -> (std::path::PathBuf, String) {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "ev-harvested-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    for args in [
        ["init"].as_slice(),
        ["config", "user.email", "t@e.st"].as_slice(),
        ["config", "user.name", "Tester"].as_slice(),
        ["commit", "--allow-empty", "-m", "init"].as_slice(),
    ] {
        Command::new("git")
            .args(args)
            .current_dir(&p)
            .output()
            .unwrap();
    }
    let head = String::from_utf8(
        Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&p)
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string();
    Store::at(&p).init().unwrap();
    (p, head)
}

/// Write a genesis tick whose lone chosen ground carries a HARVESTED test binding: `reference` runs
/// on platform "local" verified at `sha`, with full liveness and counter_test None. Returns its id.
fn write_harvested(repo: &std::path::Path, reference: &str, sha: &str) -> String {
    let store = Store::at(repo);
    let check = ev::capture::harvested_test_check(
        reference.into(),
        sha.into(),
        vec!["local".into()],
        vec!["f".into()],
        vec!["s".into()],
    )
    .expect("full liveness present → well-formed harvested binding");
    let mut t = Tick {
        id: String::new(),
        parent_id: String::new(),
        observe: "migrated from gitlog".into(),
        decision: "no Redis".into(),
        grounds: vec![Ground {
            claim: "Argus introduces no Redis".into(),
            supports: "chosen".into(),
            check: Some(check),
        }],
        status: "live".into(),
        held_since: "".into(),
        blame: "Wang Yu".into(),
        authority: None,
        jurisdiction: None,
        source_ref: None,
        provenance: None,
        corrects: None,
    };
    t.id = ev::canonical::compute_id(&t);
    store.write_tick(&t).unwrap();
    t.id
}

#[test]
fn check_should_annotate_a_harvested_row_when_counter_test_is_absent() {
    // given: a store with one harvested test binding (counter_test None, full liveness)
    let (r, head) = git_repo();
    let id = write_harvested(&r, "true", &head);

    // when: check evaluates (no --run; the binding is not-run but still a harvested row)
    let out = ev().arg("check").current_dir(&r).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);

    // then: the row is annotated as harvested with falsifiability-not-proven, and a trailing debt
    // line counts the unproven bindings and points at `ev guard` to add a counter-test
    assert!(
        stdout.contains("harvested — falsifiability not proven"),
        "row not annotated as harvested: {stdout}"
    );
    assert!(
        stdout.contains("harvested-unproven: 1 of 1 test bindings have no counter-test (run ev guard to add one)"),
        "missing/incorrect debt line: {stdout}"
    );
    // and: a `harvested` op event is appended for it
    let log = std::fs::read_to_string(r.join(".evolving/results/events.jsonl")).unwrap();
    assert!(
        log.lines()
            .any(|l| l.contains("\"op\":\"harvested\"") && l.contains(&id)),
        "no harvested event for {id}: {log}"
    );
}

#[test]
fn check_should_read_a_passing_harvested_test_green_when_run() {
    // given: a harvested binding whose bound test passes (pass-green policy — NOT blocked despite
    // having no proven counter-test)
    let (r, head) = git_repo();
    write_harvested(&r, "true", &head);

    // when: check --run --platform local runs the harvested test and gates
    let out = ev()
        .args(["check", "--run", "--platform", "local", "--exit-on-red"])
        .current_dir(&r)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);

    // then: it reads green (NOT unproven), the gate passes, and the harvested debt is still surfaced.
    // The verdict label is the first tab-delimited token of the row — assert on that, NOT on the
    // substring "unproven" (the debt line legitimately contains "harvested-unproven:").
    assert!(
        out.status.success(),
        "a passing harvested test must not gate: {stdout}"
    );
    assert!(
        row_label(&stdout) == Some("green"),
        "expected a green verdict label: {stdout}"
    );
    assert!(
        stdout.contains("harvested — falsifiability not proven"),
        "harvested debt must stay visible even when green: {stdout}"
    );
}

#[test]
fn check_should_gate_on_a_real_red_harvested_test_when_run() {
    // given: a harvested binding whose bound test fails (a genuine red, not a missing counter-test)
    let (r, head) = git_repo();
    write_harvested(&r, "false", &head);

    // when: check --run --platform local runs the harvested test and gates
    let out = ev()
        .args(["check", "--run", "--platform", "local", "--exit-on-red"])
        .current_dir(&r)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);

    // then: it gates on the real red (NOT unproven, NOT a false-red) — harvesting drops the proof,
    // never the gate on a failing test. Assert on the verdict label token, not the "unproven"
    // substring (the debt line legitimately carries "harvested-unproven:").
    assert!(
        !out.status.success(),
        "a red harvested test must gate: {stdout}"
    );
    assert!(
        row_label(&stdout) == Some("red"),
        "expected a red verdict label, not unproven: {stdout}"
    );
}
