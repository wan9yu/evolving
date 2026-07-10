use std::process::Command;

fn run(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ev"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

fn fresh() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ev-line-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    assert!(run(&dir, &["init"]).status.success());
    dir
}

#[test]
fn line_json_stable_is_byte_for_byte_the_golden() {
    let dir = fresh();
    // build a deterministic-ish ledger, then render with --stable (which normalizes ids/timestamps)
    assert!(run(&dir, &["claim", "alpha"]).status.success());
    let out = run(&dir, &["line", "--json", "--stable"]);
    let got = String::from_utf8(out.stdout).unwrap();
    let want = include_str!("goldens/line_stable.txt");
    assert_eq!(got, want, "line --json --stable drifted from the golden");
}
