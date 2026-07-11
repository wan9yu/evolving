use evolving::ledger::{Actor, ActorKind, Envelope};
use evolving::state::{fold, ClaimState};

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
fn a_bare_claim_folds_to_bare() {
    let events = vec![env(
        1,
        "clm_a",
        "claim",
        serde_json::json!({"label":"fixed X"}),
    )];
    let d = fold(&events);
    assert_eq!(d.claims.len(), 1);
    assert!(matches!(d.claims[0].state, ClaimState::Bare));
    assert_eq!(d.claims[0].label, "fixed X");
}

#[test]
fn evidence_moves_a_claim_out_of_bare() {
    let events = vec![
        env(1, "clm_a", "claim", serde_json::json!({"label":"fixed X"})),
        env(
            2,
            "evd_a",
            "evidence",
            serde_json::json!({"claim":"clm_a","ref":"commit:abc","status":"verified","self_evident":false}),
        ),
    ];
    let d = fold(&events);
    assert!(matches!(d.claims[0].state, ClaimState::Anchored));
    assert_eq!(d.claims[0].evidence.len(), 1);
    assert!(!d.claims[0].self_evident);
}

#[test]
fn a_closed_claim_leaves_the_open_list() {
    let events = vec![
        env(1, "clm_a", "claim", serde_json::json!({"label":"x"})),
        env(
            2,
            "evd_a",
            "evidence",
            serde_json::json!({"claim":"clm_a","ref":"commit:abc","status":"verified"}),
        ),
        env(3, "cls_a", "close", serde_json::json!({"claim":"clm_a"})),
    ];
    let d = fold(&events);
    assert_eq!(d.claims.len(), 0);
    assert_eq!(d.closed.len(), 1);
}

#[test]
fn a_held_claim_is_grey() {
    let events = vec![
        env(1, "clm_a", "claim", serde_json::json!({"label":"x"})),
        env(
            2,
            "hld_a",
            "hold",
            serde_json::json!({"claim":"clm_a","reason":"waiting on upstream"}),
        ),
    ];
    let d = fold(&events);
    assert_eq!(d.grey.len(), 1);
    assert_eq!(d.grey[0].reason.as_deref(), Some("waiting on upstream"));
    assert_eq!(
        d.claims.len(),
        0,
        "a grey claim must not also appear in the open list"
    );
}

#[test]
fn a_demanded_claim_that_gains_evidence_is_a_returned_demand() {
    let events = vec![
        env(1, "clm_a", "claim", serde_json::json!({"label":"x"})),
        env(2, "dmd_a", "demand", serde_json::json!({"claim":"clm_a"})),
        env(
            3,
            "evd_a",
            "evidence",
            serde_json::json!({"claim":"clm_a","ref":"commit:abc","status":"verified"}),
        ),
    ];
    let d = fold(&events);
    assert_eq!(d.demands_returned.len(), 1);
    assert_eq!(d.demands_returned[0].id, "clm_a");
}

#[test]
fn a_legacy_verified_status_reads_as_resolves() {
    // ledgers are append-only: events written before the anchor-resolution
    // rename still carry "verified" — the fold normalizes, never rewrites.
    let events = vec![
        env(
            1,
            "clm_a",
            "claim",
            serde_json::json!({"label":"old ledger"}),
        ),
        env(
            2,
            "evd_a",
            "evidence",
            serde_json::json!({"claim":"clm_a","ref":"commit:abc","status":"verified"}),
        ),
    ];
    let d = fold(&events);
    assert!(matches!(d.claims[0].state, ClaimState::Anchored));
    assert_eq!(d.claims[0].evidence[0].status, "resolves");
}
