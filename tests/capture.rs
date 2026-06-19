use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ev() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ev"))
}
fn repo() -> std::path::PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "ev-cap-cli-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    assert!(ev()
        .arg("init")
        .current_dir(&p)
        .output()
        .unwrap()
        .status
        .success());
    p
}
fn run(repo: &std::path::Path, args: &[&str]) -> std::process::Output {
    ev().args(args).current_dir(repo).output().unwrap()
}

#[test]
fn decide_should_round_trip_clean_through_verify_when_a_decision_is_well_formed() {
    // given: an initialized repo
    let r = repo();

    // when: a well-formed decision is captured
    let out = run(
        &r,
        &[
            "decide",
            "build our own retrieval; reject pgvector",
            "--observe",
            "evaluating retrieval backend",
            "--assume",
            "team has bandwidth long-term",
            "--revisit",
            "Q3 review",
            "--reject",
            "pgvector: would lock our schema",
            "--blame",
            "Wang Yu",
        ],
    );

    // then: decide succeeds and verify is clean
    assert!(
        out.status.success(),
        "decide failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        run(&r, &["verify"]).status.success(),
        "verify should be clean"
    );
}

#[test]
fn decide_should_fail_when_a_test_is_force_bound_to_a_human_ground() {
    // given: an initialized repo
    let r = repo();

    // when: a decision force-binds a test to a --revisit (human) ground
    let out = run(
        &r,
        &[
            "decide",
            "d",
            "--assume",
            "team can maintain this",
            "--revisit",
            "Q3",
            "--assume-test",
            "pytest x",
            "--counter-test",
            "ct",
            "--on-platform",
            "linux-ci",
            "--triggered-by",
            "f",
            "--surface",
            "s",
            "--verified-at-sha",
            "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901",
            "--blame",
            "Wang Yu",
        ],
    );

    // then: it exits non-zero (R2: never both)
    assert!(!out.status.success()); // R2: never both
}

#[test]
fn decide_should_warn_at_decide_and_fail_verify_when_the_system_is_a_self_evolve_subject() {
    // given: an initialized repo
    let r = repo();

    // when: a decision makes the system the subject of self-evolve
    let out = run(
        &r,
        &[
            "decide",
            "the retrieval system will self-evolve its schema",
            "--blame",
            "Wang Yu",
        ],
    );

    // then: decide warns but succeeds, while verify asserts R3 and fails
    assert!(out.status.success()); // decide warns, does not fail
    assert!(String::from_utf8_lossy(&out.stderr)
        .to_lowercase()
        .contains("self-evolve"));
    let v = run(&r, &["verify"]);
    assert!(!v.status.success()); // verify asserts R3
}

#[test]
fn decide_should_fail_when_a_check_is_attached_to_a_rejected_ground() {
    // given: an initialized repo
    let r = repo();

    // when: a decision attaches a test check to a --reject road
    let out = run(
        &r,
        &[
            "decide",
            "d",
            "--reject",
            "x: y",
            "--assume-test",
            "pytest x",
            "--counter-test",
            "ct",
            "--on-platform",
            "linux-ci",
            "--triggered-by",
            "f",
            "--surface",
            "s",
            "--verified-at-sha",
            "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901",
            "--blame",
            "Wang Yu",
        ],
    );

    // then: it exits non-zero
    assert!(!out.status.success());
}

#[test]
fn decide_should_record_the_authority_tag_when_user_ruled_is_declared() {
    // given/when: a decision recorded as user-ruled
    let r = repo();
    let out = run(
        &r,
        &[
            "decide",
            "freeze v1.8",
            "--assume",
            "team agreed",
            "--revisit",
            "Q3",
            "--authority",
            "user-ruled",
            "--blame",
            "Wang Yu",
        ],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let id = String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_string();

    // then: the on-disk tick carries authority=user-ruled
    let raw = std::fs::read_to_string(r.join(".evolving/ticks").join(&id)).unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["authority"], "user-ruled");
}

