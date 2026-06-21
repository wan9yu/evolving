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

// Decide a chosen ground bound to shell command `cmd` (with `counter` as its counter-test) on
// platform "local", verified at `sha`. The counter-test must produce the OPPOSITE result of `cmd`
// for the binding to be falsifiable — otherwise `--run` reports it `unproven`, not green/red.
fn decide_bound(repo: &std::path::Path, cmd: &str, counter: &str, sha: &str) {
    let out = ev()
        .args([
            "decide",
            "d",
            "--assume",
            "c",
            "--assume-test",
            cmd,
            "--counter-test",
            counter,
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
    // given: a git repo with a ground bound to a passing command (counter-test flips → falsifiable)
    let (r, head) = git_repo();
    decide_bound(&r, "true", "false", &head);

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
    // given: a git repo with a ground bound to a failing command (counter-test flips → falsifiable)
    let (r, head) = git_repo();
    decide_bound(&r, "false", "true", &head);

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

// A canonical intake line: a chosen ground bound to a RED-reading test (ref "false", counter "true"
// → falsifiable, so the verdict is a real red, not unproven), on platform "local" so --run executes
// it. Parameterized by provenance — the bit LOCK 3 (Part B) keys on.
fn canonical_red_check(provenance: &str) -> String {
    format!(
        "{{\"kind\":\"ev-decision-intake\",\"decision\":\"a ruling\",\
\"grounds\":[{{\"claim\":\"holds\",\"supports\":\"chosen\",\
\"check\":{{\"by\":\"test\",\"ref\":\"false\",\
\"verified_at_sha\":\"d308afac1b2c3d4e5f60718293a4b5c6d7e8f901\",\"counter_test\":\"true\",\
\"liveness\":{{\"platforms\":[\"local\"],\"triggered_by\":[\"f\"],\"surfaces\":[\"s\"]}}}}}}],\
\"blame\":\"agent-runner\",\"provenance\":\"{provenance}\",\"source_ref\":\"R-ap1\"}}\n"
    )
}

fn migrate_canonical(repo: &std::path::Path, body: &str) {
    let path = repo.join("intake.jsonl");
    std::fs::write(&path, body).unwrap();
    let out = ev()
        .args([
            "migrate",
            "--source",
            &format!("canonical:{}", path.display()),
        ])
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "migrate failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn check_should_not_flip_exit_on_red_when_the_tick_is_agent_proposed_even_if_red() {
    // given: an agent-PROPOSED tick whose bound check reads RED (Part B / §五: an agent cannot author
    // a gating rule). It is a real red — counter-test flips — not a vacuous/unproven one.
    let (r, _head) = git_repo();
    migrate_canonical(&r, &canonical_red_check("agent-proposed"));

    // when: the gate runs
    let out = ev()
        .args(["check", "--run", "--platform", "local", "--exit-on-red"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: the gate PASSES (exit 0) — LOCK 3 maps the agent-proposed not-green to the non-gating
    // memo — but the row is SURFACED as memo (not hidden): non-gating, never a false-green or a drop
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "an agent-proposed red must not gate; stdout: {stdout}"
    );
    assert!(
        stdout.contains("memo"),
        "the agent-proposed red must surface as memo, not vanish; stdout: {stdout}"
    );
}

#[test]
fn check_should_still_flip_exit_on_red_when_the_tick_is_human_authored() {
    // given: the IDENTICAL red check but provenance=imported (human-authored history) — the control
    // proving Part B is provenance-keyed, not a dead gate
    let (r, _head) = git_repo();
    migrate_canonical(&r, &canonical_red_check("imported"));

    // when: the gate runs
    let out = ev()
        .args(["check", "--run", "--platform", "local", "--exit-on-red"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: a human-authored red DOES gate (exit non-zero) and reads red
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !out.status.success(),
        "a human-authored red must gate; stdout: {stdout}"
    );
    assert!(stdout.contains("red"), "stdout: {stdout}");
}

#[test]
fn check_run_should_leave_other_platforms_not_run_when_only_local_is_run() {
    // given: a ground that declares platform "local" only, run on a different platform name
    let (r, head) = git_repo();
    decide_bound(&r, "true", "false", &head);

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
