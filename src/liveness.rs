//! Event-driven liveness: has a triggering change landed since a check last ran?
//! Impure (shells git), mirroring `staleness.rs`. The verdict engine stays pure — this
//! produces a bool the caller passes into `verdict_for`.
use std::path::Path;
use std::process::Command;

/// True if any commit in the range `from..to` touches one of `paths`. `None` if git fails or a
/// commit is unknown — in which case the caller stays conservative (staleness simply NOT relaxed).
pub fn changed_between(repo: &Path, from: &str, to: &str, paths: &[String]) -> Option<bool> {
    if paths.is_empty() {
        return Some(false);
    }
    let mut args: Vec<String> = vec!["rev-list".into(), format!("{from}..{to}"), "--".into()];
    args.extend(paths.iter().cloned());
    let out = Command::new("git")
        .args(&args)
        .current_dir(repo)
        .output()
        .ok()?;
    if !out.status.success() {
        return None; // unknown commit / not a git repo → do not evaluate
    }
    Some(!out.stdout.is_empty())
}

/// True if any commit reachable from HEAD and NEWER than `since_commit` touches one of
/// `paths` (a `triggered_by` set). `None` if git fails or `since_commit` is unknown — in
/// which case event-driven staleness is simply NOT evaluated (never a false not-green).
pub fn changed_since(repo: &Path, since_commit: &str, paths: &[String]) -> Option<bool> {
    changed_between(repo, since_commit, "HEAD", paths)
}

/// True if a TRACKED file under `paths` has uncommitted changes (staged or unstaged) — i.e. a run
/// here attests a worktree that differs from the committed code, the false-green shape. UNTRACKED
/// files are excluded (`--untracked-files=no`): a generated/never-committed file has no committed
/// baseline it could contradict, so a check that reads one (e.g. an exporter output vs a frozen
/// snapshot) is not tainted. Empty paths or a git failure → false (never fabricate a dirty).
pub fn worktree_dirty(repo: &Path, paths: &[String]) -> bool {
    if paths.is_empty() {
        return false;
    }
    let mut args: Vec<String> = vec![
        "status".into(),
        "--porcelain".into(),
        "--untracked-files=no".into(),
        "--".into(),
    ];
    args.extend(paths.iter().cloned());
    match Command::new("git").args(&args).current_dir(repo).output() {
        Ok(out) if out.status.success() => !out.stdout.is_empty(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    // A git repo with two commits; returns (path, first_sha, second_sha). The second commit
    // touches `pyproject.toml`; the first touches `readme.md`.
    fn two_commit_repo() -> (std::path::PathBuf, String, String) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "ev-liveness-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        let git = |args: &[&str]| {
            Command::new("git")
                .args(args)
                .current_dir(&p)
                .output()
                .unwrap();
        };
        git(&["init"]);
        git(&["config", "user.email", "t@e.st"]);
        git(&["config", "user.name", "Tester"]);
        std::fs::write(p.join("readme.md"), "a").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "first"]);
        let first = String::from_utf8(
            Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(&p)
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
        .trim()
        .to_string();
        std::fs::write(p.join("pyproject.toml"), "deps=[]").unwrap();
        git(&["add", "."]);
        git(&["commit", "-m", "second touches pyproject"]);
        let second = String::from_utf8(
            Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(&p)
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
        .trim()
        .to_string();
        (p, first, second)
    }

    #[test]
    fn changed_since_should_be_true_when_a_triggered_path_changed_after_the_commit() {
        // given: a repo whose second commit touched pyproject.toml, evaluated from the first commit
        let (repo, first, _second) = two_commit_repo();

        // when: we ask whether pyproject.toml changed since the first commit
        let r = changed_since(&repo, &first, &["pyproject.toml".into()]);

        // then: yes — a triggering change landed after it
        assert_eq!(r, Some(true));
    }

    #[test]
    fn changed_since_should_be_false_when_no_triggered_path_changed_after_the_commit() {
        // given: the same repo evaluated from its HEAD (second) commit
        let (repo, _first, second) = two_commit_repo();

        // when: we ask whether pyproject.toml changed since HEAD
        let r = changed_since(&repo, &second, &["pyproject.toml".into()]);

        // then: no — nothing landed after HEAD
        assert_eq!(r, Some(false));
    }

    #[test]
    fn changed_since_should_be_none_when_the_commit_is_unknown() {
        // given: a repo and a sha that is not in its history
        let (repo, _first, _second) = two_commit_repo();

        // when: we probe from an unknown commit
        let r = changed_since(
            &repo,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            &["pyproject.toml".into()],
        );

        // then: it is None — unknown ⇒ event-driven staleness is simply not evaluated
        assert_eq!(r, None);
    }
}