#[test]
fn decide_should_fail_when_the_authority_value_is_not_in_the_vocabulary() {
    // given/when: a decision with a bogus authority value
    let r = repo();
    let out = run(
        &r,
        &[
            "decide",
            "x",
            "--assume",
            "y",
            "--revisit",
            "Q3",
            "--authority",
            "whatever",
            "--blame",
            "Wang Yu",
        ],
    );

    // then: it is rejected (closed vocabulary: user-ruled | agent-disposable)
    assert!(!out.status.success());
}

#[test]
fn decide_should_record_the_round_id_when_one_is_declared() {
    // given/when: a decision recorded with a durable round_id join/dedup key
    let r = repo();
    let out = run(
        &r,
        &[
            "decide",
            "freeze v1.8",
            "--assume",
            "team agreed",
            "--revisit",
            "Q3",
            "--round-id",
            "R2289",
            "--blame",
            "Wang Yu",
        ],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let id = String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_string();

    // then: the on-disk tick carries round_id (and it is durable in the bookkeeping, not hashed)
    let raw = std::fs::read_to_string(r.join(".evolving/ticks").join(&id)).unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["round_id"], "R2289");
    // and: list + reopen render the round_id so a fresh agent can join/dedup on it
    let listed = run(&r, &["list"]);
    assert!(String::from_utf8_lossy(&listed.stdout).contains("round_id=R2289"));
    let re = run(&r, &["reopen", &id]);
    assert!(String::from_utf8_lossy(&re.stdout).contains("round_id: R2289"));
}

#[test]
fn decide_should_fail_when_the_round_id_is_empty() {
    // given/when: a decision whose declared round_id is empty
    let r = repo();
    let out = run(
        &r,
        &[
            "decide",
            "x",
            "--assume",
            "y",
            "--revisit",
            "Q3",
            "--round-id",
            "",
            "--blame",
            "Wang Yu",
        ],
    );

    // then: it is rejected (non-empty-if-present)
    assert!(!out.status.success());
}

#[test]
fn reopen_should_show_the_authority_tag_when_the_decision_is_user_ruled() {
    // given: a user-ruled decision
    let r = repo();
    let out = run(
        &r,
        &[
            "decide",
            "freeze v1.8; reject v1.9",
            "--assume",
            "team agreed",
            "--revisit",
            "Q3",
            "--reject",
            "v1.9: re-milestoned without sign-off",
            "--authority",
            "user-ruled",
            "--blame",
            "Wang Yu",
        ],
    );
    let id = String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_string();

    // when: the decision is reopened
    let re = run(&r, &["reopen", &id]);

    // then: reopen names the authority so a fresh agent sees it is user-ruled
    assert!(re.status.success());
    assert!(String::from_utf8_lossy(&re.stdout).contains("authority: user-ruled"));
}

#[test]
fn guard_should_fail_when_the_target_tick_is_not_head() {
    // given: a repo with two decisions, so the first is no longer HEAD
    let r = repo();
    let d1 = run(&r, &["decide", "d1", "--assume", "a", "--blame", "Wang Yu"]);
    assert!(
        d1.status.success(),
        "decide d1 failed: {}",
        String::from_utf8_lossy(&d1.stderr)
    );
    let id1 = String::from_utf8_lossy(&d1.stdout)
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_string();
    assert!(
        run(&r, &["decide", "d2", "--assume", "b", "--blame", "Wang Yu"])
            .status
            .success()
    );

    // when: the non-HEAD first tick is guarded
    let out = run(
        &r,
        &[
            "guard",
            "pytest x",
            &id1,
            "a",
            "--counter-test",
            "ct",
            "--on-platform",
            "linux-ci",
            "--triggered-by",
            "f",
            "--surface",
            "s",
            "--verified-at-sha",
            "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901",
            "--blame",
            "Wang Yu",
        ],
    );

    // then: it exits non-zero
    assert!(!out.status.success());
}
