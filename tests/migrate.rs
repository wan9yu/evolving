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

// One Canonical Decision Intake line: a user ruling carrying its author + an opaque source_ref.
const CANONICAL_RULING: &str = "{\"kind\":\"ev-decision-intake\",\
\"decision\":\"rate-limit lives at the edge proxy\",\
\"grounds\":[{\"claim\":\"the edge sees every request first\",\"supports\":\"chosen\"},\
{\"claim\":\"the app tier double-counts\",\"supports\":\"rejected:app-tier\"}],\
\"blame\":\"Wang Yu\",\"authority\":\"user-ruled\",\"source_ref\":\"R1043\"}\n";

#[test]
fn migrate_should_ingest_a_canonical_jsonl_source_through_the_shared_backfill() {
    // given: a store and a canonical decision-intake JSONL source (the format-neutral primary intake)
    let r = repo();
    let src = write_source(&r, "canonical", "intake.jsonl", CANONICAL_RULING);

    // when: migrate ingests it through the shared idempotent backfill
    let out = run(&r, &["migrate", "--source", &src]);

    // then: it succeeds and writes the one decision (one hashing path, like every other source)
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(tick_count(&r), 1, "the canonical record is imported");
    let v = run(&r, &["verify"]);
    assert!(
        v.status.success(),
        "the imported canonical chain must verify clean: {}",
        String::from_utf8_lossy(&v.stderr)
    );
}

#[test]
fn migrate_canonical_should_be_idempotent_on_source_ref_when_a_line_is_reingested() {
    // given: a canonical source imported once, keyed on its opaque source_ref
    let r = repo();
    let src = write_source(&r, "canonical", "intake.jsonl", CANONICAL_RULING);
    assert!(run(&r, &["migrate", "--source", &src]).status.success());
    assert_eq!(tick_count(&r), 1);

    // when: the SAME canonical line is ingested again
    let second = run(&r, &["migrate", "--source", &src]);

    // then: it is idempotent on the source_ref — nothing new written, the record reported skipped
    assert!(second.status.success());
    assert_eq!(tick_count(&r), 1, "a re-run writes no new ticks");
    let s = String::from_utf8_lossy(&second.stdout);
    assert!(
        s.contains("imported 0") && s.contains("skipped 1"),
        "summary was {s:?}"
    );
}

#[test]
fn migrate_canonical_should_stamp_authority_user_ruled_so_the_ruling_surfaces_in_brief() {
    // given: a canonical user ruling imported (carrying authority=user-ruled inline) — the two-link fix
    let r = repo();
    let src = write_source(&r, "canonical", "intake.jsonl", CANONICAL_RULING);
    assert!(
        run(&r, &["migrate", "--source", &src]).status.success(),
        "the canonical ruling imports"
    );

    // when: a fresh agent runs the boot-read
    let brief = run(&r, &["brief"]);

    // then: the imported ruling SURFACES (authority carried inline → it reaches the user-ruled boot-read,
    // closing the chain the old hardcoded-None migrate path silently broke)
    assert!(brief.status.success());
    let b = String::from_utf8_lossy(&brief.stdout);
    assert!(
        b.contains("rate-limit lives at the edge proxy") && b.contains("[user-ruled]"),
        "the imported user ruling must surface in brief; was {b:?}"
    );
}

// An agent-PROPOSED ruling that even arrives marked user-ruled — the danger case §五 names: an agent's
// invented ruling that could govern a fresh fleet before any human vouches for it.
const AGENT_PROPOSED_USER_RULED: &str = "{\"kind\":\"ev-decision-intake\",\
\"decision\":\"agent says rip out the rate limiter\",\
\"grounds\":[{\"claim\":\"it looked unused to me\",\"supports\":\"chosen\"}],\
\"blame\":\"agent-runner\",\"authority\":\"user-ruled\",\"provenance\":\"agent-proposed\",\"source_ref\":\"R-agent-1\"}\n";

