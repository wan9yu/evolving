use evolving::ledger::{mint_id, Actor, ActorKind, Envelope};

#[test]
fn minted_id_carries_the_type_prefix() {
    let id = mint_id("claim");
    assert!(id.starts_with("clm_"), "got {id}");
    assert!(id.len() > 10);
}

#[test]
fn envelope_serializes_type_as_the_type_key() {
    let e = Envelope {
        v: 2,
        id: "clm_01JABC".into(),
        ts: "2020-01-01T00:00:00Z".into(),
        writer: "host-0000".into(),
        seq: 1,
        actor: Actor {
            kind: ActorKind::Agent,
            id: Some("cc".into()),
            via: None,
        },
        etype: "claim".into(),
        body: serde_json::json!({"label": "fixed X"}),
    };
    let s = serde_json::to_string(&e).unwrap();
    assert!(s.contains("\"type\":\"claim\""), "{s}");
    assert!(s.contains("\"kind\":\"agent\""), "{s}");
    // via is None -> omitted
    assert!(!s.contains("\"via\""), "{s}");
    // round-trips
    let back: Envelope = serde_json::from_str(&s).unwrap();
    assert_eq!(back.seq, 1);
}
