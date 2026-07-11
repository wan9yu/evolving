use evolving::verify::{EvRef, RefKind};

#[test]
fn parses_a_commit_ref() {
    let r = EvRef::parse("commit:deadbeef").unwrap();
    assert!(matches!(r.kind, RefKind::Commit));
    assert_eq!(r.payload, "deadbeef");
}

#[test]
fn parses_a_test_ref_with_a_passline() {
    let r = EvRef::parse("test:target/t.log::test_foo ... ok").unwrap();
    assert!(matches!(r.kind, RefKind::Test));
    assert_eq!(r.payload, "target/t.log");
    assert_eq!(r.passline.as_deref(), Some("test_foo ... ok"));
}

#[test]
fn a_metric_ref_is_recorded_only() {
    let r = EvRef::parse("metric:466 calls/sec").unwrap();
    assert!(matches!(r.kind, RefKind::Metric));
}

#[test]
fn a_real_commit_verifies_and_a_bogus_sha_fails() {
    let dir = std::env::temp_dir().join(format!("ev-vc-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(&dir)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .output()
            .unwrap()
    };
    git(&["init", "-q"]);
    std::fs::write(dir.join("f.txt"), "x").unwrap();
    git(&["add", "."]);
    git(&["commit", "-qm", "first"]);
    let head = String::from_utf8(git(&["rev-parse", "HEAD"]).stdout).unwrap();
    let head = head.trim();
    use evolving::verify::{verify_ref, EvRef};
    assert_eq!(
        verify_ref(&EvRef::parse(&format!("commit:{head}")).unwrap(), &dir),
        "resolves"
    );
    assert_eq!(
        verify_ref(&EvRef::parse("commit:deadbeefdeadbeef").unwrap(), &dir),
        "failed"
    );
}
