use evolving::verify::{EvRef, Liveness};

fn class(raw: &str) -> &'static str {
    Liveness::of(&EvRef::parse(raw).unwrap()).as_str()
}

#[test]
fn liveness_should_be_content_when_the_ref_carries_a_passline() {
    assert_eq!(class("file:src/foo.rs::fn bar()"), "content");
    assert_eq!(class("test:tests/x.rs::assert_eq!"), "content");
    assert_eq!(class("artifact:out.bin::MAGIC"), "content");
}

#[test]
fn liveness_should_be_existence_when_a_path_ref_carries_no_passline() {
    assert_eq!(class("file:src/foo.rs"), "existence");
    assert_eq!(class("test:tests/x.rs"), "existence");
    assert_eq!(class("artifact:out.bin"), "existence");
}

#[test]
fn liveness_should_be_immutable_for_a_commit_ref() {
    assert_eq!(
        class("commit:0e046b9cb51d261426f78796e2a9478d3cb846e6"),
        "immutable"
    );
}

#[test]
fn liveness_should_be_asserted_for_metric_and_url_refs() {
    assert_eq!(class("metric:coverage=0.91"), "asserted");
    assert_eq!(class("url:https://example.com/spec"), "asserted");
}

#[test]
fn why_should_state_that_an_asserted_anchor_cannot_fail() {
    let w = Liveness::Asserted.why();
    assert!(
        w.contains("cannot fail"),
        "asserted must be honest about never going red: {w}"
    );
}
