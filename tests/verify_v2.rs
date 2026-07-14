use evolving::verify::{verify_ref, Commits, EvRef, Status};
use std::fs;

fn tmp() -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("ev-v2-{}", ulid::Ulid::new()));
    fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn an_existing_file_verifies() {
    let d = tmp();
    fs::write(d.join("out.log"), "all good\n").unwrap();
    let r = EvRef::parse("file:out.log").unwrap();
    assert_eq!(verify_ref(&r, &d, &Commits::none()), Status::Resolves);
}

#[test]
fn a_missing_file_is_gone() {
    let d = tmp();
    let r = EvRef::parse("file:nope.log").unwrap();
    // The container is absent, which is a fact about the tree. `Unreachable` is
    // reserved for a path ev can see but cannot read — a fact about ev's reach.
    assert_eq!(verify_ref(&r, &d, &Commits::none()), Status::Gone);
}

#[test]
fn a_passline_that_matches_resolves_and_one_that_misses_reads_changed() {
    let d = tmp();
    fs::write(d.join("t.log"), "running\ntest_foo ... ok\ndone\n").unwrap();
    let hit = EvRef::parse("test:t.log::test_foo ... ok").unwrap();
    assert_eq!(verify_ref(&hit, &d, &Commits::none()), Status::Resolves);
    let miss = EvRef::parse("test:t.log::test_bar ... ok").unwrap();
    // The file is there; the cited text is not — the line the anchor pointed at
    // changed. That is a different finding from the file being gone.
    assert_eq!(verify_ref(&miss, &d, &Commits::none()), Status::Changed);
}
