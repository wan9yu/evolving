//! Canonical-JSON (RFC 8785 / JCS) + content-addressed identity.
//! For our string-only payload, serde_json's default output (sorted BTreeMap
//! keys, compact separators, raw non-ASCII, no `/`-escape) IS JCS — verified by
//! the golden vectors. Liveness arrays are sorted+deduped here so set-valued
//! fields are order-insensitive; grounds[] keeps authored order.
use crate::tick::{Check, Tick};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

fn sorted_set(v: &[String]) -> Vec<String> {
    let mut s: Vec<String> = v.to_vec();
    s.sort(); // byte order == codepoint == UTF-16 order for the ASCII tokens we use
    s.dedup();
    s
}

fn check_value(c: &Check) -> Value {
    match c {
        Check::Person { reference } => json!({ "by": "person", "ref": reference }),
        Check::Test { reference, verified_at_sha, counter_test, liveness } => json!({
            "by": "test",
            "ref": reference,
            "verified_at_sha": verified_at_sha,
            "counter_test": counter_test,
            "liveness": {
                "platforms":    sorted_set(&liveness.platforms),
                "triggered_by": sorted_set(&liveness.triggered_by),
                "surfaces":     sorted_set(&liveness.surfaces),
            }
        }),
    }
}

/// The Value containing ONLY the hashed fields (decision, observe, grounds, parent_id).
pub fn hashed_value(t: &Tick) -> Value {
    let grounds: Vec<Value> = t
        .grounds
        .iter()
        .map(|g| {
            // Built as a manual Map (not json!) so an absent check OMITS the key
            // entirely — never serializes as null (design §4.8).
            let mut o = Map::new();
            o.insert("claim".into(), Value::String(g.claim.clone()));
            o.insert("supports".into(), Value::String(g.supports.clone()));
            if let Some(c) = &g.check {
                o.insert("check".into(), check_value(c));
            }
            Value::Object(o)
        })
        .collect();
    json!({
        "decision": t.decision,
        "observe": t.observe,
        "grounds": grounds,
        "parent_id": t.parent_id,
    })
}

/// Canonical bytes for our **string-only** hashed `Value`. serde_json's compact
/// output over a sorted-key `Value` equals RFC-8785/JCS *only* because the payload
/// contains no numbers/bools/nulls (JCS number canonicalization is not applied).
/// Do not reuse on a number-bearing `Value` (see module header).
pub fn canonical_json(v: &Value) -> String {
    serde_json::to_string(v).expect("Value is serializable")
}

/// id = first 12 hex of SHA-256 over the canonical-JSON of the hashed fields.
pub fn compute_id(t: &Tick) -> String {
    let canon = canonical_json(&hashed_value(t));
    let full = hex::encode(Sha256::digest(canon.as_bytes()));
    full[..12].to_string()
}
