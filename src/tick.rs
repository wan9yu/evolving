//! The decision tick and its parts. No serde derives: canonical and on-disk
//! encodings are built by hand (tick.rs / canonical.rs) for exact byte control.

#[derive(Debug, Clone, PartialEq)]
pub struct Tick {
    pub id: String,                   // bookkeeping (the hash output)
    pub parent_id: String,            // hashed; "" on genesis, present
    pub observe: String,              // hashed
    pub decision: String,             // hashed
    pub grounds: Vec<Ground>,         // hashed
    pub status: String,               // bookkeeping
    pub held_since: String,           // bookkeeping
    pub blame: String,                // bookkeeping
    pub authority: Option<String>,    // bookkeeping (declared, not hashed)
    pub jurisdiction: Option<String>, // bookkeeping (declared ∈ {A,B,C,D}, not hashed); C/D = detect-only
    pub round_id: Option<String>,     // bookkeeping (declared join/dedup key, not hashed)
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
        reference: String,            // by=test, ref=selector
        verified_at_sha: String,      // 40 lowercase hex
        counter_test: Option<String>, // None = harvested (falsifiability not yet proven)
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
        if let Some(a) = &t.authority {
            map.insert("authority".into(), Value::String(a.clone()));
        }
        if let Some(j) = &t.jurisdiction {
            map.insert("jurisdiction".into(), Value::String(j.clone()));
        }
        if let Some(r) = &t.round_id {
            map.insert("round_id".into(), Value::String(r.clone()));
        }
    }
    v
}

/// The closed jurisdiction vocabulary: A/B may gate; C/D are detect-only (structurally ungateable).
pub(crate) fn validate_jurisdiction(val: &str) -> Result<(), String> {
    if matches!(val, "A" | "B" | "C" | "D") {
        Ok(())
    } else {
        Err(format!(
            "jurisdiction must be one of A, B, C, D (got {val:?})"
        ))
    }
}

pub(crate) fn only_keys(
    obj: &Map<String, Value>,
    allowed: &[&str],
    what: &str,
) -> Result<(), String> {
    for k in obj.keys() {
        if !allowed.contains(&k.as_str()) {
            return Err(format!("{what}: field outside closed schema: {k}"));
        }
    }
    Ok(())
}

pub(crate) fn req_str(obj: &Map<String, Value>, k: &str) -> Result<String, String> {
    obj.get(k)
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .ok_or(format!("missing or non-string field: {k}"))
}

