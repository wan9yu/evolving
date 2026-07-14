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
fn a_real_commit_resolves_and_a_bogus_sha_reads_gone() {
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
    use evolving::verify::{verify_ref, Commits, EvRef, Status};
    assert_eq!(
        verify_ref(
            &EvRef::parse(&format!("commit:{head}")).unwrap(),
            &dir,
            &Commits::none()
        ),
        Status::Resolves
    );
    assert_eq!(
        verify_ref(
            &EvRef::parse("commit:deadbeefdeadbeef").unwrap(),
            &dir,
            &Commits::none()
        ),
        // The object is absent from this clone — the commit's container is gone.
        Status::Gone
    );
}

/// The batch path and the single-ref path must read the SAME status for the same sha.
/// The read path resolves every `commit:` ref in one `git cat-file --batch-check`
/// instead of one `git rev-parse` per sha; a batch that answered differently would be
/// a second source of truth about what git said.
#[test]
fn the_batch_path_reads_the_same_status_as_the_single_ref_path() {
    use evolving::verify::{verify_ref, Commits, EvRef, Status};
    let dir = std::env::temp_dir().join(format!("ev-vcb-{}", ulid::Ulid::new()));
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
    let mut real: Vec<String> = Vec::new();
    for i in 0..3 {
        std::fs::write(dir.join("f.txt"), format!("{i}")).unwrap();
        git(&["add", "."]);
        git(&["-c", "commit.gpgsign=false", "commit", "-qm", "c"]);
        let sha = String::from_utf8(git(&["rev-parse", "HEAD"]).stdout).unwrap();
        real.push(sha.trim().to_string());
    }
    // a tree sha is a real object that is NOT a commit: `<sha>^{commit}` peels to
    // nothing, so both paths must read it the same way as an absent object.
    let tree = String::from_utf8(git(&["rev-parse", "HEAD^{tree}"]).stdout).unwrap();
    let mut shas: Vec<String> = real.clone();
    shas.push("deadbeefdeadbeefdeadbeefdeadbeefdeadbeef".into());
    shas.push("0000000000000000000000000000000000000000".into());
    shas.push(tree.trim().to_string());
    // a duplicate: the batch dedupes and must still answer for it
    shas.push(real[0].clone());

    // The ONE dispatch, read twice: once through the resolved set, once through an empty
    // one (which falls back to the single-ref check). The two must not part company.
    let resolved = Commits::resolved(&shas, &dir);
    let status = |sha: &str, seen: &Commits| {
        verify_ref(&EvRef::parse(&format!("commit:{sha}")).unwrap(), &dir, seen)
    };
    for sha in &shas {
        assert_eq!(
            status(sha, &resolved),
            status(sha, &Commits::none()),
            "batch and single disagree on {sha}"
        );
    }
    assert_eq!(status(&real[0], &resolved), Status::Resolves);
    assert_eq!(
        status("deadbeefdeadbeefdeadbeefdeadbeefdeadbeef", &resolved),
        Status::Gone,
        "a sha that resolves nowhere is gone, not resolves"
    );
}
