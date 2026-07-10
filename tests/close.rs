use std::process::Command;
fn run(dir: &std::path::Path, envs: &[(&str, &str)], args: &[&str]) -> std::process::Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_ev"));
    c.args(args).current_dir(dir);
    // Isolate from the parent's CLAUDECODE so tests that omit it are not affected.
    c.env_remove("CLAUDECODE");
    for (k, v) in envs {
        c.env(k, v);
    }
    c.output().unwrap()
}
fn fresh() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ev-close-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    assert!(run(&dir, &[], &["init"]).status.success());
    dir
}
fn only_claim_id(dir: &std::path::Path) -> String {
    let out = run(dir, &[], &["brief", "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    v["open"][0]["id"].as_str().unwrap().to_string()
}

#[test]
fn closing_a_bare_claim_is_refused_with_the_sting() {
    let dir = fresh();
    assert!(run(&dir, &[], &["claim", "fixed it"]).status.success());
    let cid = only_claim_id(&dir);
    let out = run(&dir, &[], &["close", &cid]);
    assert_eq!(out.status.code(), Some(1), "bare close must exit 1");
    let msg = String::from_utf8_lossy(&out.stderr);
    assert!(
        msg.contains("evidence"),
        "sting should name the missing evidence: {msg}"
    );
}

#[test]
fn a_dead_close_needs_a_reason_and_succeeds() {
    let dir = fresh();
    assert!(run(&dir, &[], &["claim", "abandon this"]).status.success());
    let cid = only_claim_id(&dir);
    let out = run(
        &dir,
        &[],
        &["close", &cid, "--dead", "--reason", "obsoleted by redesign"],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn closure_verbs_refuse_under_claudecode_without_override() {
    let dir = fresh();
    assert!(run(&dir, &[], &["claim", "x"]).status.success());
    let cid = only_claim_id(&dir);
    let out = run(
        &dir,
        &[("CLAUDECODE", "1")],
        &["close", &cid, "--dead", "--reason", "y"],
    );
    assert_eq!(out.status.code(), Some(1), "must refuse under CLAUDECODE");
    let out = run(
        &dir,
        &[("CLAUDECODE", "1")],
        &["close", &cid, "--dead", "--reason", "y", "--i-am-the-human"],
    );
    assert!(out.status.success(), "override should allow it");
}
