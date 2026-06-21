//! Dogfood: Tier-1A drift-defense on a customer-facing SSOT ruling, end-to-end against the real
//! `ev` binary. The archetype: a settled policy ("no free tier — the trial window is 3 days") that
//! keeps getting re-derived/regressed, defended by binding the team's OWN invariant test to the
//! ruling so a regression (pushing the window back to a long free trial) flips red and gates.
//!
//! Self-contained: a synthetic `pricing.py` with a `TRIAL_DAYS` constant stands in for the real
//! customer-facing SSOT; the invariant test is a structural `grep` of that constant. No proprietary
//! content. (The mapping to the real flagship ruling + its `test_invariant_*` lives local-only in
//! internal/.) The HONESTY scope: ev catches a STRUCTURAL regression (one that touches the file the
//! test greps); a prose-only re-derivation with no such change is the #1194-class MISS — not caught.
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ev() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ev"))
}

const POLICY_HELD: &str = "TRIAL_DAYS = 3   # no free tier: 3-day trial then hard-deny\n";
const POLICY_REGRESSED: &str = "TRIAL_DAYS = 90   # regressed: re-introduced the long free trial\n";
// single-quoted fixed-string greps: shell-correct AND safe to embed verbatim in the intake JSON
// (no double-quotes to escape).
const CHECK: &str = "grep -qF 'TRIAL_DAYS = 3' pricing.py";
// the #1394 counter-factual ("regress back to 90") IS the natural counter-test: it reads the
// OPPOSITE of the check, so the binding is provably falsifiable.
const COUNTER: &str = "grep -qF 'TRIAL_DAYS = 90' pricing.py";

/// A git repo (one commit) with an ev store and the SSOT file in its policy-HELD state; → (path, HEAD).
fn repo_with_policy() -> (std::path::PathBuf, String) {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "ev-trialdrift-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    std::fs::write(p.join("pricing.py"), POLICY_HELD).unwrap();
    for args in [
        ["init"].as_slice(),
        ["config", "user.email", "t@e.st"].as_slice(),
        ["config", "user.name", "Tester"].as_slice(),
        ["add", "-A"].as_slice(),
        ["commit", "-m", "pricing: trial window = 3 days"].as_slice(),
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
    assert!(ev()
        .arg("init")
        .current_dir(&p)
        .output()
        .unwrap()
        .status
        .success());
    (p, head)
}

/// Capture the ruling with a PROVEN binding: the invariant as the check, the #1394 counter-factual
/// as the counter-test, plus the closed road. Returns the tick id.
fn decide_proven(repo: &std::path::Path, head: &str) -> String {
    let out = ev()
        .args([
            "decide",
            "no free tier: trial window stays 3 days",
            "--assume",
            "the trial window is ruled at 3 days (no free tier)",
            "--assume-test",
            CHECK,
            "--counter-test",
            COUNTER,
            "--on-platform",
            "local",
            "--triggered-by",
            "pricing.py",
            "--surface",
            "pricing",
            "--reject",
            "90-day trial / free tier: re-deriving the policy we ruled out",
            "--verified-at-sha",
            head,
            "--authority",
            "user-ruled",
            "--blame",
            "Wang Yu",
        ])
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "decide: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_string()
}

fn check_run(repo: &std::path::Path) -> std::process::Output {
    ev().args(["check", "--run", "--platform", "local", "--exit-on-red"])
        .current_dir(repo)
        .output()
        .unwrap()
}

fn regress(repo: &std::path::Path) {
    std::fs::write(repo.join("pricing.py"), POLICY_REGRESSED).unwrap();
}

#[test]
fn the_ruling_should_be_proven_green_while_the_trial_window_policy_holds() {
    // given: the ruling captured with a proven binding, policy held (trial = 3)
    let (r, head) = repo_with_policy();
    decide_proven(&r, &head);

    // when: the gate runs
    let out = check_run(&r);

    // then: green and passing — the invariant holds and the counter-test proves it can flip
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "should pass while policy holds; stdout: {stdout}"
    );
    assert!(
        stdout.lines().any(|l| l.starts_with("green\t")),
        "stdout: {stdout}"
    );
}

#[test]
fn a_fresh_agent_boot_read_should_surface_the_settled_trial_window_ruling() {
    // given: the captured ruling
    let (r, head) = repo_with_policy();
    decide_proven(&r, &head);

    // when: a fresh agent runs the boot-read
    let out = ev().arg("brief").current_dir(&r).output().unwrap();

    // then: it sees the settled ruling and the road it closed BEFORE re-deriving it (事前)
    let b = String::from_utf8_lossy(&out.stdout);
    assert!(
        b.contains("no free tier: trial window stays 3 days"),
        "stdout: {b}"
    );
    assert!(
        b.contains("90-day trial") || b.contains("free tier"),
        "stdout: {b}"
    );
}

