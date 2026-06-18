//! Dogfood — the Gateway **M1 case1 "0-Redis clean win"**, reproduced end-to-end against the real
//! `ev` binary. This is the honest-resurface slice exercised on a real decision, not a synthetic
//! one: it captures the #555 restore-safety decision (DB-backed counter, *reject Redis*), binds the
//! 0-Redis assumption to a **runnable, self-verifying** redis-absence check, and proves the arc that
//! M1 is about — green while redis is absent, then RED the instant redis is injected, with `why`/
//! `reopen` resurfacing the full decision object (decision text + the rejected "Redis" road).
//!
//! Faithful by construction: the decision text, observe, claims, and rejected road mirror the
//! *semantic content* of the frozen `case1` golden vector (`src/cmd.rs::self_test_golden`,
//! id `638c47b0c9dd`); it substitutes a *runnable* check plus a local sha/platform, so the captured
//! tick intentionally hashes to a DIFFERENT id than the golden one — this dogfood proves the M1
//! *arc*, not the frozen hash (golden-id stability is covered by `tests/golden_vectors.rs`). The
//! check is a runnable instantiation of
//! "redis ∉ declared deps" so `ev check --run` genuinely executes it and the injection genuinely
//! flips it — exactly the absence check M1 says `ev` must add (the real codebase has none; that gap
//! is the point). Self-contained and free of proprietary strings: the real material lives local-only under
//! `internal/ev-gateway-dogfood/` (see `case1-dogfood-provenance.md`); this committed fixture
//! distills it and deliberately does NOT copy the real `pyproject.toml`.

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ev() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ev"))
}

// A real git repo (one empty commit, so HEAD resolves for `--run`) + an initialized ev store.
// Returns (repo path, HEAD sha). Hermetic: sets its own git identity, never the developer's.
fn git_repo() -> (std::path::PathBuf, String) {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "ev-dogfood-m1-{}-{}",
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
    assert!(ev()
        .arg("init")
        .current_dir(&p)
        .output()
        .unwrap()
        .status
        .success());
    (p, head)
}

// The decision under test — verbatim from the frozen case1 golden vector.
const DECISION: &str = "restore-safety counter DB-backed; reject Redis";
// The 0-Redis premise the dogfood binds and watches — verbatim from the golden vector.
const ZERO_REDIS_CLAIM: &str = "Argus introduces no Redis; multi-pod coord via existing DB";
// A runnable, self-verifying instantiation of "redis ∉ declared deps": exits 0 (green) when the
// manifest has no redis, non-zero (red) when it does. Its inverse is the injection counter-test.
const REDIS_ABSENT_CHECK: &str = "! grep -q redis pyproject.toml";
const REDIS_INJECTION_COUNTER_TEST: &str = "grep -q redis pyproject.toml";

// Write the project manifest the check reads. `with_redis` toggles the 0-Redis violation.
fn write_pyproject(repo: &std::path::Path, with_redis: bool) {
    let deps = if with_redis {
        "[\"httpx>=0.27,<1.0\", \"sqlalchemy>=2.0,<3.0\", \"redis>=5.0,<6.0\"]"
    } else {
        "[\"httpx>=0.27,<1.0\", \"sqlalchemy>=2.0,<3.0\"]"
    };
    std::fs::write(
        repo.join("pyproject.toml"),
        format!("[project]\nname = \"gateway-restore-safety\"\ndependencies = {deps}\n"),
    )
    .unwrap();
}

// Capture #555: the 0-Redis decision, its human-rechecked sibling, and the rejected Redis road.
// The chosen 0-Redis ground is left UNBOUND here — `guard` binds it next, writing a child tick.
// Returns the decision tick id.
fn decide_555(repo: &std::path::Path) -> String {
    let out = ev()
        .args([
            "decide",
            DECISION,
            "--observe",
            "multi-pod restore-safety counter — chat-room R2289→R2290",
            "--assume",
            ZERO_REDIS_CLAIM,
            "--assume",
            "team still wants 0-Redis posture",
            "--revisit",
            "Q3 infra review",
            "--reject",
            "Redis: would add a new infra dependency",
            "--blame",
            "Wang Yu",
        ])
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "decide failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // "recorded <id> (<n> ground(s))" — the id is the 2nd whitespace token.
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_string()
}

// Bind the 0-Redis assumption to the runnable redis-absence check — a self-verifying Test with its
// injection counter-test and the 3-key liveness contract (platform / triggered-by / surface).
// `guard` writes a NEW CHILD tick (the immutable chain is never edited). Returns the child id.
fn guard_redis_check(repo: &std::path::Path, parent_id: &str, head: &str) -> String {
    let out = ev()
        .args([
            "guard",
            REDIS_ABSENT_CHECK,
            parent_id,
            ZERO_REDIS_CLAIM, // the 0-Redis ground — the only unbound one (ground[1] is person-checked, ground[2] the rejected road)
            "--counter-test",
            REDIS_INJECTION_COUNTER_TEST,
            "--on-platform",
            "local",
            "--triggered-by",
            "pyproject.toml",
            "--surface",
            "pyproject-deps",
            "--verified-at-sha",
            head,
            "--blame",
            "Wang Yu",
        ])
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "guard failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // "bound; wrote child <id>" — the child id is the last whitespace token.
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .last()
        .unwrap()
        .to_string()
}

fn check_run(repo: &std::path::Path, gate: bool) -> std::process::Output {
    let mut args = vec!["check", "--run", "--platform", "local"];
    if gate {
        args.push("--exit-on-red");
    }
    ev().args(&args).current_dir(repo).output().unwrap()
}

