use std::process::Command;

fn ev() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ev"))
}
fn tmp() -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("ev-cli-{}-{:?}", std::process::id(), std::time::SystemTime::now()));
    std::fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn show_prints_a_tick_and_a_missing_id_fails() {
    let repo = tmp();
    ev().arg("init").current_dir(&repo).output().unwrap();
    // hand-write a minimal tick file so show has something (decide comes in Plan 2)
    let id = "aaaaaaaaaaaa";
    let tick = r#"{"id":"aaaaaaaaaaaa","parent_id":"","observe":"o","decision":"d","grounds":[],"status":"live","held_since":"","blame":"Wang Yu"}"#;
    std::fs::write(repo.join(".evolving/ticks").join(id), tick).unwrap();

    let out = ev().args(["show", id]).current_dir(&repo).output().unwrap();
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("decision") && s.contains("\"d\""));

    let missing = ev().args(["show", "ffffffffffff"]).current_dir(&repo).output().unwrap();
    assert!(!missing.status.success());
}

#[test]
fn init_creates_the_store_and_is_idempotent() {
    let repo = tmp();
    let out = ev().arg("init").current_dir(&repo).output().unwrap();
    assert!(out.status.success());
    assert!(repo.join(".evolving/ticks").is_dir());
    // second run: still success, no error
    let out2 = ev().arg("init").current_dir(&repo).output().unwrap();
    assert!(out2.status.success());
}

#[test]
fn verify_self_test_reproduces_the_golden_vectors() {
    let out = ev().args(["verify", "--self-test"]).output().unwrap();
    assert!(out.status.success(), "self-test must pass");
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("e2b337f53a1f") && s.contains("638c47b0c9dd"));
}
