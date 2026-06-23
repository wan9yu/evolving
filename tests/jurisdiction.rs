//! Task 2 — the jurisdiction tag + the C/D **detect-only** guarantee, end-to-end against the real
//! `ev` binary. A jurisdiction in {A,B,C,D} is a non-hashed bookkeeping tag (sibling of authority).
//! C/D are detect-only: such a decision NEVER gates. As of 0.1.19 that is enforced BY CONSTRUCTION —
//! `capture::build()` REFUSES to mint a C/D decision that carries a runnable Test, so decide/guard/
//! ratify finally agree with verify/migrate/correct (the door itself says it). LOCK 1 (cmd.rs,
//! gate-time → maps a C/D not-green verdict to the non-gating Memo) remains ONLY as a LEGACY DEFENSE
//! for immutable C/D+Test ticks created before that door existed. This test drives the refusal at the
//! door AND LOCK 1's legacy defense (via a directly-injected legacy tick); the at-rest LOCK 2 and the
//! vocab guard are unit-tested in verify.rs / tick.rs.

use std::path::Path;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

fn ev() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ev"))
}

// A real git repo (one empty commit, so HEAD resolves for `--run`) + an initialized ev store.
fn git_repo() -> (std::path::PathBuf, String) {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "ev-jurisdiction-{}-{}",
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

// A check that always reads RED: `false` exits non-zero. Its inverse (`true`) flips green, so the
// binding is genuinely falsifiable — the red is real, not a wiring artifact.
const ALWAYS_RED_CHECK: &str = "false";
const ALWAYS_RED_COUNTER_TEST: &str = "true";

// Attempt to decide a jurisdiction-tagged decision that ALSO binds a Test, in one call. Returns the
// raw Output — as of 0.1.19 a C/D + Test is refused at the door, so the caller asserts on the outcome.
fn attempt_decide_tagged_with_test(repo: &Path, head: &str, jurisdiction: &str) -> Output {
    ev().args([
        "decide",
        "import the gateway #1194 ruling (detect-only)",
        "--observe",
        "backfilled from the gateway history",
        "--jurisdiction",
        jurisdiction,
        "--assume",
        "the imported invariant holds",
        "--assume-test",
        ALWAYS_RED_CHECK,
        "--counter-test",
        ALWAYS_RED_COUNTER_TEST,
        "--on-platform",
        "local",
        "--triggered-by",
        "src/lib.rs",
        "--surface",
        "imported-invariant",
        "--verified-at-sha",
        head,
        "--blame",
        "Wang Yu",
    ])
    .current_dir(repo)
    .output()
    .unwrap()
}

// Decide an UNTAGGED test-bound decision (allowed) and return its id. Used to manufacture a legacy
// C/D+Test tick by injecting the jurisdiction tag onto its on-disk file (see inject_jurisdiction).
fn decide_untagged_with_test(repo: &Path, head: &str) -> String {
    let out = ev()
        .args([
            "decide",
            "ordinary gating decision",
            "--assume",
            "the invariant holds",
            "--assume-test",
            ALWAYS_RED_CHECK,
            "--counter-test",
            ALWAYS_RED_COUNTER_TEST,
            "--on-platform",
            "local",
            "--triggered-by",
            "src/lib.rs",
            "--surface",
            "invariant",
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
        "untagged test decide failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_string()
}

// Inject a jurisdiction tag onto an already-written tick file, simulating an immutable tick created
// before the 0.1.19 door. jurisdiction is NON-hashed, so the content-addressed id stays valid — this
// is exactly the legacy data LOCK 1 must keep non-gating. (serde_json is not a dev-dependency, so we
// splice the tag in as text before the closing brace; any field order parses back the same.)
fn inject_jurisdiction(repo: &Path, id: &str, jurisdiction: &str) {
    let path = repo.join(".evolving/ticks").join(id);
    let content = std::fs::read_to_string(&path).unwrap();
    let close = content.rfind('}').expect("a tick file is a JSON object");
    let injected = format!(
        "{},\n  \"jurisdiction\": \"{}\"\n{}",
        content[..close].trim_end(),
        jurisdiction,
        &content[close..]
    );
    std::fs::write(&path, injected).unwrap();
}

#[test]
#[allow(non_snake_case)] // the BDD subject "C-tagged" keeps the jurisdiction letter capitalized
fn decide_should_refuse_a_C_tagged_decision_that_carries_a_test() {
    // given: a repo
    let (r, head) = git_repo();

    // when: decide a C-jurisdiction (detect-only) decision that ALSO binds a runnable test
    let out = attempt_decide_tagged_with_test(&r, &head, "C");

    // then: refused at the door — detect-only never gates, so it holds no test binding (decide now
    // agrees with verify/migrate/correct; the contradiction is closed at the shared build() primitive)
    assert!(
        !out.status.success(),
        "a C-tagged decision carrying a test must be refused at decide"
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("detect-only"),
        "the refusal should name the detect-only conflict; got: {err}"
    );
}

#[test]
#[allow(non_snake_case)]
fn check_should_not_flip_exit_on_red_when_a_LEGACY_C_tagged_decision_is_not_green() {
    // given: a LEGACY C-tagged decision carrying a red test. As of 0.1.19 this is un-creatable via
    // decide (refused at the door — see the test above), so we simulate the immutable pre-fix tick by
    // deciding an untagged test-bound decision and injecting jurisdiction=C onto its on-disk file.
    // This is exactly the data LOCK 1 exists to defend.
    let (r, head) = git_repo();
    let id = decide_untagged_with_test(&r, &head);
    inject_jurisdiction(&r, &id, "C");

    // when: `ev check --run --exit-on-red` runs the red check and would gate
    let out = ev()
        .args(["check", "--run", "--platform", "local", "--exit-on-red"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: LOCK 1 (legacy defense) holds — the C-tagged not-green is mapped to the non-gating memo,
    // so the gate does NOT fire (exit 0) and the row STILL prints, labelled "memo".
    assert!(
        out.status.success(),
        "LOCK 1 must keep a legacy C-tagged red non-gating: stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .any(|l| l.starts_with("memo\t")),
        "expected a memo row; got: {}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn check_should_still_flip_exit_on_red_when_an_untagged_decision_is_not_green() {
    // given: the SAME red-reading decision as the legacy-defense test — but with NO jurisdiction tag
    // (an ordinary gating decision). The only difference from that test is the missing tag, which is
    // exactly what makes this the control proving the C-tagged exit-0 is the lock, not a dead check.
    let (r, head) = git_repo();
    decide_untagged_with_test(&r, &head);

    // when: the same gated check runs
    let out = ev()
        .args(["check", "--run", "--platform", "local", "--exit-on-red"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: the gate DOES fire — exit non-zero, and the row reads "red" (the control, proving the
    // legacy C-tagged exit-0 above is the jurisdiction lock and not a dead check)
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout)
        .lines()
        .any(|l| l.starts_with("red\t")));
}

#[test]
fn list_should_render_the_jurisdiction_when_a_decision_is_tagged() {
    // given: a C-tagged decision with NO test (a detect-only decision — now the only kind that can
    // exist, since a C/D decision carrying a Test is refused at the door)
    let (r, _head) = git_repo();
    let out = ev()
        .args([
            "decide",
            "import the gateway #1194 ruling (detect-only)",
            "--jurisdiction",
            "C",
            "--assume",
            "the imported invariant holds",
            "--blame",
            "Wang Yu",
        ])
        .current_dir(&r)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "deciding a C-tagged decision WITHOUT a test must succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // when: `ev list` prints the ledger
    let out = ev().arg("list").current_dir(&r).output().unwrap();

    // then: the row surfaces the jurisdiction tag
    assert!(out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("jurisdiction=C"),
        "list did not render jurisdiction: {}",
        String::from_utf8_lossy(&out.stdout)
    );
}
