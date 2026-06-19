//! `ev migrate` — multi-source idempotent backfill + reconcile, driven end-to-end against the real
//! binary. Each test writes a source fixture, runs `ev migrate`, and asserts on the printed summary
//! and the on-disk store. The fixtures are minimal, self-contained substrates with no proprietary
//! content.
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ev() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ev"))
}

/// A fresh, initialized ev store in a unique temp dir.
fn repo() -> std::path::PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "ev-migrate-{}-{}",
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

/// Write a source fixture file under the repo and return its `<kind>:<path>` source spec.
fn write_source(repo: &std::path::Path, kind: &str, name: &str, body: &str) -> String {
    let path = repo.join(name);
    std::fs::write(&path, body).unwrap();
    format!("{kind}:{}", path.display())
}

/// Write a `--jurisdiction-map` file under the repo and return its path as a string.
fn write_map(repo: &std::path::Path, name: &str, body: &str) -> String {
    let path = repo.join(name);
    std::fs::write(&path, body).unwrap();
    path.display().to_string()
}

/// How many tick files the store holds.
fn tick_count(repo: &std::path::Path) -> usize {
    std::fs::read_dir(repo.join(".evolving/ticks"))
        .unwrap()
        .filter(|e| e.as_ref().unwrap().path().is_file())
        .count()
}

const TWO_ROUNDS: &str = "\
## R2289 restore-safety counter DB-backed
- rejected: Redis: would add a new infra dependency
## R2290 ship the cross-pod drain
";

#[test]
fn migrate_should_skip_every_record_when_run_twice() {
    // given: a store and a 2-record gitlog source, imported once with a --blame fallback
    let r = repo();
    let src = write_source(&r, "gitlog", "chat-room.md", TWO_ROUNDS);
    let first = run(&r, &["migrate", "--source", &src, "--blame", "Wang Yu"]);
    assert!(
        first.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    let after_first = tick_count(&r);
    assert_eq!(after_first, 2, "first import writes both records");

    // when: the SAME migrate runs a second time
    let second = run(&r, &["migrate", "--source", &src, "--blame", "Wang Yu"]);

    // then: it succeeds, writes nothing new (idempotent), and reports both records skipped
    assert!(second.status.success());
    assert_eq!(tick_count(&r), after_first, "a re-run writes no new ticks");
    let out = String::from_utf8_lossy(&second.stdout);
    assert!(
        out.contains("imported 0") && out.contains("skipped 2"),
        "summary was {out:?}"
    );
}

#[test]
fn migrate_should_report_the_relinked_count_when_a_record_is_back_dated() {
    // given: a store that already holds the LATER round R2290 as genesis (parent ""), captured
    // before the earlier round was ever migrated.
    let r = repo();
    let later = write_source(
        &r,
        "gitlog",
        "later.md",
        "## R2290 ship the cross-pod drain\n",
    );
    assert!(
        run(&r, &["migrate", "--source", &later, "--blame", "Wang Yu"])
            .status
            .success()
    );

    // when: a source brings BOTH the EARLIER R2289 and the existing R2290 — sorted, R2289 lands
    // first, so R2290 should now sit AFTER it, but its stored parent is still "" (a back-dated
    // mid-chain insert: the chain is being re-linked around the already-present R2290).
    let both = write_source(
        &r,
        "gitlog",
        "both.md",
        "## R2289 restore-safety counter DB-backed\n## R2290 ship the cross-pod drain\n",
    );
    let out = run(&r, &["migrate", "--source", &both, "--blame", "Wang Yu"]);

    // then: it succeeds, imports the new earlier round, and reports the existing one re-linked
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        s.contains("imported 1") && s.contains("re-linked 1"),
        "summary was {s:?}"
    );
}

