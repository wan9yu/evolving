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

/// RFC-8785 canonical bytes (serde_json compact over the sorted-key Value).
pub fn canonical_json(v: &Value) -> String {
    serde_json::to_string(v).expect("Value is serializable")
}

/// id = first 12 hex of SHA-256 over the canonical-JSON of the hashed fields.
pub fn compute_id(t: &Tick) -> String {
    let canon = canonical_json(&hashed_value(t));
    let full = hex::encode(Sha256::digest(canon.as_bytes()));
    full[..12].to_string()
}
