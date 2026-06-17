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
fn init_creates_the_store_and_is_idempotent() {
    let repo = tmp();
    let out = ev().arg("init").current_dir(&repo).output().unwrap();
    assert!(out.status.success());
    assert!(repo.join(".evolving/ticks").is_dir());
    // second run: still success, no error
    let out2 = ev().arg("init").current_dir(&repo).output().unwrap();
    assert!(out2.status.success());
}