// Run a shell command exactly as ev's runner does (`sh -c <cmd>` in the repo). Used to prove the
// check and its counter-test are real logical inverses, independent of ev's evaluation.
fn sh(repo: &std::path::Path, cmd: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .current_dir(repo)
        .status()
        .unwrap()
        .success()
}

#[test]
fn decide_should_capture_the_zero_redis_decision_with_its_rejected_road_when_555_is_recorded() {
    // given: a fresh ev store in a git repo
    let (r, _head) = git_repo();

    // when: decision #555 is recorded — the 0-Redis posture, rejecting Redis
    let id = decide_555(&r);

    // then: an immutable tick carries the decision, the chosen 0-Redis assumption, and the rejected:Redis road
    let raw = std::fs::read_to_string(r.join(".evolving/ticks").join(&id)).unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["decision"], DECISION);
    let grounds = v["grounds"].as_array().unwrap();
    assert!(grounds
        .iter()
        .any(|g| g["supports"] == "chosen" && g["claim"] == ZERO_REDIS_CLAIM));
    assert!(grounds.iter().any(|g| g["supports"] == "rejected:Redis"));
}

#[test]
fn check_should_report_green_when_redis_is_absent_and_the_bound_check_runs() {
    // given: #555 captured, the 0-Redis assumption bound to a runnable redis-absence check, and a redis-free manifest
    let (r, head) = git_repo();
    let parent = decide_555(&r);
    guard_redis_check(&r, &parent, &head);
    write_pyproject(&r, false);

    // when: check actually runs the bound check on this platform
    let out = check_run(&r, true);

    // then: it ran, saw no redis, and reports green — green for the right reason, the gate passes
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    // the bound row's verdict label is exactly "green" (row shape: "<label>\t<file>\t...")
    assert!(String::from_utf8_lossy(&out.stdout)
        .lines()
        .any(|l| l.starts_with("green\t")));
}

#[test]
fn check_should_resurface_and_gate_the_decision_when_redis_is_injected() {
    // given: the bound 0-Redis decision, green while redis is absent (the clean-win baseline)
    let (r, head) = git_repo();
    let parent = decide_555(&r);
    let child = guard_redis_check(&r, &parent, &head);
    write_pyproject(&r, false);
    let baseline = check_run(&r, true);
    assert!(
        baseline.status.success(),
        "baseline not green: {}",
        String::from_utf8_lossy(&baseline.stdout)
    );

    // when: redis is injected into the manifest and check re-evaluates the bound assumption
    write_pyproject(&r, true);
    let out = check_run(&r, true);

    // then: the bound check flips red and the gate fails
    assert!(!out.status.success());
    // exactly Verdict::Red (row prefix "red\t"), not a gray->red or any other non-green state
    assert!(String::from_utf8_lossy(&out.stdout)
        .lines()
        .any(|l| l.starts_with("red\t")));

    // and: `why` and `reopen` resurface the FULL decision object — decision + rejected Redis road
    let why = ev()
        .args(["why", REDIS_ABSENT_CHECK])
        .current_dir(&r)
        .output()
        .unwrap();
    assert!(why.status.success());
    assert!(String::from_utf8_lossy(&why.stdout).contains(DECISION));

    let reopen = ev()
        .args(["reopen", &child])
        .current_dir(&r)
        .output()
        .unwrap();
    assert!(reopen.status.success());
    let reopened = String::from_utf8_lossy(&reopen.stdout);
    assert!(reopened.contains(DECISION)); // the decision comes back, not just the red check
    assert!(reopened.contains("rejected:Redis")); // the road-not-taken resurfaces with it
    assert!(reopened.contains("now: red")); // resurfaced because the bound check is actually red
}

#[test]
fn check_should_write_the_red_verdict_to_the_state_contract_when_the_decision_resurfaces() {
    // given: the 0-Redis decision bound, with redis injected
    let (r, head) = git_repo();
    let parent = decide_555(&r);
    let child = guard_redis_check(&r, &parent, &head);
    write_pyproject(&r, true);

    // when: check evaluates and writes the per-tick state read-contract (no gate, so it exits 0)
    let out = check_run(&r, false);
    assert!(out.status.success());

    // then: results/state/<child>.json records the bound ground's verdict as red for a hook to read
    //       WITHOUT shelling `ev check`
    let raw = std::fs::read_to_string(
        r.join(".evolving/results/state")
            .join(format!("{child}.json")),
    )
    .unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["tick_id"], child);
    let bound = v["grounds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["ref"] == REDIS_ABSENT_CHECK)
        .unwrap();
    assert_eq!(bound["check"], "test");
    assert_eq!(bound["verdict"], "red");
}

#[test]
fn the_redis_check_should_be_a_falsifiable_inverse_of_its_counter_test_when_run_on_each_manifest() {
    // given: a repo whose manifest toggles the 0-Redis violation
    let (r, _head) = git_repo();

    // when: the bound check and its injection counter-test are each run (as ev's runner does) against
    //       a redis-free manifest
    write_pyproject(&r, false);
    let (check_clean, counter_clean) = (
        sh(&r, REDIS_ABSENT_CHECK),
        sh(&r, REDIS_INJECTION_COUNTER_TEST),
    );
    // and when: against a redis-injected manifest
    write_pyproject(&r, true);
    let (check_injected, counter_injected) = (
        sh(&r, REDIS_ABSENT_CHECK),
        sh(&r, REDIS_INJECTION_COUNTER_TEST),
    );

    // then: the check passes iff redis is absent and the counter-test fires iff redis is present —
    //       they are true inverses, so the binding can actually go red (M1's "falsifiable, no
    //       false-green" clause, machine-checked rather than taken on the author's word)
    assert!(check_clean && !check_injected);
    assert!(!counter_clean && counter_injected);
}
