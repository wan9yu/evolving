//! Diff-aware sha-staleness (the Run-10 #1 fix). The sha-staleness gate used to fire on ANY HEAD
//! advance (pure sha-equality), so on an auto-committing host every bookkeeping commit made a bound
//! check STALE — a check was single-use (green at birth, stale forever after), and a real later
//! regression could never surface. The fix: a sha difference only stales a binding when its
//! `triggered_by` paths actually changed between `verified_at_sha` and the live origin. A bookkeeping
//! commit (HEAD moves, guarded code does not) keeps the binding live; a real guarded change stales it.

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ev() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ev"))
}

fn git(repo: &std::path::Path, args: &[&str]) {
    assert!(Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap()
        .status
        .success());
}

fn head(repo: &std::path::Path) -> String {
    String::from_utf8(
        Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(repo)
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string()
}

fn run(repo: &std::path::Path, args: &[&str]) -> String {
    String::from_utf8_lossy(&ev().args(args).current_dir(repo).output().unwrap().stdout).to_string()
}

/// A git repo + ev store with `staleness_ref = local-head` (so the live origin = working HEAD, the
/// mode that exposes the auto-commit staleness). Two tracked files: guarded.txt + other.txt.
fn repo() -> (std::path::PathBuf, String) {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "ev-staleness-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    git(&p, &["init"]);
    git(&p, &["config", "user.email", "t@e.st"]);
    git(&p, &["config", "user.name", "Tester"]);
    std::fs::write(p.join("guarded.txt"), "v1").unwrap();
    std::fs::write(p.join("other.txt"), "v1").unwrap();
    git(&p, &["add", "."]);
    git(&p, &["commit", "-m", "c1"]);
    assert!(ev()
        .arg("init")
        .current_dir(&p)
        .output()
        .unwrap()
        .status
        .success());
    std::fs::write(p.join(".evolving/config"), "staleness_ref = local-head\n").unwrap();
    let c1 = head(&p);
    (p, c1)
}

/// Decide a chosen ground bound to a green-on-clean test (`true`) with a working negative control
/// (`false` fails on clean → flips), triggered by guarded.txt, verified at `sha`.
fn decide_bound(repo: &std::path::Path, sha: &str) {
    let out = ev()
        .args([
            "decide",
            "guarded.txt stays v1",
            "--assume",
            "the contract holds",
            "--assume-test",
            "true",
            "--counter-test",
            "false",
            "--on-platform",
            "local",
            "--triggered-by",
            "guarded.txt",
            "--surface",
            "s",
            "--verified-at-sha",
            sha,
            "--blame",
            "You",
        ])
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "decide: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn row_label(stdout: &str) -> Option<String> {
    stdout
        .lines()
        .find(|l| l.contains('\t'))
        .and_then(|l| l.split('\t').next())
        .map(|s| s.to_string())
}

#[test]
fn a_bookkeeping_commit_does_not_stale_a_green_binding() {
    // given: a green bound check, then a BOOKKEEPING commit that advances HEAD without touching the
    // guarded path (the auto-commit-host scenario)
    let (r, c1) = repo();
    decide_bound(&r, &c1);
    assert_eq!(
        row_label(&run(&r, &["check", "--run", "--platform", "local"])).as_deref(),
        Some("green"),
        "the binding is green at its verified sha"
    );
    std::fs::write(r.join("other.txt"), "v2").unwrap(); // NOT a guarded path
    git(&r, &["add", "other.txt"]);
    git(&r, &["commit", "-m", "bookkeeping: touch other.txt"]);

    // when: check reads the verdict (HEAD has advanced past verified_at_sha)
    let out = run(&r, &["check"]);

    // then: still GREEN — the sha moved but the guarded path did not, so it is not sha-stale (the fix;
    // before it, this read `stale` and the check was single-use)
    assert_eq!(
        row_label(&out).as_deref(),
        Some("green"),
        "a bookkeeping commit must not stale a binding whose guarded path is unchanged: {out}"
    );
}

#[test]
fn a_commit_touching_the_guarded_path_does_stale_the_binding() {
    // given: a green bound check
    let (r, c1) = repo();
    decide_bound(&r, &c1);
    run(&r, &["check", "--run", "--platform", "local"]);

    // when: a commit changes the GUARDED path (a real drift on the verified premise)
    std::fs::write(r.join("guarded.txt"), "v2").unwrap();
    git(&r, &["add", "guarded.txt"]);
    git(&r, &["commit", "-m", "real change to the guarded path"]);
    let out = run(&r, &["check"]);

    // then: STALE — the guarded path drifted from the verified sha, so the conservative sha-stale
    // stands (re-verify); the fix never lets a real guarded change read green
    assert_eq!(
        row_label(&out).as_deref(),
        Some("stale"),
        "a commit touching the guarded path must stale the binding: {out}"
    );
}