#[test]
fn migrate_reconcile_should_surface_a_source_only_ruling_as_a_gap() {
    // given: a store holding ONLY R2289 (imported), and a source that ALSO declares R9999 — a
    // ruling the source has but the ledger never captured (the capture gap).
    let r = repo();
    let seed = write_source(
        &r,
        "gitlog",
        "seed.md",
        "## R2289 restore-safety counter DB-backed\n",
    );
    assert!(
        run(&r, &["migrate", "--source", &seed, "--blame", "Wang Yu"])
            .status
            .success()
    );
    let against = write_source(
        &r,
        "gitlog",
        "against.md",
        "## R2289 restore-safety counter DB-backed\n## R9999 a ruling never captured\n",
    );

    // when: reconcile joins the source against the store
    let out = run(&r, &["migrate", "--reconcile", "--against", &against]);

    // then: it succeeds and surfaces R2289 as IN-BOTH and R9999 as a SOURCE-ONLY gap
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("in-both 1"), "summary was {s:?}");
    assert!(s.contains("source-only 1"), "summary was {s:?}");
}

#[test]
fn migrate_should_require_a_blame_fallback_when_a_source_lacks_authors() {
    // given: a store and a gitlog source whose records carry NO author, run WITHOUT --blame
    let r = repo();
    let src = write_source(&r, "gitlog", "no-authors.md", TWO_ROUNDS);

    // when: migrate runs with no --blame fallback
    let out = run(&r, &["migrate", "--source", &src]);

    // then: R5 stays intact — no author is fabricated, no tick is written, and the gap is surfaced
    assert!(out.status.success(), "a surfaced gap is not a hard failure");
    assert_eq!(tick_count(&r), 0, "no tick is written without an author");
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        s.contains("source-only gap") || s.contains("source-only"),
        "the gap must be surfaced; summary was {s:?}"
    );
}

#[test]
fn migrate_dry_run_should_write_no_tick_when_asked_to_preview() {
    // given: a store and a 2-record source
    let r = repo();
    let src = write_source(&r, "gitlog", "chat-room.md", TWO_ROUNDS);

    // when: migrate runs with --dry-run
    let out = run(
        &r,
        &[
            "migrate",
            "--source",
            &src,
            "--blame",
            "Wang Yu",
            "--dry-run",
        ],
    );

    // then: it reports what WOULD import but writes nothing
    assert!(out.status.success());
    assert_eq!(tick_count(&r), 0, "--dry-run writes no ticks");
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        s.contains("imported 2"),
        "preview should count both; was {s:?}"
    );
}

#[test]
fn migrate_bind_check_should_print_a_harvested_check_when_a_selector_is_given() {
    // given: a store
    let r = repo();

    // when: migrate --bind-check harvests a test with full liveness (no counter-test)
    let out = run(
        &r,
        &[
            "migrate",
            "--bind-check",
            "pytest tests/test_invariant_no_redis.py",
            "--on-platform",
            "linux-ci",
            "--triggered-by",
            "pyproject.toml",
            "--surface",
            "pyproject-deps",
            "--verified-at-sha",
            "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901",
        ],
    );

    // then: it succeeds and prints the harvested (counter-test-less) binding honestly
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("harvested"), "output was {s:?}");
    assert!(
        s.contains("pytest tests/test_invariant_no_redis.py"),
        "output was {s:?}"
    );
}

#[test]
fn migrate_should_tag_an_imported_decision_from_the_jurisdiction_map() {
    // given: a store, a 1-record gitlog source (R2289), and a map line tagging R2289 as C
    let r = repo();
    let src = write_source(
        &r,
        "gitlog",
        "chat-room.md",
        "## R2289 restore-safety counter DB-backed\n",
    );
    let map = write_map(&r, "jurisdiction.map", "# round -> bucket\nR2289 C\n");

    // when: migrate imports it WITH the --jurisdiction-map
    let out = run(
        &r,
        &[
            "migrate",
            "--source",
            &src,
            "--blame",
            "Wang Yu",
            "--jurisdiction-map",
            &map,
        ],
    );

    // then: it succeeds and the imported decision carries jurisdiction=C on both list and show
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let list = run(&r, &["list"]);
    let l = String::from_utf8_lossy(&list.stdout);
    assert!(
        l.contains("jurisdiction=C"),
        "list did not render the imported jurisdiction: {l:?}"
    );
    // the id leads the list row — pull it and confirm `show` agrees (the from_value round-trip)
    let id = l
        .lines()
        .find(|line| line.contains("jurisdiction=C"))
        .and_then(|line| line.split('\t').next())
        .expect("a row with the tagged decision");
    let show = run(&r, &["show", id]);
    assert!(
        String::from_utf8_lossy(&show.stdout).contains("jurisdiction: C"),
        "show did not render jurisdiction: {}",
        String::from_utf8_lossy(&show.stdout)
    );
}

