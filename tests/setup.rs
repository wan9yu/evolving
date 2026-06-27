//! `ev setup` — self-contained one-step setup. Installs the whole usage loop into a working tree:
//! co-located ledger + the skill where Claude Code finds it + session-start brief + pre-commit gate.
//! Idempotent, non-destructive, --dry-run writes nothing.

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

/// A fresh git working tree with one commit.
fn git_repo() -> std::path::PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "ev-setup-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    git(&p, &["init"]);
    git(&p, &["config", "user.email", "t@e.st"]);
    git(&p, &["config", "user.name", "Tester"]);
    std::fs::write(p.join("a.txt"), "x").unwrap();
    git(&p, &["add", "."]);
    git(&p, &["commit", "-m", "c1"]);
    p
}

#[test]
fn setup_should_install_the_whole_loop_into_a_fresh_working_tree() {
    let r = git_repo();
    let out = ev().arg("setup").current_dir(&r).output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // 1. co-located ledger, tuned to catch local changes
    assert!(r.join(".evolving").is_dir(), "co-located ledger");
    assert!(std::fs::read_to_string(r.join(".evolving/config"))
        .unwrap()
        .contains("staleness_ref = \"local-head\""));

    // 2. the skill, where Claude Code discovers it (content embedded from the repo's SKILL.md)
    let skill = std::fs::read_to_string(r.join(".claude/skills/ev/SKILL.md")).unwrap();
    assert!(
        skill.contains("git for decisions"),
        "the real skill is installed"
    );

    // 3. hooks: a valid settings.json naming the session hook + an executable pre-commit gate
    assert!(r.join(".claude/hooks/ev-brief-sessionstart.sh").is_file());
    let settings: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(r.join(".claude/settings.json")).unwrap())
            .expect("settings.json is valid JSON");
    let cmd = settings["hooks"]["SessionStart"][0]["hooks"][0]["command"]
        .as_str()
        .unwrap();
    assert!(
        cmd.contains("ev-brief-sessionstart.sh"),
        "session hook wired: {cmd}"
    );
    assert!(r.join(".git/hooks/pre-commit").is_file(), "pre-commit gate");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(r.join(".git/hooks/pre-commit"))
            .unwrap()
            .permissions()
            .mode();
        assert!(mode & 0o111 != 0, "pre-commit is executable");
    }
}

#[test]
fn setup_should_be_idempotent_and_not_clobber_on_rerun() {
    let r = git_repo();
    ev().arg("setup").current_dir(&r).output().unwrap();

    // a user customizes the pre-commit hook after the first setup
    let mine = "#!/bin/sh\necho mine\n";
    std::fs::write(r.join(".git/hooks/pre-commit"), mine).unwrap();

    let out = ev().arg("setup").current_dir(&r).output().unwrap();
    assert!(out.status.success());

    // re-run kept the ledger, did not overwrite the user's pre-commit, and made no spurious backup
    assert!(r.join(".evolving").is_dir());
    assert_eq!(
        std::fs::read_to_string(r.join(".git/hooks/pre-commit")).unwrap(),
        mine,
        "an existing pre-commit must not be overwritten"
    );
    assert!(
        !r.join(".claude/skills/ev/SKILL.md.bak").exists(),
        "an identical skill must not be backed up"
    );
}

#[test]
fn setup_dry_run_should_write_nothing() {
    let r = git_repo();
    let out = ev()
        .args(["setup", "--dry-run"])
        .current_dir(&r)
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(!r.join(".evolving").exists(), "dry-run created no ledger");
    assert!(!r.join(".claude").exists(), "dry-run wrote no .claude/");
}

#[test]
fn setup_should_refuse_a_non_git_target() {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "ev-setup-nogit-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    let out = ev().arg("setup").current_dir(&p).output().unwrap();
    assert!(!out.status.success(), "must refuse a non-git target");
}