pub(crate) fn is_40_lower_hex(s: &str) -> bool {
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
            // counter_test is optional: absent = a harvested binding. When present it MUST be a
            // non-empty string (req_str accepts "", so guard non-emptiness explicitly here).
            let counter_test = match obj.get("counter_test") {
                None => None,
                Some(cv) => {
                    let s = cv.as_str().ok_or("counter_test present but not a string")?;
                    if s.is_empty() {
                        return Err("counter_test present but empty".into());
                    }
                    Some(s.to_string())
                }
            };
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
            "authority",
            "jurisdiction",
            "round_id",
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
        authority: obj
            .get("authority")
            .and_then(|x| x.as_str())
            .map(String::from),
        jurisdiction: match obj.get("jurisdiction").and_then(|x| x.as_str()) {
            None => None,
            Some(j) => {
                validate_jurisdiction(j)?; // out-of-vocab → Err
                Some(j.to_string())
            }
        },
        round_id: match obj.get("round_id") {
            None => None,
            Some(rv) => {
                // non-empty-if-present; no other format constraint (a free-form join/dedup key).
                let s = rv.as_str().ok_or("round_id present but not a string")?;
                if s.is_empty() {
                    return Err("round_id present but empty".into());
                }
                Some(s.to_string())
            }
        },
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
    fn from_value_should_round_trip_an_authority_tag_when_present() {
        // given: a well-formed tick carrying an authority tag
        let mut v = genesis_full();
        v.as_object_mut()
            .unwrap()
            .insert("authority".into(), json!("user-ruled"));

        // when: it is parsed
        let t = from_value(&v).expect("valid");

        // then: the authority tag is preserved
        assert_eq!(t.authority.as_deref(), Some("user-ruled"));
    }

    #[test]
    fn from_value_should_default_authority_to_none_when_absent() {
        // given: a tick with no authority field (the existing genesis shape)
        let v = genesis_full();

        // when: it is parsed
        let t = from_value(&v).expect("valid");

        // then: authority is None (absent = no claim)
        assert_eq!(t.authority, None);
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
    fn from_value_should_reject_an_empty_counter_test_when_present() {
        // given: a tick with a test check whose counter_test is present but empty
        let mut v = genesis_full();
        v["grounds"][0]["check"] = json!({
            "by": "test", "ref": "r", "verified_at_sha": "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901", "counter_test": "",
            "liveness": { "platforms": ["p"], "triggered_by": ["t"], "surfaces": ["s"] }
        });

        // when: it is parsed through from_value
        let result = from_value(&v);

        // then: parsing fails (non-empty-if-present; req_str would have accepted "")
        assert!(result.is_err());
    }

    #[test]
    fn from_value_should_round_trip_a_harvested_test_check_when_counter_test_is_absent() {
        // given: a tick with a test check that omits counter_test (a harvested binding)
        let mut v = genesis_full();
        v["grounds"][0]["check"] = json!({
            "by": "test", "ref": "r", "verified_at_sha": "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901",
            "liveness": { "platforms": ["p"], "triggered_by": ["t"], "surfaces": ["s"] }
        });

        // when: it is parsed through from_value
        let t = from_value(&v).expect("valid");

        // then: the test check parses with counter_test None
        assert!(matches!(
            &t.grounds[0].check,
            Some(Check::Test {
                counter_test: None,
                ..
            })
        ));
    }

    #[test]
    fn from_value_should_round_trip_a_jurisdiction_tag_when_present() {
        // given: a well-formed tick carrying a jurisdiction tag in the vocabulary
        let mut v = genesis_full();
        v.as_object_mut()
            .unwrap()
            .insert("jurisdiction".into(), json!("C"));

        // when: it is parsed
        let t = from_value(&v).expect("valid");

        // then: the jurisdiction tag is preserved
        assert_eq!(t.jurisdiction.as_deref(), Some("C"));
    }

    #[test]
    fn from_value_should_default_jurisdiction_to_none_when_absent() {
        // given: a tick with no jurisdiction field (the existing genesis shape)
        let v = genesis_full();

        // when: it is parsed
        let t = from_value(&v).expect("valid");

        // then: jurisdiction is None (absent = no claim)
        assert_eq!(t.jurisdiction, None);
    }

    #[test]
    fn from_value_should_round_trip_round_id_when_present() {
        // given: a well-formed tick carrying a round_id join/dedup key
        let mut v = genesis_full();
        v.as_object_mut()
            .unwrap()
            .insert("round_id".into(), json!("R2289"));

        // when: it is parsed
        let t = from_value(&v).expect("valid");

        // then: the round_id is preserved (durable, non-hashed)
        assert_eq!(t.round_id.as_deref(), Some("R2289"));
    }

    #[test]
    fn from_value_should_default_round_id_to_none_when_absent() {
        // given: a tick with no round_id field (the existing genesis shape)
        let v = genesis_full();

        // when: it is parsed
        let t = from_value(&v).expect("valid");

        // then: round_id is None (absent = no claim)
        assert_eq!(t.round_id, None);
    }

    #[test]
    fn from_value_should_reject_an_empty_round_id_when_present() {
        // given: a tick whose round_id is present but empty
        let mut v = genesis_full();
        v.as_object_mut()
            .unwrap()
            .insert("round_id".into(), json!(""));

        // when: it is parsed
        let result = from_value(&v);

        // then: parsing fails (non-empty-if-present; no other format constraint)
        assert!(result.is_err());
    }

    #[test]
    fn from_value_should_reject_an_out_of_vocab_jurisdiction() {
        // given: a tick whose jurisdiction is outside the closed {A,B,C,D} vocabulary
        let mut v = genesis_full();
        v.as_object_mut()
            .unwrap()
            .insert("jurisdiction".into(), json!("Z"));

        // when: it is parsed
        let result = from_value(&v);

        // then: parsing fails (vocab-validated, like authority)
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
