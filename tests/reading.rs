use evolving::ledger::{Actor, ActorKind, Envelope};
use evolving::reading::{Depth, Lang};
use evolving::state::fold;

fn env(seq: u64, id: &str, etype: &str, body: serde_json::Value) -> Envelope {
    Envelope {
        v: 2,
        id: id.into(),
        ts: format!("2020-01-01T00:00:{:02}Z", seq),
        writer: "w-0000".into(),
        seq,
        actor: Actor {
            kind: ActorKind::Human,
            id: None,
            via: None,
        },
        etype: etype.into(),
        body,
    }
}

#[test]
fn a_reading_slot_folds_onto_its_claim() {
    let events = vec![
        env(
            1,
            "clm_a",
            "claim",
            serde_json::json!({ "label": "fixed X" }),
        ),
        env(
            2,
            "rdg_a",
            "reading",
            serde_json::json!({ "claim": "clm_a", "depth": "plain", "lang": "zh", "ref": "thk_note1" }),
        ),
    ];
    let d = fold(&events);
    let r = &d.claims[0].reading;
    assert_eq!(r.get(Depth::Plain, Lang::Zh), Some("thk_note1"));
    assert_eq!(
        r.get(Depth::Plain, Lang::En),
        None,
        "the en slot was never filled — a fact"
    );
    assert_eq!(r.present(), 1);
    assert_eq!(
        r.empties().len(),
        3,
        "three of the four storable slots are empty"
    );
}

#[test]
fn refilling_a_slot_keeps_the_latest_and_never_rewrites() {
    // R4: two appends to the same slot. The fold shows the latest; both event bytes survive.
    let events = vec![
        env(1, "clm_a", "claim", serde_json::json!({ "label": "y" })),
        env(
            2,
            "rdg_a",
            "reading",
            serde_json::json!({ "claim": "clm_a", "depth": "ground", "lang": "en", "ref": "url:one" }),
        ),
        env(
            3,
            "rdg_b",
            "reading",
            serde_json::json!({ "claim": "clm_a", "depth": "ground", "lang": "en", "ref": "url:two" }),
        ),
    ];
    let d = fold(&events);
    assert_eq!(
        d.claims[0].reading.get(Depth::Ground, Lang::En),
        Some("url:two")
    );
    assert_eq!(
        d.claims[0].reading.present(),
        1,
        "a re-fill replaces, it does not add a slot"
    );
}

#[test]
fn a_concept_pointer_folds_into_concepts() {
    let events = vec![
        env(1, "clm_a", "claim", serde_json::json!({ "label": "y" })),
        env(
            2,
            "rdg_a",
            "reading",
            serde_json::json!({ "claim": "clm_a", "concept": "thk_basics" }),
        ),
    ];
    let d = fold(&events);
    assert_eq!(d.claims[0].reading.concepts, vec!["thk_basics".to_string()]);
    assert_eq!(
        d.claims[0].reading.present(),
        0,
        "a concept is not a grid slot"
    );
}