#[test]
fn brief_should_exclude_an_agent_proposed_ruling_even_when_it_arrives_user_ruled() {
    // given: an agent-proposed ruling ingested while marked user-ruled (the §五 belt-and-suspenders case)
    let r = repo();
    let src = write_source(&r, "canonical", "agent.jsonl", AGENT_PROPOSED_USER_RULED);
    assert!(
        run(&r, &["migrate", "--source", &src]).status.success(),
        "the agent-proposed ruling ingests as a record"
    );

    // when: a fresh agent runs the boot-read in both forms
    let text = run(&r, &["brief"]);
    let json = run(&r, &["brief", "--json"]);

    // then: it NEVER governs — neither the human text nor the machine JSON surfaces it. An
    // agent-proposed proposal cannot reach a fresh agent before a named human vouches for it.
    assert!(text.status.success() && json.status.success());
    let t = String::from_utf8_lossy(&text.stdout);
    assert!(
        !t.contains("rip out the rate limiter"),
        "agent-proposed must not surface in the text brief; was {t:?}"
    );
    let v: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(
        v["decisions"].as_array().unwrap().len(),
        0,
        "agent-proposed must not surface in brief --json; was {v}"
    );
}

#[test]
fn migrate_canonical_should_report_a_source_only_gap_when_a_line_has_no_blame_and_no_fallback() {
    // given: a canonical line carrying NO blame, ingested WITHOUT a --blame fallback
    let r = repo();
    let no_author = "{\"kind\":\"ev-decision-intake\",\"decision\":\"x\",\"grounds\":[],\"source_ref\":\"R7\"}\n";
    let src = write_source(&r, "canonical", "intake.jsonl", no_author);

    // when: migrate ingests it with no author available
    let out = run(&r, &["migrate", "--source", &src]);

    // then: R5 stays intact — no author invented, no tick written, the gap surfaced
    assert!(out.status.success(), "a surfaced gap is not a hard failure");
    assert_eq!(tick_count(&r), 0, "no tick is written without an author");
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        s.contains("source-only"),
        "the gap must be surfaced; was {s:?}"
    );
}

#[test]
fn migrate_should_reject_a_canonical_line_with_an_unknown_kind() {
    // given: a JSONL line whose envelope kind is not ev-decision-intake (a mis-piped file)
    let r = repo();
    let bad = "{\"kind\":\"notes\",\"decision\":\"x\",\"grounds\":[]}\n";
    let src = write_source(&r, "canonical", "intake.jsonl", bad);

    // when: migrate ingests it
    let out = run(&r, &["migrate", "--source", &src]);

    // then: it is a hard error and writes nothing (the wire envelope is strict, not tolerant)
    assert!(!out.status.success(), "an unknown kind must fail loudly");
    assert_eq!(tick_count(&r), 0, "nothing is written on a rejected source");
}

// A canonical line carrying a Test check, parameterized by the bits the ingest gates act on:
// `extra_tags` splices in provenance/jurisdiction; `counter` is "" (harvested) or a counter_test field.
fn canonical_with_test(extra_tags: &str, counter: &str) -> String {
    format!(
        "{{\"kind\":\"ev-decision-intake\",\"decision\":\"keep the schema frozen\",\
\"grounds\":[{{\"claim\":\"the frozen schema still holds\",\"supports\":\"chosen\",\
\"check\":{{\"by\":\"test\",\"ref\":\"pytest test_schema.py\",\
\"verified_at_sha\":\"d308afac1b2c3d4e5f60718293a4b5c6d7e8f901\"{counter},\
\"liveness\":{{\"platforms\":[\"linux-ci\"],\"triggered_by\":[\"schema.sql\"],\"surfaces\":[\"schema-ddl\"]}}}}}}],\
\"blame\":\"Wang Yu\",\"source_ref\":\"R5\"{extra_tags}}}\n"
    )
}

#[test]
fn ingest_should_accept_a_harvested_check_when_provenance_is_imported() {
    // given: an imported record whose Test check carries NO counter-test (a harvested binding)
    let r = repo();
    let body = canonical_with_test(",\"provenance\":\"imported\"", "");
    let src = write_source(&r, "canonical", "intake.jsonl", &body);

    // when: migrate ingests it
    let out = run(&r, &["migrate", "--source", &src]);

    // then: it is accepted — imported history may carry a harvested (falsifiability-unproven) binding
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(tick_count(&r), 1);
}

