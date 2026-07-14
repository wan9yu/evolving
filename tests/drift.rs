use std::process::Command;

fn run(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ev"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

fn git(dir: &std::path::Path, args: &[&str]) {
    Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .output()
        .unwrap();
}

/// git repo + enrolled ledger + one claim anchored to file:f.txt — the base
/// records the world-state at filing.
fn anchored_repo(tag: &str) -> (std::path::PathBuf, String) {
    let dir = std::env::temp_dir().join(format!("ev-{tag}-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q"]);
    std::fs::write(dir.join("f.txt"), "the invariant\n").unwrap();
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-qm", "one"]);
    assert!(run(&dir, &["init"]).status.success());
    assert!(run(&dir, &["claim", "x", "--source-ref", "s1"])
        .status
        .success());
    let b = run(&dir, &["brief", "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&b.stdout).unwrap();
    let cid = v["open"][0]["id"].as_str().unwrap().to_string();
    assert!(run(&dir, &["evidence", &cid, "file:f.txt"])
        .status
        .success());
    (dir, cid)
}

/// The ONE count, taken two ways: through the memo (one `git log` per reference point, a
/// touch-count table answering every path) and through the per-anchor `git rev-list --count`
/// the memo replaces. The two must agree for EVERY path at EVERY reference point, or the
/// batch is a second source of truth about how far the world moved — and a low count is a
/// false-green, the one failure ev may not have.
///
/// The merge below is the case the memo refuses: `git log --name-only` prints no file list
/// for a merge, and `rev-list -- <path>` prunes history the log does not. ev falls back to
/// the per-anchor count there rather than report a number it did not take the same way, and
/// this test holds it to that on both sides of the merge.
#[test]
fn the_memo_counts_what_the_per_anchor_count_counts() {
    use evolving::verify::{drift, drift_since, EvRef, Seen};

    let dir = std::env::temp_dir().join(format!("ev-memo-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::create_dir_all(dir.join(".evolving/artifacts")).unwrap();
    git(&dir, &["init", "-q"]);
    let head = |d: &std::path::Path| {
        let o = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(d)
            .output()
            .unwrap();
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    };
    let commit = |d: &std::path::Path, files: &[(&str, &str)]| {
        for (p, text) in files {
            std::fs::write(d.join(p), text).unwrap();
        }
        git(d, &["add", "-A"]);
        git(d, &["-c", "commit.gpgsign=false", "commit", "-qm", "c"]);
    };

    // a history that touches each path a different number of times, from each reference
    commit(
        &dir,
        &[
            ("a.txt", "a1\n"),
            ("b.txt", "b1\n"),
            ("sub/c.txt", "c1\n"),
            (".evolving/artifacts/r.md", "r1\n"),
        ],
    );
    let mut refs = vec![head(&dir)];
    commit(&dir, &[("a.txt", "a2\n")]);
    refs.push(head(&dir));
    commit(&dir, &[("a.txt", "a3\n"), ("b.txt", "b2\n")]);
    refs.push(head(&dir));
    commit(&dir, &[("sub/c.txt", "c2\n")]);
    refs.push(head(&dir));
    // a path that is deleted, and one that is renamed: `rev-list -- <path>` does no rename
    // detection, so both must read as a touch of the OLD path too.
    std::fs::remove_file(dir.join("b.txt")).unwrap();
    commit(&dir, &[(".evolving/artifacts/r.md", "r2\n")]);
    refs.push(head(&dir));

    // a reference git resolves in no clone at all: both paths must say "ev cannot count".
    refs.push("0123456789abcdef0123456789abcdef01234567".to_string());

    let anchors = [
        "file:a.txt",
        "file:b.txt",
        "file:sub/c.txt",
        "test:a.txt::a",
        "artifact:r.md",
        "file:never-existed.txt",
        // no path under it at all: neither count says anything about a commit ref
        "commit:HEAD",
    ];
    let sweep = |refs: &[String]| {
        // ONE memo across the sweep — a reference is walked once and answers every anchor.
        let seen = Seen::new();
        for reference in refs {
            for raw in anchors {
                let r = EvRef::parse(raw).unwrap();
                // the reference in the ack slot with no base to fall back on reads exactly
                // the memo's count for that reference.
                let memo = drift_since(&dir, Some(reference), None, &r, &seen);
                let per_anchor = drift(&dir, reference, &r);
                assert_eq!(
                    memo, per_anchor,
                    "memo and per-anchor count disagree on {raw} from {reference}"
                );
            }
        }
        // an unresolvable reference falls back to the pinned base, through the memo too —
        // the C2 fallback, which the ratchet's survival across clones depends on.
        let ghost = "0123456789abcdef0123456789abcdef01234567";
        let a = EvRef::parse("file:a.txt").unwrap();
        assert_eq!(
            drift_since(&dir, Some(ghost), Some(&refs[1]), &a, &seen),
            drift(&dir, &refs[1], &a),
            "an unresolvable ack must fall back to the pinned base, not disarm the ratchet"
        );
    };

    // FIRST: a linear range, which is the range the memo answers from its own table.
    sweep(&refs);
    // the numbers are the ones the history says: from the first commit, a.txt moved twice
    // (a2, a3) and b.txt twice (b2, and its deletion).
    let seen = Seen::new();
    let a = EvRef::parse("file:a.txt").unwrap();
    let b = EvRef::parse("file:b.txt").unwrap();
    assert_eq!(drift_since(&dir, Some(&refs[0]), None, &a, &seen), Some(2));
    assert_eq!(drift_since(&dir, Some(&refs[0]), None, &b, &seen), Some(2));

    // THEN: a merge in the range. `git log --name-only` prints no file list for a merge, and
    // `rev-list -- <path>` prunes history the log does not, so the memo must hand the count
    // back to the per-anchor path rather than report a number it did not take the same way.
    let before_branch = head(&dir);
    git(&dir, &["checkout", "-q", "-b", "side"]);
    commit(&dir, &[("a.txt", "a-side\n"), ("sub/c.txt", "c-side\n")]);
    git(&dir, &["checkout", "-q", "-"]);
    commit(&dir, &[("sub/c.txt", "c-main\n")]);
    git(
        &dir,
        &[
            "-c",
            "commit.gpgsign=false",
            "merge",
            "-q",
            "--no-edit",
            "-X",
            "ours",
            "side",
        ],
    );
    refs.push(before_branch);
    refs.push(head(&dir));
    sweep(&refs);
}

#[test]
fn a_file_anchor_reports_drift_after_the_cited_path_changes() {
    let (dir, cid) = anchored_repo("drift");

    // no drift yet: the cited path is exactly as the anchor saw it
    let out = run(&dir, &["verify", &cid]);
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(!s.contains("drift"), "no drift expected yet: {s}");

    // the world moves under the anchor: the cited path changes in one commit
    std::fs::write(dir.join("f.txt"), "the invariant, rewritten\n").unwrap();
    git(&dir, &["add", "f.txt"]);
    git(&dir, &["commit", "-qm", "two"]);

    let out = run(&dir, &["verify", &cid]);
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        s.contains("drift") && s.contains("1 commit"),
        "drift of 1 commit expected: {s}"
    );

    // the base is a visible fact in the brief's evidence
    let b = run(&dir, &["brief", "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&b.stdout).unwrap();
    assert!(
        v["open"][0]["evidence"][0]["base"].is_string(),
        "evidence should carry its filing base: {v}"
    );
}

#[test]
fn drift_reaches_every_reading_surface() {
    let (dir, cid) = anchored_repo("drift2");

    // the world moves under the anchor
    std::fs::write(dir.join("f.txt"), "rewritten\n").unwrap();
    git(&dir, &["add", "f.txt"]);
    git(&dir, &["commit", "-qm", "two"]);

    // brief --json carries the computed drift (the agents' surface)
    let b = run(&dir, &["brief", "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&b.stdout).unwrap();
    assert_eq!(
        v["open"][0]["evidence"][0]["drift"].as_u64(),
        Some(1),
        "brief --json should carry drift: {v}"
    );

    // verify --json carries resolution + base + drift (the scripted surface)
    let out = run(&dir, &["verify", &cid, "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let check = &v["checks"][0];
    assert_eq!(check["status"].as_str(), Some("resolves"));
    assert!(check["base"].is_string());
    assert_eq!(check["drift"].as_u64(), Some(1), "verify --json drift: {v}");
}
