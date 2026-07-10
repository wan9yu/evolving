use evolving::verify::{verify_ref, EvRef};
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
    assert_eq!(verify_ref(&r, &d), "verified");
}

#[test]
fn a_missing_file_is_unreachable() {
    let d = tmp();
    let r = EvRef::parse("file:nope.log").unwrap();
    assert_eq!(verify_ref(&r, &d), "unreachable");
}

#[test]
fn a_passline_that_matches_verifies_and_one_that_misses_fails() {
    let d = tmp();
    fs::write(d.join("t.log"), "running\ntest_foo ... ok\ndone\n").unwrap();
    let hit = EvRef::parse("test:t.log::test_foo ... ok").unwrap();
    assert_eq!(verify_ref(&hit, &d), "verified");
    let miss = EvRef::parse("test:t.log::test_bar ... ok").unwrap();
    assert_eq!(verify_ref(&miss, &d), "failed");
}
