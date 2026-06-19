use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ev() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ev"))
}
fn repo() -> std::path::PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "ev-listlog-{}-{}",
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
fn decide(repo: &std::path::Path, text: &str) -> String {
    let out = ev()
        .args(["decide", text, "--assume", "a reason", "--blame", "Wang Yu"])
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "decide: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_string()
}

#[test]
fn list_should_show_each_decision_when_the_ledger_has_ticks() {
    // given: a store with two recorded decisions
    let r = repo();
    decide(&r, "first decision");
    decide(&r, "second decision");

    // when: ev list runs
    let out = ev().arg("list").current_dir(&r).output().unwrap();

    // then: it exits 0 and names both decisions
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("first decision"));
    assert!(stdout.contains("second decision"));
}

#[test]
fn list_should_say_so_when_the_ledger_is_empty() {
    // given: a fresh store with no decisions
    let r = repo();

    // when: ev list runs
    let out = ev().arg("list").current_dir(&r).output().unwrap();

    // then: it exits 0 and says there are none
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("no decisions"));
}

#[test]
fn log_should_show_the_lineage_from_head_when_decisions_chain() {
    // given: a decision then a guard that writes a child (a two-tick lineage)
    let r = repo();
    let parent = decide(&r, "freeze the schema");
    let g = ev()
        .args([
            "guard",
            "pytest x",
            &parent,
            "a reason",
            "--counter-test",
            "pytest x::flips",
            "--on-platform",
            "linux-ci",
            "--triggered-by",
            "schema.sql",
            "--surface",
            "schema-ddl",
            "--verified-at-sha",
            "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901",
            "--blame",
            "Wang Yu",
        ])
        .current_dir(&r)
        .output()
        .unwrap();
    assert!(
        g.status.success(),
        "guard: {}",
        String::from_utf8_lossy(&g.stderr)
    );

    // when: ev log runs
    let out = ev().arg("log").current_dir(&r).output().unwrap();

    // then: it exits 0 and shows the decision (HEAD child + its parent share the decision text)
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("freeze the schema"));
}

#[test]
fn list_should_fail_when_there_is_no_store() {
    // given: a bare directory with no .evolving store
    let p = std::env::temp_dir().join(format!("ev-listlog-nostore-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();

    // when: ev list runs
    let out = ev().arg("list").current_dir(&p).output().unwrap();

    // then: it errors and exits non-zero
    assert!(!out.status.success());
}

#[test]
fn check_should_point_to_run_to_prove_falsifiability_when_not_run() {
    // given: a decision whose chosen ground is bound to a test (so check prints a row)
    let r = repo();
    let parent = decide(&r, "no Redis");
    let g = ev()
        .args([
            "guard",
            "pytest x",
            &parent,
            "a reason",
            "--counter-test",
            "pytest x::flips",
            "--on-platform",
            "linux-ci",
            "--triggered-by",
            "pyproject.toml",
            "--surface",
            "pyproject-deps",
            "--verified-at-sha",
            "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901",
            "--blame",
            "Wang Yu",
        ])
        .current_dir(&r)
        .output()
        .unwrap();
    assert!(
        g.status.success(),
        "guard: {}",
        String::from_utf8_lossy(&g.stderr)
    );

    // when: check runs
    let out = ev().arg("check").current_dir(&r).output().unwrap();

    // then: it points the reader at --run to prove falsifiability (the counter-test runs there)
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("prove its falsifiability"));
}
