//! Dogfood — Gateway M2 case5, the liveness meta-guard, end-to-end against the real `ev`
//! binary. A ship-image check that is silently never-run reads not-green (the fact no grep
//! gives); a green receipt flips it green; a triggering change after that receipt makes it
//! stale (event-driven, NOT count-N); a wrong-platform receipt does not satisfy the platform.
//! Self-contained and free of proprietary strings; real source stays local-only in internal/.

use ev::receipt::{append, Receipt};
use ev::store::Store;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ev() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ev"))
}

// A real git repo (one commit so HEAD resolves) + an initialized ev store. Returns (path, head).
fn git_repo() -> (std::path::PathBuf, String) {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "ev-dogfood-m2-{}-{}",
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

const DECISION: &str = "AC-5 ship-image smoke must run; loadLdapConfig absent from the built image";
const CLAIM: &str = "the ship-image app.js has 0 hits of loadLdapConfig";
const CHECK: &str = "true"; // a runnable stand-in for the ship-image smoke
const COUNTER: &str = "false";

// Capture #1415-style decision and guard the AC-5 binding on platform "ship-image". Returns child id.
fn decide_and_guard(repo: &std::path::Path, head: &str) -> String {
    let out = ev()
        .args(["decide", DECISION, "--assume", CLAIM, "--blame", "Wang Yu"])
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "decide: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parent = String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_string();
    let g = ev()
        .args([
            "guard",
            CHECK,
            &parent,
            CLAIM,
            "--counter-test",
            COUNTER,
            "--on-platform",
            "ship-image",
            "--triggered-by",
            "Dockerfile",
            "--surface",
            "built-image-app.js",
            "--verified-at-sha",
            head,
            "--blame",
            "Wang Yu",
        ])
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        g.status.success(),
        "guard: {}",
        String::from_utf8_lossy(&g.stderr)
    );
    String::from_utf8_lossy(&g.stdout)
        .split_whitespace()
        .last()
        .unwrap()
        .to_string()
}

#[test]
fn check_should_be_not_run_and_name_the_decision_when_the_ship_image_check_never_ran() {
    // given: an AC-5 binding on platform ship-image with no receipt
    let (r, head) = git_repo();
    decide_and_guard(&r, &head);

    // when: check evaluates with gating (nothing ran on ship-image)
    let out = ev()
        .args(["check", "--exit-on-red"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: not-run names the decision and gates — the fact no grep can give
    assert!(!out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.lines().any(|l| l.starts_with("not-run\t")));
}

#[test]
fn check_should_be_green_when_a_ship_image_receipt_attests_the_run() {
    // given: the AC-5 binding with a fresh green ship-image receipt at HEAD
    let (r, head) = git_repo();
    decide_and_guard(&r, &head);
    append(
        &Store::at(&r),
        &Receipt {
            test: CHECK.into(),
            platform: "ship-image".into(),
            commit: head.clone(),
            ran_at: "2099-01-01T00:00:00Z".into(),
            result: "green".into(),
            falsifiable: None,
        },
    )
    .unwrap();

    // when: check evaluates
    let out = ev()
        .args(["check", "--exit-on-red"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: it is green — the platform is attested
    assert!(
        out.status.success(),
        "stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(String::from_utf8_lossy(&out.stdout)
        .lines()
        .any(|l| l.starts_with("green\t")));
}

#[test]
fn check_should_not_let_a_wrong_platform_receipt_satisfy_the_ship_image_platform() {
    // given: the AC-5 binding (platform ship-image) but a receipt only for platform "local"
    let (r, head) = git_repo();
    decide_and_guard(&r, &head);
    append(
        &Store::at(&r),
        &Receipt {
            test: CHECK.into(),
            platform: "local".into(),
            commit: head.clone(),
            ran_at: "2099-01-01T00:00:00Z".into(),
            result: "green".into(),
            falsifiable: None,
        },
    )
    .unwrap();

    // when: check evaluates with gating
    let out = ev()
        .args(["check", "--exit-on-red"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: still not-run on ship-image — a wrong-platform green does not attest
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout)
        .lines()
        .any(|l| l.starts_with("not-run\t")));
}

#[test]
fn check_should_go_stale_when_a_triggering_change_lands_after_the_green_receipt() {
    // given: a green ship-image receipt at HEAD, then a NEW commit touching the trigger (Dockerfile)
    let (r, head) = git_repo();
    decide_and_guard(&r, &head);
    append(
        &Store::at(&r),
        &Receipt {
            test: CHECK.into(),
            platform: "ship-image".into(),
            commit: head.clone(),
            ran_at: "2099-01-01T00:00:00Z".into(),
            result: "green".into(),
            falsifiable: None,
        },
    )
    .unwrap();
    std::fs::write(r.join("Dockerfile"), "FROM scratch\n").unwrap();
    for args in [
        ["add", "."].as_slice(),
        ["commit", "-m", "touch Dockerfile"].as_slice(),
    ] {
        Command::new("git")
            .args(args)
            .current_dir(&r)
            .output()
            .unwrap();
    }

    // when: check evaluates (a triggering change landed after the run)
    let out = ev()
        .args(["check", "--exit-on-red"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: stale — the green is for a stale world; event-driven, not count-N
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout)
        .lines()
        .any(|l| l.starts_with("stale\t")));
}

#[test]
fn check_should_exempt_the_ship_image_binding_when_the_runner_attests_only_local() {
    // given: the ship-image AC-5 binding, no receipt, a runner attesting only local
    let (r, head) = git_repo();
    decide_and_guard(&r, &head);

    // when: check gates but attests only local (this runner does not speak for ship-image)
    let out = ev()
        .args(["check", "--attest", "local", "--exit-on-red"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: it does NOT gate — the binding is exempt here, not a not-run noise line
    assert!(
        out.status.success(),
        "stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(!String::from_utf8_lossy(&out.stdout)
        .lines()
        .any(|l| l.starts_with("not-run\t")));
}