#[test]
fn ingest_should_refuse_a_harvested_check_when_provenance_is_agent_proposed() {
    // given: an agent-proposed record whose Test check carries NO counter-test
    let r = repo();
    let body = canonical_with_test(",\"provenance\":\"agent-proposed\"", "");
    let src = write_source(&r, "canonical", "intake.jsonl", &body);

    // when: migrate ingests it
    let out = run(&r, &["migrate", "--source", &src]);

    // then: it is refused — a fresh agent binding must prove falsifiability with a counter-test
    assert!(!out.status.success(), "an agent-proposed harvest must fail");
    assert_eq!(tick_count(&r), 0, "nothing is written on a refused record");
}

#[test]
fn ingest_should_accept_an_agent_proposed_binding_when_it_carries_a_counter_test() {
    // given: an agent-proposed record whose Test check carries a counter-test (falsifiability proven)
    let r = repo();
    let body = canonical_with_test(
        ",\"provenance\":\"agent-proposed\"",
        ",\"counter_test\":\"pytest test_schema.py::test_change_flips_red\"",
    );
    let src = write_source(&r, "canonical", "intake.jsonl", &body);

    // when: migrate ingests it
    let out = run(&r, &["migrate", "--source", &src]);

    // then: it is accepted — an agent binding with a counter-test is exactly as sound as decide/guard
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(tick_count(&r), 1);
}

#[test]
fn ingest_should_refuse_a_c_jurisdiction_record_that_carries_a_test_check() {
    // given: a C-jurisdiction (detect-only) record that carries a fully-proven Test check
    let r = repo();
    let body = canonical_with_test(
        ",\"jurisdiction\":\"C\",\"provenance\":\"imported\"",
        ",\"counter_test\":\"pytest test_schema.py::test_change_flips_red\"",
    );
    let src = write_source(&r, "canonical", "intake.jsonl", &body);

    // when: migrate ingests it
    let out = run(&r, &["migrate", "--source", &src]);

    // then: it is refused at the door — a detect-only decision must hold no runnable test binding
    assert!(!out.status.success(), "a C/D test binding must fail");
    assert_eq!(tick_count(&r), 0, "nothing is written on a refused record");
}

#[test]
fn migrate_canonical_should_default_provenance_to_imported_when_a_line_omits_it() {
    // given: a canonical ruling that declares NO provenance (the migrate verb backfills history)
    let r = repo();
    let src = write_source(&r, "canonical", "intake.jsonl", CANONICAL_RULING);

    // when: migrate imports it
    assert!(run(&r, &["migrate", "--source", &src]).status.success());

    // then: the on-disk tick is stamped provenance=imported (so verify treats its text as transcribed)
    let id = std::fs::read_dir(r.join(".evolving/ticks"))
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().into_string().unwrap())
        .find(|n| n.len() == 12)
        .expect("one imported tick");
    let raw = std::fs::read_to_string(r.join(".evolving/ticks").join(&id)).unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        v.get("provenance").and_then(|x| x.as_str()),
        Some("imported"),
        "an undeclared canonical import defaults to imported; tick was {v}"
    );
}

