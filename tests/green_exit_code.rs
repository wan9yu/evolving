use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ev() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ev"))
}

// A git repo (one empty commit) + an ev store with green_exit_code = 1; returns (path, HEAD-sha).
fn repo_green_is_1() -> (std::path::PathBuf, String) {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "ev-gec-{}-{}",
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
    assert!(ev()
        .arg("init")
        .current_dir(&p)
        .output()
        .unwrap()
        .status
        .success());
    let cfg = std::fs::read_to_string(p.join(".evolving/config"))
        .unwrap()
        .replace("green_exit_code = 0", "green_exit_code = 1");
    std::fs::write(p.join(".evolving/config"), cfg).unwrap();
    (p, head)
}

fn decide_bound(repo: &std::path::Path, cmd: &str, sha: &str) {
    let out = ev()
        .args([
            "decide",
            "d",
            "--assume",
            "c",
            "--assume-test",
            cmd,
            "--counter-test",
            "true",
            "--on-platform",
            "local",
            "--triggered-by",
            "f",
            "--surface",
            "s",
            "--verified-at-sha",
            sha,
            "--blame",
            "Wang Yu",
        ])
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "decide failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn check_run_should_be_green_when_the_command_exits_the_configured_green_code() {
    // given: green_exit_code = 1 and a ground bound to a command that exits 1
    let (r, head) = repo_green_is_1();
    decide_bound(&r, "false", &head);

    // when: check --run runs the bound command
    let out = ev()
        .args(["check", "--run", "--platform", "local", "--exit-on-red"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: exit 1 == green_exit_code reads green and the gate passes
    assert!(
        out.status.success(),
        "stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("green"));
}

#[test]
fn check_run_should_be_red_when_the_command_exits_a_non_green_code() {
    // given: green_exit_code = 1 and a ground bound to a command that exits 0
    let (r, head) = repo_green_is_1();
    decide_bound(&r, "true", &head);

    // when: check --run runs the bound command
    let out = ev()
        .args(["check", "--run", "--platform", "local", "--exit-on-red"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: exit 0 != green_exit_code reads red and the gate fails
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("red"));
}
