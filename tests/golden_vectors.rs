//! The two frozen golden vectors. If serde_json's canonicalization ever diverges
//! from JCS for our string-only payload, these fail — the byte-stability anchor.
use ev::canonical::compute_id;
use ev::tick::{Check, Ground, Liveness, Tick};

fn book(parent: &str, observe: &str, decision: &str, grounds: Vec<Ground>) -> Tick {
    Tick {
        id: String::new(),
        parent_id: parent.into(),
        observe: observe.into(),
        decision: decision.into(),
        grounds,
        status: "live".into(),
        held_since: "".into(),
        blame: "Wang Yu".into(),
        authority: None,
        jurisdiction: None,
    }
}

#[test]
fn compute_id_should_return_the_frozen_golden_id_when_given_the_genesis_tick() {
    // given: the genesis tick with its frozen field values
    let t = book(
        "",
        "evaluating retrieval backend",
        "freeze the retrieval schema for v2",
        vec![
            Ground {
                claim: "team still wants a frozen schema".into(),
                supports: "chosen".into(),
                check: Some(Check::Person {
                    reference: "Q3 infra review".into(),
                }),
            },
            Ground {
                claim: "pgvector would lock our schema".into(),
                supports: "rejected:pgvector".into(),
                check: None,
            },
        ],
    );

    // when: its canonical id is computed
    let id = compute_id(&t);

    // then: it matches the frozen golden id
    assert_eq!(id, "e2b337f53a1f");
}

#[test]
fn compute_id_should_freeze_the_harvested_binding_id_when_counter_test_is_absent() {
    // given: a harvested test binding — full 3-key liveness present, but no counter_test
    // (the migrate path can only half-prove falsifiability), with the case1 lineage/text.
    let t = book(
        "7b21f0a4c8de",
        "multi-pod restore-safety counter — chat-room R2289→R2290",
        "restore-safety counter DB-backed; reject Redis",
        vec![
            Ground {
                claim: "Argus introduces no Redis; multi-pod coord via existing DB".into(),
                supports: "chosen".into(),
                check: Some(Check::Test {
                    reference: "pytest tests/test_redis_absent.py".into(),
                    verified_at_sha: "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901".into(),
                    counter_test: None,
                    liveness: Liveness {
                        platforms: vec!["linux-ci".into()],
                        triggered_by: vec!["pyproject.toml".into()],
                        surfaces: vec!["pyproject-deps".into()],
                    },
                }),
            },
            Ground {
                claim: "team still wants 0-Redis posture".into(),
                supports: "chosen".into(),
                check: Some(Check::Person {
                    reference: "Q3 infra review".into(),
                }),
            },
            Ground {
                claim: "Redis would add a new infra dependency".into(),
                supports: "rejected:Redis".into(),
                check: None,
            },
        ],
    );

    // when: its canonical id is computed
    let id = compute_id(&t);

    // then: it matches the frozen harvested golden id (counter_test key omitted from the payload)
    assert_eq!(id, "0cf784b51331");
}

#[test]
fn compute_id_should_return_the_frozen_golden_id_when_given_the_case1_tick_with_non_ascii() {
    // given: the case1 tick whose fields carry non-ascii content
    let t = book(
        "7b21f0a4c8de",
        "multi-pod restore-safety counter — chat-room R2289→R2290",
        "restore-safety counter DB-backed; reject Redis",
        vec![
            Ground {
                claim: "Argus introduces no Redis; multi-pod coord via existing DB".into(),
                supports: "chosen".into(),
                check: Some(Check::Test {
                    reference: "pytest tests/test_redis_absent.py".into(),
                    verified_at_sha: "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901".into(),
                    counter_test: Some(
                        "pytest tests/test_redis_absent.py::test_redis_injection_flips_red".into(),
                    ),
                    liveness: Liveness {
                        platforms: vec!["linux-ci".into()],
                        triggered_by: vec!["pyproject.toml".into()],
                        surfaces: vec!["pyproject-deps".into()],
                    },
                }),
            },
            Ground {
                claim: "team still wants 0-Redis posture".into(),
                supports: "chosen".into(),
                check: Some(Check::Person {
                    reference: "Q3 infra review".into(),
                }),
            },
            Ground {
                claim: "Redis would add a new infra dependency".into(),
                supports: "rejected:Redis".into(),
                check: None,
            },
        ],
    );

    // when: its canonical id is computed
    let id = compute_id(&t);

    // then: it matches the frozen golden id
    assert_eq!(id, "638c47b0c9dd");
}
