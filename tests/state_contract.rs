use serde_json::Value;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ev() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ev"))
}
fn repo() -> std::path::PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "ev-state-cli-{}-{}",
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
// Decide a test-bound ground (no receipts -> not-run) and a rejected road; returns the tick id.
fn decide(repo: &std::path::Path) -> String {
    let out = ev()
        .args([
            "decide",
            "no-Redis posture",
            "--assume",
            "no Redis; multi-pod via existing DB",
            "--assume-test",
            "pytest x",
            "--counter-test",
            "pytest x::flips",
            "--on-platform",
            "linux-ci",
            "--triggered-by",
            "pyproject.toml",
            "--surface",
            "s",
            "--verified-at-sha",
            "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901",
            "--reject",
            "Redis: a new infra dependency",
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
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_string()
}

#[test]
fn check_should_write_a_state_file_naming_each_ground_verdict() {
    // given: a decided tick with a not-run test ground and a rejected road
    let r = repo();
    let id = decide(&r);

    // when: ev check evaluates it
    assert!(ev()
        .arg("check")
        .current_dir(&r)
        .output()
        .unwrap()
        .status
        .success());

    // then: results/state/<id>.json names the tick and carries each ground's verdict
    let text =
        std::fs::read_to_string(r.join(".evolving/results/state").join(format!("{id}.json")))
            .unwrap();
    let v: Value = serde_json::from_str(&text).expect("valid json");
    assert_eq!(v["tick_id"], id);
    assert_eq!(v["staleness_ref"]["policy"], "live-origin");
    // the test-bound ground reads not-run (no receipts); the rejected road is a "none" check
    let grounds = v["grounds"].as_array().unwrap();
    assert!(grounds
        .iter()
        .any(|g| g["check"] == "test" && g["verdict"] == "not-run"));
    assert!(grounds
        .iter()
        .any(|g| g["check"] == "none" && g["supports"] == "rejected:Redis"));
}
