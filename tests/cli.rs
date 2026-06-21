use std::process::Command;

fn ev() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ev"))
}
fn tmp() -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "ev-cli-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn show_should_print_the_tick_and_fail_on_a_missing_id_when_given_an_initialized_store() {
    // given: an initialized store holding one hand-written tick
    let repo = tmp();
    ev().arg("init").current_dir(&repo).output().unwrap();
    // hand-write a minimal tick file so show has something (decide comes in Plan 2)
    let id = "aaaaaaaaaaaa";
    let tick = r#"{"id":"aaaaaaaaaaaa","parent_id":"","observe":"o","decision":"d","grounds":[],"status":"live","held_since":"","blame":"Wang Yu"}"#;
    std::fs::write(repo.join(".evolving/ticks").join(id), tick).unwrap();

    // when: show is run for the present id and for a missing id
    let out = ev().args(["show", id]).current_dir(&repo).output().unwrap();
    let missing = ev()
        .args(["show", "ffffffffffff"])
        .current_dir(&repo)
        .output()
        .unwrap();

    // then: the present id prints the decision and the missing id fails
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("decision") && s.contains("\"d\""));
    assert!(!missing.status.success());
}

#[test]
fn init_should_create_the_store_and_stay_idempotent_when_run_twice() {
    // given: an empty repository directory
    let repo = tmp();

    // when: init is run twice in that directory
    let out = ev().arg("init").current_dir(&repo).output().unwrap();
    let out2 = ev().arg("init").current_dir(&repo).output().unwrap();

    // then: both runs succeed and the store directory exists
    assert!(out.status.success());
    assert!(repo.join(".evolving/ticks").is_dir());
    // second run: still success, no error
    assert!(out2.status.success());
}

#[test]
fn verify_should_reproduce_the_golden_vectors_when_run_in_self_test_mode() {
    // given: the verify command in self-test mode

    // when: verify --self-test is run
    let out = ev().args(["verify", "--self-test"]).output().unwrap();

    // then: it succeeds and emits ALL frozen golden vectors (genesis, case1, harvested, and the
    // 0.1.8 rejected-road tripwire) — so a drift in any pinned byte layout fails self-test
    assert!(out.status.success(), "self-test must pass");
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        s.contains("e2b337f53a1f")
            && s.contains("638c47b0c9dd")
            && s.contains("0cf784b51331")
            && s.contains("9c5feb4582ac")
    );
}