#[test]
fn reconcile_should_accept_a_canonical_source_and_report_the_capture_gap() {
    // given: a store seeded with the R1043 ruling (via canonical import)
    let r = repo();
    let seed = write_source(&r, "canonical", "seed.jsonl", CANONICAL_RULING);
    assert!(run(&r, &["migrate", "--source", &seed]).status.success());

    // and: a canonical source that ALSO declares an uncaptured ruling R9999
    let extra = "{\"kind\":\"ev-decision-intake\",\"decision\":\"a ruling never captured\",\
\"grounds\":[],\"blame\":\"Wang Yu\",\"source_ref\":\"R9999\"}\n";
    let against = write_source(
        &r,
        "canonical",
        "against.jsonl",
        &format!("{CANONICAL_RULING}{extra}"),
    );

    // when: reconcile joins the canonical source against the store
    let out = run(&r, &["migrate", "--reconcile", "--against", &against]);

    // then: it succeeds and surfaces R1043 as IN-BOTH and R9999 as a SOURCE-ONLY capture gap
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
fn ingest_should_error_when_an_inline_jurisdiction_conflicts_with_the_jurisdiction_map() {
    // given: a canonical record declaring jurisdiction=C inline, and a map tagging the SAME key as D
    let r = repo();
    let body = "{\"kind\":\"ev-decision-intake\",\"decision\":\"x\",\"grounds\":[],\
\"blame\":\"Wang Yu\",\"jurisdiction\":\"C\",\"source_ref\":\"R1043\"}\n";
    let src = write_source(&r, "canonical", "intake.jsonl", body);
    let map = write_map(&r, "jurisdiction.map", "R1043 D\n");

    // when: migrate ingests it with the conflicting map
    let out = run(
        &r,
        &["migrate", "--source", &src, "--jurisdiction-map", &map],
    );

    // then: it is a hard error (two sources of truth disagree) and writes nothing
    assert!(!out.status.success(), "a jurisdiction conflict must fail");
    assert_eq!(tick_count(&r), 0, "nothing is written on a conflict");
    let e = String::from_utf8_lossy(&out.stderr);
    assert!(
        e.contains("conflicts"),
        "the error should name the conflict: {e:?}"
    );
}

#[test]
fn ingest_should_let_an_inline_jurisdiction_agree_with_a_matching_map_entry() {
    // given: a canonical record declaring jurisdiction=C inline, and a map tagging the SAME key C too
    let r = repo();
    let body = "{\"kind\":\"ev-decision-intake\",\"decision\":\"x\",\"grounds\":[],\
\"blame\":\"Wang Yu\",\"jurisdiction\":\"C\",\"source_ref\":\"R1043\"}\n";
    let src = write_source(&r, "canonical", "intake.jsonl", body);
    let map = write_map(&r, "jurisdiction.map", "R1043 C\n");

    // when: migrate ingests it with the agreeing map
    let out = run(
        &r,
        &["migrate", "--source", &src, "--jurisdiction-map", &map],
    );

    // then: agreement is not a conflict — the record imports tagged C
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let list = run(&r, &["list"]);
    assert!(
        String::from_utf8_lossy(&list.stdout).contains("jurisdiction=C"),
        "list did not render the agreed jurisdiction"
    );
}

#[test]
fn canonical_ingest_of_the_genesis_payload_with_non_hashed_extras_still_computes_the_frozen_id() {
    // given: a canonical line whose HASHED payload is exactly the genesis golden, plus every non-hashed
    // extra the contract carries (source_ref + provenance=imported + jurisdiction=C, legal with a person
    // check). The trust boundary: ev computes parent_id=HEAD("") and the same id regardless of the extras.
    let r = repo();
    let genesis = "{\"kind\":\"ev-decision-intake\",\
\"decision\":\"freeze the retrieval schema for v2\",\
\"observe\":\"evaluating retrieval backend\",\
\"grounds\":[{\"claim\":\"team still wants a frozen schema\",\"supports\":\"chosen\",\
\"check\":{\"by\":\"person\",\"ref\":\"Q3 infra review\"}},\
{\"claim\":\"pgvector would lock our schema\",\"supports\":\"rejected:pgvector\"}],\
\"blame\":\"Wang Yu\",\"source_ref\":\"R-genesis\",\"provenance\":\"imported\",\"jurisdiction\":\"C\"}\n";
    let src = write_source(&r, "canonical", "genesis.jsonl", genesis);

    // when: it is ingested onto an empty store (so parent_id == "")
    let out = run(&r, &["migrate", "--source", &src]);

    // then: the written tick is the FROZEN genesis golden id — the non-hashed extras never move it
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        r.join(".evolving/ticks/e2b337f53a1f").exists(),
        "the canonical path must compute the same frozen genesis id; ticks: {:?}",
        std::fs::read_dir(r.join(".evolving/ticks"))
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect::<Vec<_>>()
    );
}

// The same decision, first imported with authority omitted, then corrected to user-ruled.
const RULING_NO_AUTH: &str = "{\"kind\":\"ev-decision-intake\",\"decision\":\"#247/#1458 Insights scope\",\"grounds\":[],\"blame\":\"Mac\",\"source_ref\":\"#247/#1458\",\"provenance\":\"imported\"}\n";
const RULING_USER_RULED: &str = "{\"kind\":\"ev-decision-intake\",\"decision\":\"#247/#1458 Insights scope\",\"grounds\":[],\"blame\":\"Mac\",\"authority\":\"user-ruled\",\"source_ref\":\"#247/#1458\",\"provenance\":\"imported\"}\n";

fn single_tick_id(repo: &std::path::Path) -> String {
    std::fs::read_dir(repo.join(".evolving/ticks"))
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().into_string().unwrap())
        .find(|n| n.len() == 12)
        .expect("exactly one tick")
}

