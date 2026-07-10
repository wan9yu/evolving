use std::process::Command;
fn run(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ev"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}
fn fresh() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ev-brief-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    assert!(run(&dir, &["init"]).status.success());
    dir
}

#[test]
fn brief_json_lists_open_claims_and_a_footer_event_id() {
    let dir = fresh();
    assert!(run(&dir, &["claim", "fix the thing"]).status.success());
    let out = run(&dir, &["brief", "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["open"].as_array().unwrap().len(), 1);
    assert!(
        v["as_of"].is_string(),
        "brief must carry the as-of event id"
    );
}

#[test]
fn brief_text_stays_under_2kb() {
    let dir = fresh();
    for i in 0..10 {
        assert!(run(&dir, &["claim", &format!("claim number {i}")])
            .status
            .success());
    }
    let out = run(&dir, &["brief"]);
    assert!(
        out.stdout.len() <= 2048,
        "brief was {} bytes",
        out.stdout.len()
    );
}
