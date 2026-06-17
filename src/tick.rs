//! The decision tick and its parts. No serde derives: canonical and on-disk
//! encodings are built by hand (tick.rs / canonical.rs) for exact byte control.

#[derive(Debug, Clone, PartialEq)]
pub struct Tick {
    pub id: String,           // bookkeeping (the hash output)
    pub parent_id: String,    // hashed; "" on genesis, present
    pub observe: String,      // hashed
    pub decision: String,     // hashed
    pub grounds: Vec<Ground>, // hashed
    pub status: String,       // bookkeeping
    pub held_since: String,   // bookkeeping
    pub blame: String,        // bookkeeping
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ground {
    pub claim: String,
    pub supports: String, // "chosen" | "rejected:<option>"
    pub check: Option<Check>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Check {
    Person {
        reference: String,
    }, // by=person, ref=note
    Test {
        reference: String,       // by=test, ref=selector
        verified_at_sha: String, // 40 lowercase hex
        counter_test: String,
        liveness: Liveness,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Liveness {
    pub platforms: Vec<String>,
    pub triggered_by: Vec<String>,
    pub surfaces: Vec<String>,
}

use crate::canonical::hashed_value;
use serde_json::{Map, Value};

/// The on-disk tick: the hashed fields + the excluded bookkeeping at top level.
pub fn full_value(t: &Tick) -> Value {
    let mut v = hashed_value(t);
    if let Value::Object(map) = &mut v {
        map.insert("id".into(), Value::String(t.id.clone()));
        map.insert("status".into(), Value::String(t.status.clone()));
        map.insert("held_since".into(), Value::String(t.held_since.clone()));
        map.insert("blame".into(), Value::String(t.blame.clone()));
    }
    v
}

fn only_keys(obj: &Map<String, Value>, allowed: &[&str], what: &str) -> Result<(), String> {
    for k in obj.keys() {
        if !allowed.contains(&k.as_str()) {
            return Err(format!("{what}: field outside closed schema: {k}"));
        }
    }
    Ok(())
}

fn req_str(obj: &Map<String, Value>, k: &str) -> Result<String, String> {
    obj.get(k)
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .ok_or(format!("missing or non-string field: {k}"))
}

fn is_40_lower_hex(s: &str) -> bool {
    s.len() == 40
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn nonempty_str_set(obj: &Map<String, Value>, k: &str) -> Result<Vec<String>, String> {
    let a = obj
        .get(k)
        .and_then(|x| x.as_array())
        .ok_or(format!("liveness.{k} missing/not array"))?;
    let mut out = Vec::new();
    for e in a {
        let s = e
            .as_str()
            .ok_or(format!("liveness.{k} element not a string"))?;
        if s.is_empty() {
            return Err(format!("liveness.{k} has an empty element"));
        }
        out.push(s.to_string());
    }
    if out.is_empty() {
        return Err(format!("liveness.{k} must be non-empty"));
    }
    Ok(out)
}

fn check_from_value(v: &Value) -> Result<Check, String> {
    let obj = v.as_object().ok_or("check is not an object")?;
    match obj.get("by").and_then(|x| x.as_str()) {
        Some("person") => {
            only_keys(obj, &["by", "ref"], "person check")?;
            let reference = req_str(obj, "ref")?;
            if reference.is_empty() {
                return Err("person check ref is empty".into());
            }
            Ok(Check::Person { reference })
        }
        Some("test") => {
            only_keys(
                obj,
                &["by", "ref", "verified_at_sha", "counter_test", "liveness"],
                "test check",
            )?;
            let reference = req_str(obj, "ref")?;
            if reference.is_empty() {
                return Err("test check ref is empty".into());
            }
            let verified_at_sha = req_str(obj, "verified_at_sha")?;
            if !is_40_lower_hex(&verified_at_sha) {
                return Err(format!(
                    "verified_at_sha must be 40 lowercase hex: {verified_at_sha}"
                ));
            }
            let counter_test = req_str(obj, "counter_test")?;
            let lv = obj
                .get("liveness")
                .and_then(|x| x.as_object())
                .ok_or("liveness missing/not object")?;
            only_keys(lv, &["platforms", "triggered_by", "surfaces"], "liveness")?;
            let liveness = Liveness {
                platforms: nonempty_str_set(lv, "platforms")?,
                triggered_by: nonempty_str_set(lv, "triggered_by")?,
                surfaces: nonempty_str_set(lv, "surfaces")?,
            };
            Ok(Check::Test {
                reference,
                verified_at_sha,
                counter_test,
                liveness,
            })
        }
        other => Err(format!(
            "check.by must be \"test\" or \"person\", got {other:?}"
        )),
    }
}

fn ground_from_value(v: &Value) -> Result<Ground, String> {
    let obj = v.as_object().ok_or("ground is not an object")?;
    only_keys(obj, &["claim", "supports", "check"], "ground")?;
    let claim = req_str(obj, "claim")?;
    if claim.is_empty() {
        return Err("ground claim is empty".into());
    }
    let supports = req_str(obj, "supports")?;
    let ok_supports = supports == "chosen"
        || (supports.starts_with("rejected:") && supports.len() > "rejected:".len());
    if !ok_supports {
        return Err(format!("invalid supports: {supports}"));
    }
    let check = match obj.get("check") {
        None => None,
        Some(cv) => Some(check_from_value(cv)?),
    };
    Ok(Ground {
        claim,
        supports,
        check,
    })
}

/// Strict parse of an on-disk tick — this IS the R1 (closed schema) + R2 (check shape) check.
pub fn from_value(v: &Value) -> Result<Tick, String> {
    let obj = v.as_object().ok_or("tick is not an object")?;
    only_keys(
        obj,
        &[
            "id",
            "parent_id",
            "observe",
            "decision",
            "grounds",
            "status",
            "held_since",
            "blame",
        ],
        "tick",
    )?;
    let grounds_v = obj
        .get("grounds")
        .and_then(|x| x.as_array())
        .ok_or("grounds missing/not array")?;
    let mut grounds = Vec::new();
    for gv in grounds_v {
        grounds.push(ground_from_value(gv)?);
    }
    Ok(Tick {
        id: req_str(obj, "id")?,
        parent_id: req_str(obj, "parent_id")?,
        observe: req_str(obj, "observe")?,
        decision: req_str(obj, "decision")?,
        grounds,
        status: req_str(obj, "status")?,
        held_since: req_str(obj, "held_since")?,
        blame: req_str(obj, "blame")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn genesis_full() -> serde_json::Value {
        json!({
            "id": "e2b337f53a1f", "parent_id": "",
            "observe": "o", "decision": "d",
            "grounds": [{ "claim": "c", "supports": "chosen",
                          "check": { "by": "person", "ref": "Q3 review" } }],
            "status": "live", "held_since": "", "blame": "Wang Yu"
        })
    }

    #[test]
    fn from_value_should_round_trip_the_tick_when_it_is_well_formed() {
        // given: a well-formed on-disk tick value
        let v = genesis_full();

        // when: it is parsed through from_value
        let t = from_value(&v).expect("valid");

        // then: the parsed fields and the person check are preserved
        assert_eq!(t.decision, "d");
        assert_eq!(t.grounds.len(), 1);
        assert!(matches!(t.grounds[0].check, Some(Check::Person { .. })));
    }

    #[test]
    fn from_value_should_reject_the_tick_when_it_has_an_unknown_top_level_field() {
        // given: a tick value carrying a field outside the closed schema
        let mut v = genesis_full();
        v.as_object_mut()
            .unwrap()
            .insert("health".into(), json!("0.8"));

        // when: it is parsed through from_value
        let result = from_value(&v);

        // then: parsing fails
        assert!(result.is_err());
    }

    #[test]
    fn from_value_should_reject_the_check_when_it_carries_both_test_and_person_shape() {
        // given: a tick whose person check also carries a test-only liveness field
        let mut v = genesis_full();
        v["grounds"][0]["check"] = json!({ "by": "person", "ref": "x", "liveness": {} });

        // when: it is parsed through from_value
        let result = from_value(&v);

        // then: parsing fails
        assert!(result.is_err());
    }

    #[test]
    fn from_value_should_reject_the_test_check_when_its_sha_is_not_40_hex() {
        // given: a tick with a test check whose verified_at_sha is not 40 lowercase hex
        let mut v = genesis_full();
        v["grounds"][0]["check"] = json!({
            "by": "test", "ref": "r", "verified_at_sha": "ABC", "counter_test": "ct",
            "liveness": { "platforms": ["p"], "triggered_by": ["t"], "surfaces": ["s"] }
        });

        // when: it is parsed through from_value
        let result = from_value(&v);

        // then: parsing fails
        assert!(result.is_err());
    }

    #[test]
    fn from_value_should_reject_the_test_check_when_its_ref_is_empty() {
        // given: a tick with a test check whose ref is empty
        let mut v = genesis_full();
        v["grounds"][0]["check"] = json!({
            "by": "test", "ref": "", "verified_at_sha": "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901", "counter_test": "ct",
            "liveness": { "platforms": ["p"], "triggered_by": ["t"], "surfaces": ["s"] }
        });

        // when: it is parsed through from_value
        let result = from_value(&v);

        // then: parsing fails
        assert!(result.is_err());
    }
}
