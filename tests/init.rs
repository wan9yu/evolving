use std::process::Command;

fn run_in(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ev"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

#[test]
fn init_creates_the_ledger_tree_and_union_merge() {
    let dir = std::env::temp_dir().join(format!("ev-init-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = run_in(&dir, &["init"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(dir.join(".evolving/version").exists());
    assert!(dir.join(".evolving/ledger").is_dir());
    assert!(dir.join(".evolving/artifacts").is_dir());
    assert!(dir.join(".evolving/.gitignore").exists());
    let attrs = std::fs::read_to_string(dir.join(".gitattributes")).unwrap();
    assert!(attrs.contains("merge=union"), "{attrs}");
    assert_eq!(
        std::fs::read_to_string(dir.join(".evolving/version"))
            .unwrap()
            .trim(),
        "2"
    );
}

#[test]
fn init_is_idempotent() {
    let dir = std::env::temp_dir().join(format!("ev-init2-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    assert!(run_in(&dir, &["init"]).status.success());
    let out = run_in(&dir, &["init"]);
    assert!(out.status.success());
    // .gitattributes must not double the union line
    let attrs = std::fs::read_to_string(dir.join(".gitattributes")).unwrap();
    assert_eq!(attrs.matches("merge=union").count(), 1, "{attrs}");
}
