use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ev() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ev"))
}

// A git repo (one empty commit) with an ev store; returns (path, HEAD-sha).
fn git_repo() -> (std::path::PathBuf, String) {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "ev-checkrun-{}-{}",
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

// Decide a chosen ground bound to shell command `cmd` on platform "local", verified at `sha`.
fn decide_bound(repo: &std::path::Path, cmd: &str, sha: &str) {
    let out = ev()
        .args([
            "decide",
            "d",
            "--assume",
            "c",
            "--assume-test",
            cmd,
            "--counter-test",
            "false",
            "--on-platform",
            "local",
            "--triggered-by",
            "f",
            "--surface",
            "s",
            "--verified-at-sha",
            sha,
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
}

#[test]
fn check_run_should_record_green_and_pass_when_the_bound_test_succeeds() {
    // given: a git repo with a ground bound to a passing command
    let (r, head) = git_repo();
    decide_bound(&r, "true", &head);

    // when: check --run --platform local runs the bound test and gates
    let out = ev()
        .args(["check", "--run", "--platform", "local", "--exit-on-red"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: the test ran green and the gate passes
    assert!(
        out.status.success(),
        "stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("green"));
}

#[test]
fn check_run_should_record_red_and_gate_when_the_bound_test_fails() {
    // given: a git repo with a ground bound to a failing command
    let (r, head) = git_repo();
    decide_bound(&r, "false", &head);

    // when: check --run --platform local runs the bound test and gates
    let out = ev()
        .args(["check", "--run", "--platform", "local", "--exit-on-red"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: the test ran red and the gate fails
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("red"));
}

#[test]
fn check_run_should_leave_other_platforms_not_run_when_only_local_is_run() {
    // given: a ground that declares platform "local" only, run on a different platform name
    let (r, head) = git_repo();
    decide_bound(&r, "true", &head);

    // when: check --run targets a platform the ground does not declare
    let out = ev()
        .args([
            "check",
            "--run",
            "--platform",
            "ship-image",
            "--exit-on-red",
        ])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: nothing ran for it, so the ground stays not-run and the gate fails
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("not-run"));
}
