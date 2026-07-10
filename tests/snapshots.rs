use evolving::ledger::{Actor, ActorKind, Envelope};
use evolving::state::fold;

fn env(seq: u64, id: &str, etype: &str, body: serde_json::Value) -> Envelope {
    Envelope {
        v: 2,
        id: id.into(),
        ts: format!("2020-01-01T00:00:{:02}Z", seq),
        writer: "w".into(),
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
fn a_bare_claim_past_two_boundaries_is_expired_bare() {
    let events = vec![
        env(1, "clm_a", "claim", serde_json::json!({"label":"x"})),
        env(2, "pau_1", "pause", serde_json::json!({"boundary":true})),
        env(3, "pau_2", "pause", serde_json::json!({"boundary":true})),
    ];
    let d = fold(&events);
    // the claim has been open across two boundaries with no evidence
    assert_eq!(d.claims[0].boundaries_open, 2);
    assert!(matches!(
        d.claims[0].state,
        evolving::state::ClaimState::ExpiredBare
    ));
}

#[test]
fn a_snapshot_counts_closes_not_in_a_prior_snapshot() {
    // the fold surfaces each snapshot's recorded delta as-is: one close before snap A (1), one before snap B (1)
    let events = vec![
        env(1, "clm_a", "claim", serde_json::json!({"label":"a"})),
        env(
            2,
            "evd_a",
            "evidence",
            serde_json::json!({"claim":"clm_a","ref":"commit:x","status":"verified"}),
        ),
        env(3, "cls_a", "close", serde_json::json!({"claim":"clm_a"})),
        env(
            4,
            "snp_1",
            "snapshot",
            serde_json::json!({"closed_with_evidence":1,"expired_bare":0}),
        ),
        env(5, "clm_b", "claim", serde_json::json!({"label":"b"})),
        env(
            6,
            "evd_b",
            "evidence",
            serde_json::json!({"claim":"clm_b","ref":"commit:y","status":"verified"}),
        ),
        env(7, "cls_b", "close", serde_json::json!({"claim":"clm_b"})),
        env(
            8,
            "snp_2",
            "snapshot",
            serde_json::json!({"closed_with_evidence":1,"expired_bare":0}),
        ),
    ];
    let d = fold(&events);
    assert_eq!(d.snapshots.len(), 2);
    assert_eq!(d.snapshots[0].closed_with_evidence, 1);
    assert_eq!(d.snapshots[1].closed_with_evidence, 1);
}
