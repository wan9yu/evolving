use std::process::Command;

fn run(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ev"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

fn fresh() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ev-claim-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    assert!(run(&dir, &["init"]).status.success());
    dir
}

#[test]
fn a_claim_writes_one_claim_event() {
    let dir = fresh();
    assert!(run(&dir, &["claim", "fixed the parser"]).status.success());
    let wid = std::fs::read_to_string(dir.join(".evolving/local/writer.toml")).unwrap();
    let wid = wid.split('"').nth(1).unwrap();
    let log =
        std::fs::read_to_string(dir.join(".evolving/ledger").join(format!("{wid}.jsonl"))).unwrap();
    assert_eq!(
        log.lines()
            .filter(|l| l.contains("\"type\":\"claim\""))
            .count(),
        1
    );
    assert!(log.contains("fixed the parser"));
}

#[test]
fn same_source_ref_files_only_once() {
    let dir = fresh();
    assert!(
        run(&dir, &["claim", "did a thing", "--source-ref", "sess-1"])
            .status
            .success()
    );
    let second = run(&dir, &["claim", "did a thing", "--source-ref", "sess-1"]);
    assert!(second.status.success());
    let wid = std::fs::read_to_string(dir.join(".evolving/local/writer.toml")).unwrap();
    let wid = wid.split('"').nth(1).unwrap();
    let log =
        std::fs::read_to_string(dir.join(".evolving/ledger").join(format!("{wid}.jsonl"))).unwrap();
    assert_eq!(
        log.lines()
            .filter(|l| l.contains("\"type\":\"claim\""))
            .count(),
        1
    );
}