#[test]
fn migrate_should_report_a_discrepancy_when_a_re_import_corrects_a_tag_but_never_apply_it() {
    // given: a store holding #247/#1458 imported with authority OMITTED (the gateway stale tick)
    let r = repo();
    let p1 = write_source(&r, "canonical", "p1.jsonl", RULING_NO_AUTH);
    assert!(run(&r, &["migrate", "--source", &p1]).status.success());
    let id = single_tick_id(&r);
    let before = std::fs::read_to_string(r.join(".evolving/ticks").join(&id)).unwrap();

    // when: the SAME decision is re-imported CORRECTED to authority=user-ruled
    let p2 = write_source(&r, "canonical", "p2.jsonl", RULING_USER_RULED);
    let out = run(&r, &["migrate", "--source", &p2]);

    // then: the difference is SURFACED loudly (never the silent skip), and the stored tick is
    // BYTE-UNCHANGED — migrate reports the discrepancy, it does not rewrite an immutable tick.
    assert!(out.status.success());
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        said.contains("discrepancy") && said.contains("authority"),
        "the corrected authority must surface as a discrepancy; was {said:?}"
    );
    assert!(said.contains("imported 0") && said.contains("skipped 1"));
    let after = std::fs::read_to_string(r.join(".evolving/ticks").join(&id)).unwrap();
    assert_eq!(before, after, "migrate must never rewrite a tick in place");
    assert_eq!(
        tick_count(&r),
        1,
        "no new tick is written on a discrepancy-skip"
    );
}

#[test]
fn migrate_should_not_report_a_discrepancy_when_a_re_import_is_identical() {
    // given: a store holding a record imported WITH authority=user-ruled
    let r = repo();
    let p1 = write_source(&r, "canonical", "p1.jsonl", RULING_USER_RULED);
    assert!(run(&r, &["migrate", "--source", &p1]).status.success());

    // when: the IDENTICAL record is re-imported (provenance defaults to imported on both passes)
    let out = run(&r, &["migrate", "--source", &p1]);

    // then: no discrepancy — the compare runs against the RESOLVED tags, so imported-vs-None never false-fires
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !said.contains("discrepancy"),
        "an identical re-import is clean; was {said:?}"
    );
}

#[test]
fn migrate_should_report_a_discrepancy_on_the_dry_run_path_too() {
    // given: a stale authority-omitted tick
    let r = repo();
    let p1 = write_source(&r, "canonical", "p1.jsonl", RULING_NO_AUTH);
    assert!(run(&r, &["migrate", "--source", &p1]).status.success());

    // when: the corrected record is PREVIEWED with --dry-run
    let p2 = write_source(&r, "canonical", "p2.jsonl", RULING_USER_RULED);
    let out = run(&r, &["migrate", "--source", &p2, "--dry-run"]);

    // then: dry-run is not blind — it surfaces the pending discrepancy before any commit
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        said.contains("discrepancy"),
        "dry-run must surface the pending discrepancy; was {said:?}"
    );
}

#[test]
fn migrate_should_not_silently_double_import_a_within_pass_duplicate_source_key() {
    // given: TWO canonical records in ONE pass sharing source_key R555 but differing on authority
    // (the gitlog-None vs to-human-user-ruled collision the audit flagged) — neither yet in the store
    let r = repo();
    let body = "{\"kind\":\"ev-decision-intake\",\"decision\":\"R555 ruling\",\"grounds\":[],\"blame\":\"Mac\",\"source_ref\":\"R555\"}\n\
{\"kind\":\"ev-decision-intake\",\"decision\":\"R555 ruling\",\"grounds\":[],\"blame\":\"Mac\",\"authority\":\"user-ruled\",\"source_ref\":\"R555\"}\n";
    let src = write_source(&r, "canonical", "dup.jsonl", body);

    // when: migrate imports them
    let out = run(&r, &["migrate", "--source", &src]);

    // then: it does NOT silently write two ticks for one key — the second routes into the skip arm and
    // the authority difference is surfaced as a discrepancy (one imported, one skipped+flagged)
    assert!(out.status.success());
    assert_eq!(
        tick_count(&r),
        1,
        "a within-pass duplicate key must not double-import"
    );
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        said.contains("imported 1") && said.contains("skipped 1") && said.contains("discrepancy"),
        "the within-pass collision must surface, not silently double-import; was {said:?}"
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
