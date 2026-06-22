//! `ev decide --dry-run` — assemble + validate + compute the real id, but write nothing. A safe
//! preview before the immutable append.
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ev() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ev"))
}
fn repo() -> std::path::PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "ev-dryrun-{}-{}",
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
fn run(repo: &std::path::Path, args: &[&str]) -> std::process::Output {
    ev().args(args).current_dir(repo).output().unwrap()
}
fn tick_count(repo: &std::path::Path) -> usize {
    std::fs::read_dir(repo.join(".evolving/ticks"))
        .unwrap()
        .filter(|e| e.as_ref().unwrap().path().is_file())
        .count()
}

#[test]
fn decide_dry_run_should_preview_a_tick_without_writing_anything() {
    // given: an empty store
    let r = repo();
    assert_eq!(tick_count(&r), 0);

    // when: a decision is run with --dry-run
    let out = run(
        &r,
        &[
            "decide",
            "ship the slice",
            "--assume",
            "scope is locked",
            "--blame",
            "You",
            "--dry-run",
        ],
    );

    // then: it succeeds, says it WOULD record (preview only), and no tick is written
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        s.contains("would record") && s.contains("dry run"),
        "dry-run output was {s:?}"
    );
    assert_eq!(tick_count(&r), 0, "a dry run must write no tick");
}

#[test]
fn decide_dry_run_should_preview_the_same_id_a_real_decide_then_writes() {
    // given: a store and one set of decide args
    let r = repo();
    let args = [
        "decide",
        "freeze the schema",
        "--assume",
        "v2 stays stable",
        "--blame",
        "You",
    ];

    // when: the SAME decision is previewed (dry-run) then actually recorded
    let mut dargs = args.to_vec();
    dargs.push("--dry-run");
    let preview = run(&r, &dargs);
    let real = run(&r, &args);

    // then: the dry-run preview id equals the id the real append mints — the id is content-addressed,
    // and a dry-run never moves HEAD, so the parent (and thus the id) is identical
    assert!(preview.status.success());
    assert!(
        real.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&real.stderr)
    );
    let pid = String::from_utf8_lossy(&preview.stdout)
        .split_whitespace()
        .nth(2)
        .unwrap()
        .to_string();
    let rid = String::from_utf8_lossy(&real.stdout)
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_string();
    assert_eq!(
        pid, rid,
        "the dry-run preview must show the id the real decide records"
    );
    assert_eq!(tick_count(&r), 1, "only the real decide writes a tick");
}

#[test]
fn decide_should_treat_a_dry_run_in_value_position_as_a_literal_not_the_preview_flag() {
    // given: a REAL decide whose --observe value is literally the string "--dry-run" (no standalone
    // --dry-run flag is present), exercising the pairing-aware extraction
    let r = repo();
    let out = run(
        &r,
        &[
            "decide",
            "d",
            "--observe",
            "--dry-run",
            "--assume",
            "c",
            "--blame",
            "You",
        ],
    );

    // then: it RECORDS (not a preview) and writes one tick — a global strip would instead eat the
    // value, misalign the flags, and fail
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        s.contains("recorded") && !s.contains("would record"),
        "a --dry-run in value position must record, not preview: {s:?}"
    );
    assert_eq!(tick_count(&r), 1);
}

#[test]
fn decide_dry_run_should_still_reject_an_invalid_binding_and_write_nothing() {
    // given: a store
    let r = repo();

    // when: a dry run carries a test binding with no --counter-test (a hard validation error)
    let out = run(
        &r,
        &[
            "decide",
            "d",
            "--assume",
            "c",
            "--assume-test",
            "pytest x",
            "--blame",
            "You",
            "--dry-run",
        ],
    );

    // then: --dry-run validates exactly like a real append — it fails, and nothing is written
    assert!(
        !out.status.success(),
        "an invalid binding must fail even under --dry-run"
    );
    assert_eq!(tick_count(&r), 0);
}
