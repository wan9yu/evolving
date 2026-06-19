//! ev check --run: execute a bound test locally and produce a run-receipt. A THIN runner —
//! the production receipt-writer is CI / a supervisor hook; --run is for local verification.
//! exit == the configured green_exit_code => green, anything else => red (gray comes from
//! external writers, never from --run).
use crate::receipt::Receipt;
use std::path::Path;
use std::process::Command;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

/// Run the bound `reference` as a shell command in `repo`; return a receipt stamped for
/// `platform`, the current git commit (HEAD), and now (UTC). exit == `green_exit_code` => green,
/// else red.
pub fn run_check(
    repo: &Path,
    reference: &str,
    platform: &str,
    green_exit_code: i32,
) -> Result<Receipt, String> {
    let commit = crate::capture::resolve_sha(repo, &None)?;
    let ran_at = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|e| format!("timestamp: {e}"))?;
    let status = Command::new("sh")
        .arg("-c")
        .arg(reference)
        .current_dir(repo)
        .status()
        .map_err(|e| format!("cannot run {reference:?}: {e}"))?;
    // exit == the configured green code is green; anything else (incl. signal kills) is red.
    let result = if status.code() == Some(green_exit_code) {
        "green"
    } else {
        "red"
    };
    Ok(Receipt {
        test: reference.to_string(),
        platform: platform.to_string(),
        commit,
        ran_at,
        result: result.to_string(),
        falsifiable: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // A fresh git repo (with one empty commit, so HEAD resolves) for resolve_sha.
    fn git_repo() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "ev-runner-{}-{}",
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
        p
    }

    #[test]
    fn run_check_should_record_green_when_the_command_exits_zero() {
        // given: a git repo and a bound command that succeeds
        let repo = git_repo();

        // when: the bound ref is run on platform "local"
        let r = run_check(&repo, "true", "local", 0).expect("ok");

        // then: the receipt is green for that platform, test, and a 40-hex commit
        assert_eq!(r.result, "green");
        assert_eq!(r.platform, "local");
        assert_eq!(r.test, "true");
        assert_eq!(r.commit.len(), 40);
    }

    #[test]
    fn run_check_should_record_red_when_the_command_exits_nonzero() {
        // given: a git repo and a bound command that fails
        let repo = git_repo();

        // when: the bound ref is run
        let r = run_check(&repo, "false", "local", 0).expect("ok");

        // then: the receipt is red
        assert_eq!(r.result, "red");
    }
}
