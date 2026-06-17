use ev::receipt::{append, Receipt};
use ev::store::Store;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ev() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ev"))
}

// A git repo (one empty commit) + an ev store; the bound verified_at_sha is the all-`a` sha,
// which is NOT the repo HEAD, so a HEAD-based staleness reference reports a mismatch.
const BOUND_SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn repo_with_set(staleness_ref: &str) -> std::path::PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "ev-offline-{}-{}",
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
    assert!(ev()
        .arg("init")
        .current_dir(&p)
        .output()
        .unwrap()
        .status
        .success());
    let cfg = std::fs::read_to_string(p.join(".evolving/config"))
        .unwrap()
        .replace(
            "staleness_ref = \"live-origin\"",
            &format!("staleness_ref = \"{staleness_ref}\""),
        );
    std::fs::write(p.join(".evolving/config"), cfg).unwrap();
    p
}
// Decide a Test-bound ground (ref "pytest x" on platform linux-ci) verified at BOUND_SHA, plus a green receipt.
fn decide_and_green(repo: &std::path::Path) {
    let out = ev()
        .args([
            "decide",
            "d",
            "--assume",
            "c",
            "--assume-test",
            "pytest x",
            "--counter-test",
            "pytest x::flips",
            "--on-platform",
            "linux-ci",
            "--triggered-by",
            "pyproject.toml",
            "--surface",
            "s",
            "--verified-at-sha",
            BOUND_SHA,
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
    append(
        &Store::at(repo),
        &Receipt {
            test: "pytest x".into(),
            platform: "linux-ci".into(),
            commit: BOUND_SHA.into(),
            ran_at: "2099-01-01T00:00:00Z".into(), // far future so age-staleness never fires
            result: "green".into(),
        },
    )
    .unwrap();
}

#[test]
fn check_should_flag_stale_when_local_head_differs_from_the_bound_sha() {
    // given: staleness_ref = local-head and a binding verified at a sha that is not HEAD
    let r = repo_with_set("local-head");
    decide_and_green(&r);

    // when: check runs with gating
    let out = ev()
        .args(["check", "--exit-on-red"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: the live-origin (= local HEAD) differs from verified_at_sha -> stale, gate fails
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("stale"));
}

#[test]
fn check_should_skip_sha_staleness_when_the_policy_is_none() {
    // given: staleness_ref = none and the same mismatched binding (green receipt)
    let r = repo_with_set("none");
    decide_and_green(&r);

    // when: check runs with gating
    let out = ev()
        .args(["check", "--exit-on-red"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: sha-staleness is not evaluated -> green, gate passes
    assert!(
        out.status.success(),
        "stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("green"));
}

#[test]
fn check_offline_should_not_gate_on_an_unresolvable_reference() {
    // given: live-origin policy, no cached origin-sha, and --offline (so the reference is unknown)
    let r = repo_with_set("live-origin");
    decide_and_green(&r);

    // when: check runs --offline with gating
    let out = ev()
        .args(["check", "--offline", "--exit-on-red"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: stale-unknown is not gated -> the green receipt stands, exit 0
    assert!(
        out.status.success(),
        "stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("green"));
}