#[test]
fn migrate_should_leave_a_decision_untagged_when_its_key_is_absent_from_the_map() {
    // given: a store, a 1-record source (R2290), and a map that names a DIFFERENT key only
    let r = repo();
    let src = write_source(
        &r,
        "gitlog",
        "chat-room.md",
        "## R2290 ship the cross-pod drain\n",
    );
    let map = write_map(&r, "jurisdiction.map", "R9999 C\n");

    // when: migrate imports it with the map (whose only entry does not match R2290)
    let out = run(
        &r,
        &[
            "migrate",
            "--source",
            &src,
            "--blame",
            "Wang Yu",
            "--jurisdiction-map",
            &map,
        ],
    );

    // then: it succeeds and the imported decision is UNTAGGED (purely additive — absent key ⇒ None)
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let list = run(&r, &["list"]);
    assert!(
        !String::from_utf8_lossy(&list.stdout).contains("jurisdiction="),
        "an unmapped key must import untagged: {}",
        String::from_utf8_lossy(&list.stdout)
    );
}

#[test]
fn migrate_should_reject_an_out_of_vocab_bucket_in_the_jurisdiction_map() {
    // given: a store, a source, and a map whose bucket is outside the {A,B,C,D} vocabulary
    let r = repo();
    let src = write_source(
        &r,
        "gitlog",
        "chat-room.md",
        "## R2289 restore-safety counter DB-backed\n",
    );
    let map = write_map(&r, "jurisdiction.map", "R2289 Z\n");

    // when: migrate runs with that map
    let out = run(
        &r,
        &[
            "migrate",
            "--source",
            &src,
            "--blame",
            "Wang Yu",
            "--jurisdiction-map",
            &map,
        ],
    );

    // then: it is a hard error (out-of-vocab bucket), names the offending line, and writes nothing
    assert!(!out.status.success(), "an out-of-vocab bucket must fail");
    assert_eq!(tick_count(&r), 0, "no tick is written on a bad map");
    let e = String::from_utf8_lossy(&out.stderr);
    assert!(
        e.contains("R2289 Z") || e.contains("R2289"),
        "the error should name the offending line: {e:?}"
    );
}

#[test]
fn migrate_should_still_skip_a_tagged_record_on_a_re_run_because_jurisdiction_is_non_hashed() {
    // given: a store with R2289 imported once, tagged C from the map
    let r = repo();
    let src = write_source(
        &r,
        "gitlog",
        "chat-room.md",
        "## R2289 restore-safety counter DB-backed\n",
    );
    let map = write_map(&r, "jurisdiction.map", "R2289 C\n");
    let args = [
        "migrate",
        "--source",
        &src,
        "--blame",
        "Wang Yu",
        "--jurisdiction-map",
        &map,
    ];
    assert!(run(&r, &args).status.success());
    assert_eq!(tick_count(&r), 1, "first import writes the record");

    // when: the SAME tagged migrate runs again (jurisdiction is non-hashed ⇒ the id is unchanged)
    let second = run(&r, &args);

    // then: it is idempotent — nothing new is written and the record is reported skipped
    assert!(second.status.success());
    assert_eq!(tick_count(&r), 1, "a re-run writes no new ticks");
    let s = String::from_utf8_lossy(&second.stdout);
    assert!(
        s.contains("imported 0") && s.contains("skipped 1"),
        "summary was {s:?}"
    );
}

#[test]
fn migrate_should_round_trip_clean_through_verify_when_records_are_imported() {
    // given: a store and a 2-record source imported with a fallback author
    let r = repo();
    let src = write_source(&r, "gitlog", "chat-room.md", TWO_ROUNDS);
    assert!(
        run(&r, &["migrate", "--source", &src, "--blame", "Wang Yu"])
            .status
            .success()
    );

    // when: the store is verified
    let v = run(&r, &["verify"]);

    // then: the migrated chain passes verify (id == hash, lineage forward-only, schema closed)
    assert!(
        v.status.success(),
        "verify failed: {}",
        String::from_utf8_lossy(&v.stderr)
    );
}