#[test]
fn regressing_the_trial_window_should_flip_the_check_red_and_gate() {
    // given: the proven ruling, green while the policy holds
    let (r, head) = repo_with_policy();
    decide_proven(&r, &head);
    assert!(
        check_run(&r).status.success(),
        "precondition: green while held"
    );

    // when: a #1394-style regression pushes the window back to 90
    regress(&r);
    let out = check_run(&r);

    // then: the bound invariant flips RED and --exit-on-red gates the regression at commit time (事后)
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !out.status.success(),
        "the regression must gate; stdout: {stdout}"
    );
    assert!(
        stdout.lines().any(|l| l.starts_with("red\t")),
        "stdout: {stdout}"
    );
}

#[test]
fn reopening_should_resurface_the_named_ruling_and_its_closed_road() {
    // given: a captured ruling
    let (r, head) = repo_with_policy();
    let id = decide_proven(&r, &head);

    // when: an operator reopens it
    let out = ev().args(["reopen", &id]).current_dir(&r).output().unwrap();

    // then: ev resurfaces the WHOLE named ruling — the decision, the closed road, and the live verdict
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success());
    assert!(
        s.contains("no free tier: trial window stays 3 days"),
        "stdout: {s}"
    );
    assert!(
        s.contains("rejected:90-day trial / free tier"),
        "stdout: {s}"
    );
}

#[test]
fn the_invariant_and_its_counter_factual_should_be_true_inverses_independent_of_ev() {
    // given: the policy-held substrate. The falsifiability claim — the check and the #1394
    // counter-factual are genuine inverses — is proven here WITHOUT ev, so it is real, not an
    // artifact of how ev runs them.
    let (r, _head) = repo_with_policy();
    let sh = |cmd: &str| {
        Command::new("sh")
            .args(["-c", cmd])
            .current_dir(&r)
            .status()
            .unwrap()
            .success()
    };

    // when/then: while the policy HOLDS, the invariant passes and the regression-counter fails
    assert!(sh(CHECK), "invariant green while policy holds");
    assert!(
        !sh(COUNTER),
        "the #1394 counter-factual is red while policy holds"
    );

    // and: after the regression they invert — the invariant fails, the counter-factual passes
    regress(&r);
    assert!(!sh(CHECK), "invariant red after regression");
    assert!(
        sh(COUNTER),
        "the #1394 counter-factual is green after regression"
    );
}

#[test]
fn a_harvested_invariant_should_still_gate_the_regression_but_flag_unproven_debt() {
    // given: the realistic adoption path — the ruling imported via the canonical intake, ADOPTING
    // the team's existing invariant as a HARVESTED check (no counter-test; provenance=imported).
    // This is the roadmap's headline: drift-defense by harvesting an existing test_invariant_*.
    let (r, head) = repo_with_policy();
    let intake = format!(
        "{{\"kind\":\"ev-decision-intake\",\"decision\":\"no free tier: trial window stays 3 days (harvested)\",\
\"grounds\":[{{\"claim\":\"the trial window is ruled at 3 days\",\"supports\":\"chosen\",\
\"check\":{{\"by\":\"test\",\"ref\":\"{CHECK}\",\"verified_at_sha\":\"{head}\",\
\"liveness\":{{\"platforms\":[\"local\"],\"triggered_by\":[\"pricing.py\"],\"surfaces\":[\"pricing\"]}}}}}}],\
\"blame\":\"Wang Yu\",\"authority\":\"user-ruled\",\"provenance\":\"imported\",\"source_ref\":\"R-trial-harvest\"}}\n"
    );
    let path = r.join("intake.jsonl");
    std::fs::write(&path, intake).unwrap();
    assert!(
        ev().args([
            "migrate",
            "--source",
            &format!("canonical:{}", path.display())
        ])
        .current_dir(&r)
        .output()
        .unwrap()
        .status
        .success(),
        "the harvested ruling imports"
    );

    // when: the policy regresses and the gate runs
    regress(&r);
    let out = check_run(&r);

    // then: the harvested check still gates the regression (a harvested binding gates on its own red),
    // AND the output flags the unproven debt honestly — a harvested green is never presented as proven
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !out.status.success(),
        "a harvested invariant still gates a regression; stdout: {stdout}"
    );
    assert!(stdout.contains("red"), "stdout: {stdout}");
    assert!(
        stdout.contains("harvested") || stdout.contains("falsifiability not proven"),
        "the harvested falsifiability debt must be surfaced; stdout: {stdout}"
    );
}
