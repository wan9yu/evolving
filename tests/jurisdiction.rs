//! Task 2 — the jurisdiction tag + the C/D **structurally ungateable** guarantee, end-to-end against
//! the real `ev` binary. A jurisdiction in {A,B,C,D} is a non-hashed bookkeeping tag (sibling of
//! authority). C/D are detect-only: a not-green C/D-tagged decision must NEVER flip `--exit-on-red`
//! (LOCK 1, gate-time → maps any not-green verdict to the non-gating `Verdict::Memo`), and a C/D tick
//! may carry no Test check at rest (LOCK 2, an `ev verify` rule). This test drives LOCK 1 through the
//! real CLI; the at-rest rule and the vocab guard are unit-tested in verify.rs / tick.rs.

use std::process::Command;
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

// Decide a tick that carries a Test-bound ground (so `check` reads a verdict for it) AND a
// jurisdiction tag in one call. Returns the decision tick id.
fn decide_tagged(repo: &std::path::Path, head: &str, jurisdiction: &str) -> String {
    let out = ev()
        .args([
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
        .unwrap();
    assert!(
        out.status.success(),
        "decide failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_string()
}

#[test]
#[allow(non_snake_case)] // the BDD subject "C-tagged" keeps the jurisdiction letter capitalized
fn check_should_not_flip_exit_on_red_when_a_C_tagged_decision_is_not_green() {
    // given: a C-tagged decision whose bound check reads red (a detect-only jurisdiction)
    let (r, head) = git_repo();
    let _id = decide_tagged(&r, &head, "C");

    // when: `ev check --run --exit-on-red` runs the red check and gates
    let out = ev()
        .args(["check", "--run", "--platform", "local", "--exit-on-red"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: the gate does NOT fire — exit 0 (C is structurally ungateable, mapped to memo) ...
    assert!(
        out.status.success(),
        "C-tagged not-green flipped the gate: stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    // ... and the row STILL prints, now labelled "memo" (the fact is surfaced, just non-gating).
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
    // given: the SAME red-reading decision but with NO jurisdiction tag (an ordinary gating decision)
    let (r, head) = git_repo();
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
            &head,
            "--blame",
            "Wang Yu",
        ])
        .current_dir(&r)
        .output()
        .unwrap();
    assert!(out.status.success());

    // when: the same gated check runs
    let out = ev()
        .args(["check", "--run", "--platform", "local", "--exit-on-red"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: the gate DOES fire — exit non-zero, and the row reads "red" (the control, proving the
    // C-tagged exit-0 above is the jurisdiction lock and not a dead check)
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout)
        .lines()
        .any(|l| l.starts_with("red\t")));
}

#[test]
fn list_should_render_the_jurisdiction_when_a_decision_is_tagged() {
    // given: a C-tagged decision in the ledger
    let (r, head) = git_repo();
    let _id = decide_tagged(&r, &head, "C");

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
