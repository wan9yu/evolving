//! Security regressions — guard the two arbitrary-file primitives the audit found: a non-id passed to
//! `ev show` (path traversal on READ) and a hyphen-leading `--from-git` value (git flag-injection on
//! WRITE). Both are reachable when an id/commit flows from a semi-trusted agent wrapper, so ev refuses
//! anything that is not a well-formed commit-ish / 12-hex tick id.
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ev() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ev"))
}

fn repo() -> std::path::PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "ev-sec-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    assert!(ev()
        .arg("init")
        .current_dir(&p)
        .output()
        .unwrap()
        .status
        .success());
    p
}

fn git_init(dir: &std::path::Path) {
    for args in [
        vec!["init"],
        vec!["config", "user.email", "t@example.com"],
        vec!["config", "user.name", "T"],
    ] {
        Command::new("git")
            .args(&args)
            .current_dir(dir)
            .output()
            .unwrap();
    }
    std::fs::write(dir.join("f.txt"), "x").unwrap();
    Command::new("git")
        .args(["add", "-A"])
        .current_dir(dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "c"])
        .current_dir(dir)
        .output()
        .unwrap();
}

#[test]
fn show_should_reject_a_non_id_and_never_read_an_arbitrary_file() {
    // given: a secret file reachable by absolute path and by `..` traversal out of .evolving/ticks
    let r = repo();
    std::fs::write(r.join("secret.txt"), "TOPSECRET").unwrap();
    let abs = r.join("secret.txt");

    for arg in [abs.to_str().unwrap(), "../../../secret.txt"] {
        let out = ev().args(["show", arg]).current_dir(&r).output().unwrap();
        // then: a non-id is refused (not joined to the ticks dir and read)
        assert!(
            !out.status.success(),
            "ev show {arg:?} must be rejected, not treated as a path"
        );
        assert!(
            !String::from_utf8_lossy(&out.stdout).contains("TOPSECRET"),
            "ev show {arg:?} leaked an arbitrary file"
        );
    }
}

#[test]
fn from_git_should_not_let_a_hyphen_value_inject_a_git_flag() {
    // given: a git repo (so --from-git would shell `git show`) and a would-be victim path
    let r = repo();
    git_init(&r);
    let target = r.join("pwned.txt");
    let inject = format!("--output={}", target.display());

    // when: --from-git is fed a git flag instead of a commit-ish
    let out = ev()
        .args(["decide", "--from-git", &inject])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: it does NOT become a `git show --output=<path>` that writes the file
    assert!(
        !target.is_file(),
        "git flag-injection via --from-git wrote {}; stderr: {}",
        target.display(),
        String::from_utf8_lossy(&out.stderr)
    );
}
