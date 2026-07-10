use std::process::Command;

fn run(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ev"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

fn fresh() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ev-doc-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    assert!(run(&dir, &["init"]).status.success());
    dir
}

#[test]
fn doctor_on_a_clean_ledger_reports_ok_and_exits_zero() {
    let dir = fresh();
    assert!(run(&dir, &["claim", "x"]).status.success());
    let out = run(&dir, &["doctor"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stdout)
        .to_lowercase()
        .contains("clean"));
}

#[test]
fn doctor_flags_a_dangling_evidence_ref() {
    let dir = fresh();
    // hand-write an evidence event pointing at a non-existent claim
    let wid = std::fs::read_to_string(dir.join(".evolving/local/writer.toml")).unwrap();
    let wid = wid.split('"').nth(1).unwrap().to_string();
    let path = dir.join(".evolving/ledger").join(format!("{wid}.jsonl"));
    let line = serde_json::json!({
        "v":2,"id":"evd_x","ts":"2020-01-01T00:00:00Z","writer":wid,"seq":99,
        "actor":{"kind":"human"},"type":"evidence","body":{"claim":"clm_missing","ref":"commit:x","status":"recorded"}
    });
    std::fs::write(&path, format!("{line}\n")).unwrap();
    let out = run(&dir, &["doctor"]);
    assert_ne!(
        out.status.code(),
        Some(0),
        "dangling ref should be non-zero"
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("dangling"));
}
