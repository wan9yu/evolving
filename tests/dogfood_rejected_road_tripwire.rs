//! Dogfood: the 0.1.8 rejected-road tripwire, end-to-end against the real `ev` binary.
//!
//! A user-ruled decision CLOSES a road (`--reject "Redis: …"`) and binds a falsifiable tripwire to
//! that road: a structural check that stays GREEN while the road is closed and goes RED when someone
//! re-walks it (re-introduces the token). Re-walking trips `--exit-on-red`. This is the "first teeth"
//! arc — and it is honest precisely BECAUSE the check binds a STRUCTURAL token (a grep-able string in
//! a file). A prose re-walk with no token (the #1194 milestone re-assignment) has nothing to bind and
//! stays surface-only — this dogfood does NOT and CANNOT claim to catch that class.
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ev() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ev"))
}

/// A git repo (one empty commit) with an ev store and a redis-FREE manifest; returns (path, HEAD).
fn repo_with_manifest() -> (std::path::PathBuf, String) {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "ev-rrtw-{}-{}",
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
    // a redis-free manifest: the road stays closed
    std::fs::write(
        p.join("pyproject.toml"),
        "[project]\nname = \"argus\"\ndependencies = []\n",
    )
    .unwrap();
    (p, head)
}

/// Decide a user-ruled decision that CLOSES the Redis road and binds a tripwire to that road:
/// the check `! grep -q redis pyproject.toml` reads GREEN while redis is absent; the counter-test
/// `grep -q redis pyproject.toml` is the re-walk (it reads the opposite), so the binding is falsifiable.
fn decide_closed_road_with_tripwire(repo: &std::path::Path, head: &str) {
    let out = ev()
        .args([
            "decide",
            "keep Redis out of the manifest",
            "--reject",
            "Redis: a new infra dependency the team ruled out",
            "--assume-test",
            "! grep -q redis pyproject.toml",
            "--counter-test",
            "grep -q redis pyproject.toml",
            "--on-platform",
            "local",
            "--triggered-by",
            "pyproject.toml",
            "--surface",
            "pyproject-deps",
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
        "decide failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn check_run(repo: &std::path::Path) -> std::process::Output {
    ev().args(["check", "--run", "--platform", "local", "--exit-on-red"])
        .current_dir(repo)
        .output()
        .unwrap()
}

#[test]
fn a_user_ruled_rejected_road_tripwire_should_stay_green_while_the_road_is_closed() {
    // given: a user-ruled decision closing the Redis road with a tripwire, manifest redis-free
    let (r, head) = repo_with_manifest();
    decide_closed_road_with_tripwire(&r, &head);

    // when: the gate runs while the road stays closed
    let out = check_run(&r);

    // then: the tripwire reads green and the gate passes
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "should pass while closed; stdout: {stdout}"
    );
    assert!(stdout.contains("green"), "stdout: {stdout}");
}

#[test]
fn re_walking_the_closed_road_should_flip_the_tripwire_red_and_gate() {
    // given: the same closed-road tripwire, green while redis is absent
    let (r, head) = repo_with_manifest();
    decide_closed_road_with_tripwire(&r, &head);
    assert!(
        check_run(&r).status.success(),
        "precondition: green while closed"
    );

    // when: someone RE-WALKS the closed road — re-introduces redis into the manifest
    std::fs::write(
        r.join("pyproject.toml"),
        "[project]\nname = \"argus\"\ndependencies = [\"redis>=5\"]\n",
    )
    .unwrap();
    let out = check_run(&r);

    // then: the structural re-walk flips the tripwire RED and --exit-on-red gates
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !out.status.success(),
        "re-walking the closed road must gate; stdout: {stdout}"
    );
    assert!(stdout.contains("red"), "stdout: {stdout}");
}

#[test]
fn the_resurfaced_tripwire_should_name_the_closed_road_in_why_and_reopen() {
    // given: a tripped tripwire (road re-walked)
    let (r, head) = repo_with_manifest();
    decide_closed_road_with_tripwire(&r, &head);

    // when: an operator looks up what the failing selector guards, and reopens the decision
    let why = ev()
        .args(["why", "! grep -q redis pyproject.toml"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: `why` resolves the selector to the decision (a rejected-road tripwire is reverse-lookable)
    assert!(
        why.status.success(),
        "why should resolve the bound selector"
    );
    let why_out = String::from_utf8_lossy(&why.stdout);
    assert!(
        why_out.contains("keep Redis out of the manifest"),
        "why names the decision; stdout: {why_out}"
    );

    // and: `ev brief` surfaces the closed road for a fresh agent (it is user-ruled, load-bearing)
    let brief = ev().arg("brief").current_dir(&r).output().unwrap();
    let brief_out = String::from_utf8_lossy(&brief.stdout);
    assert!(
        brief_out.contains("rejected Redis") || brief_out.contains("Redis"),
        "brief surfaces the closed road; stdout: {brief_out}"
    );
}

#[test]
fn the_tripwire_and_its_counter_test_should_be_true_inverses_independent_of_ev() {
    // given: the redis-free manifest (road closed). The honesty claim is that the check and its
    // counter-test are genuine inverses — proven here WITHOUT ev, so the falsifiability is real, not
    // an artifact of how ev runs them.
    let (r, _head) = repo_with_manifest();
    let sh = |cmd: &str| {
        Command::new("sh")
            .args(["-c", cmd])
            .current_dir(&r)
            .status()
            .unwrap()
            .success()
    };

    // when/then: while the road is CLOSED, the check passes and the re-walk counter-test fails
    assert!(
        sh("! grep -q redis pyproject.toml"),
        "check green while closed"
    );
    assert!(
        !sh("grep -q redis pyproject.toml"),
        "counter (re-walk) red while closed"
    );

    // and: after re-walking the road, the two invert — the check fails and the counter-test passes
    std::fs::write(
        r.join("pyproject.toml"),
        "[project]\nname = \"argus\"\ndependencies = [\"redis>=5\"]\n",
    )
    .unwrap();
    assert!(
        !sh("! grep -q redis pyproject.toml"),
        "check red after re-walk"
    );
    assert!(
        sh("grep -q redis pyproject.toml"),
        "counter green after re-walk"
    );
}
