//! Resolve the staleness-reference sha per the configured policy. No network — `live-origin`
//! reads the last-fetched upstream tracking ref. Returns None when the reference can't be
//! determined ("stale-unknown"), in which case sha-staleness is simply not evaluated.
use crate::store::Store;
use std::path::Path;
use std::process::Command;

/// `git rev-parse <rev>` in `repo` → the 40-lower-hex sha, or None if it fails / is malformed.
pub fn git_sha(repo: &Path, rev: &str) -> Option<String> {
    let out = Command::new("git")
        .args(["rev-parse", rev])
        .current_dir(repo)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let is_40_hex = sha.len() == 40
        && sha
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    is_40_hex.then_some(sha)
}

/// The staleness-reference sha per `policy` ("none" | "local-head" | anything else = live-origin).
/// `offline` forces the cached reference (results/origin-sha) and never resolves fresh.
pub fn resolve(repo: &Path, store: &Store, policy: &str, offline: bool) -> Option<String> {
    if offline {
        return store.read_origin_sha();
    }
    match policy {
        "none" => None,
        "local-head" => git_sha(repo, "HEAD"),
        _ => {
            // live-origin: the last-fetched upstream; cache it so an --offline run can reuse it.
            match git_sha(repo, "@{upstream}") {
                Some(sha) => {
                    let _ = std::fs::write(store.root.join("results").join("origin-sha"), &sha);
                    Some(sha)
                }
                None => {
                    eprintln!("warning: cannot resolve the live-origin staleness reference (no upstream?) — sha-staleness skipped");
                    None
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

    // A fresh git repo (one empty commit) + an ev store; returns (path, HEAD-sha).
    fn git_store() -> (std::path::PathBuf, Store, String) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "ev-staleness-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        for args in [
            ["init"].as_slice(),
            ["config", "user.email", "t@e.st"].as_slice(),
            ["config", "user.name", "Tester"].as_slice(),
            ["commit", "--allow-empty", "-m", "init"].as_slice(),
        ] {
            Command::new("git")
                .args(args)
                .current_dir(&p)
                .output()
                .unwrap();
        }
        let head = String::from_utf8(
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
        let s = Store::at(&p);
        s.init().unwrap();
        (p, s, head)
    }

    #[test]
    fn git_sha_should_return_the_head_when_run_in_a_git_repo() {
        // given: a git repo with one commit
        let (p, _s, head) = git_store();

        // when: HEAD is resolved
        let sha = git_sha(&p, "HEAD");

        // then: it is the 40-hex HEAD sha
        assert_eq!(sha.as_deref(), Some(head.as_str()));
    }

    #[test]
    fn resolve_should_be_none_when_the_policy_is_none() {
        // given: a git repo
        let (p, s, _head) = git_store();

        // when: resolved with policy "none"
        let sha = resolve(&p, &s, "none", false);

        // then: there is no staleness reference
        assert!(sha.is_none());
    }

    #[test]
    fn resolve_should_be_the_local_head_when_the_policy_is_local_head() {
        // given: a git repo at a known HEAD
        let (p, s, head) = git_store();

        // when: resolved with policy "local-head"
        let sha = resolve(&p, &s, "local-head", false);

        // then: it is the local HEAD sha
        assert_eq!(sha.as_deref(), Some(head.as_str()));
    }

    #[test]
    fn resolve_should_use_the_cached_origin_when_offline() {
        // given: a store with a cached origin-sha
        let (p, s, _head) = git_store();
        std::fs::write(
            s.root.join("results").join("origin-sha"),
            "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901",
        )
        .unwrap();

        // when: resolved offline (any policy)
        let sha = resolve(&p, &s, "live-origin", true);

        // then: it is the cached reference, resolved without git
        assert_eq!(
            sha.as_deref(),
            Some("d308afac1b2c3d4e5f60718293a4b5c6d7e8f901")
        );
    }
}
