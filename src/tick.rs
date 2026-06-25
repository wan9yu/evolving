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
    pub source_ref: Option<Value>, // bookkeeping (opaque producer-supplied source identity — a string or object, not hashed); ev derives a dedup key, never interprets it
    pub provenance: Option<String>, // bookkeeping (declared ∈ {imported,agent-proposed,human-now}, not hashed); absent = human-now
    pub corrects: Option<String>, // bookkeeping (non-hashed): a relation-overlay edge — this tick CORRECTS the tick with this id (written by `ev correct`).
    pub ratifies: Option<String>, // bookkeeping (non-hashed): a relation-overlay edge — this (human-now) tick RATIFIES the agent proposal with this id (written by `ev ratify`). The SECOND overlay edge; corrects + ratifies are the two specific, adopter-driven bridges — the general case-law graph is deliberately not built.
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
        if let Some(r) = &t.source_ref {
            map.insert("source_ref".into(), r.clone());
        }
        if let Some(p) = &t.provenance {
            map.insert("provenance".into(), Value::String(p.clone()));
        }
        if let Some(c) = &t.corrects {
            map.insert("corrects".into(), Value::String(c.clone()));
        }
        if let Some(r) = &t.ratifies {
            map.insert("ratifies".into(), Value::String(r.clone()));
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

/// The closed provenance vocabulary — how a tick entered the ledger. `human-now` (the absent
/// default) and `agent-proposed` are fresh authorship; `imported` is faithfully-transcribed history.
/// `verify` partitions only the R5 lexical op-lint by this tag (a transcribed historical op-word is
/// a warning, not a fresh op-claim); every hard refusal stays hard for all provenance. The vocabulary
/// is a non-hashed bookkeeping value — a future value is a non-breaking additive change.
pub(crate) fn validate_provenance(val: &str) -> Result<(), String> {
    if matches!(val, "imported" | "agent-proposed" | "human-now") {
        Ok(())
    } else {
        Err(format!(
            "provenance must be one of imported, agent-proposed, human-now (got {val:?})"
        ))
    }
}

/// The opaque source reference is a producer-supplied identity for the decision in ITS source — a
/// non-empty string (e.g. an issue/commit ref) or a non-empty structured object. ev NEVER interprets
/// its contents: it is the adopter's concept (a "round", a ticket, a work-unit), carried opaquely.
/// Only these two shapes are accepted; a bare number/bool/null/array is not a meaningful identity.
pub(crate) fn validate_source_ref(v: &Value) -> Result<(), String> {
    match v {
        Value::String(s) if !s.is_empty() => Ok(()),
        Value::String(_) => Err("source_ref string is empty".into()),
        Value::Object(m) if !m.is_empty() => Ok(()),
        Value::Object(_) => Err("source_ref object is empty".into()),
        _ => Err("source_ref must be a non-empty string or object".into()),
    }
}

/// The ONE thing ev derives from a source_ref: a stable scalar dedup/reconcile key. A string is its
/// own key; an object's key is its deterministic JSON (serde_json's Map is a sorted BTreeMap with
/// `preserve_order` off, so equal objects serialize identically). ev compares THESE keys, never the
/// contents — so a producer that keeps its source identity stable gets idempotent re-imports.
pub(crate) fn source_ref_key(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Whether a tick's source_ref normalizes to `key` (the same dedup key `source_ref_key` derives).
/// One predicate so propose's idempotency and `ev pending --source-ref` agree on "same source_ref".
pub(crate) fn source_ref_matches(t: &Tick, key: &str) -> bool {
    t.source_ref.as_ref().map(source_ref_key).as_deref() == Some(key)
}

/// Whether a detect-only (`C`/`D`) jurisdiction carries a runnable Test check on any ground. This is
/// the single predicate behind the structural "a detect-only decision must not be able to gate, so it
/// holds no runnable test binding" refusal — enforced both at the migrate ingest boundary (refuse at
/// the door) and at-rest by `verify` (LOCK 2). One definition so the two sites can never drift.
pub(crate) fn detect_only_carries_test(jurisdiction: Option<&str>, grounds: &[Ground]) -> bool {
    matches!(jurisdiction, Some("C") | Some("D")) && has_test_check(grounds)
}

/// Whether any ground carries a runnable Test check — i.e. the decision is test-bound (catch-eligible).
/// One definition so the gate (LOCK 2), the migrate boundary, and the brief cost decomposition never drift.
pub(crate) fn has_test_check(grounds: &[Ground]) -> bool {
    grounds
        .iter()
        .any(|g| matches!(g.check, Some(Check::Test { .. })))
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

pub(crate) fn ground_from_value(v: &Value) -> Result<Ground, String> {
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

/// The hashed/identity set: top-level keys that the schema is STRICT about. A tick whose payload
/// has an unknown key INSIDE these (e.g. a stray key on a ground/check) is rejected by the nested
/// strict `only_keys`; these names are also what `unknown_top_level_keys` excludes when surfacing
/// a tolerated forward-compat key as a warning.
pub(crate) const HASHED_TOP_LEVEL_KEYS: &[&str] = &[
    "id",
    "parent_id",
    "observe",
    "decision",
    "grounds",
    "status",
    "held_since",
    "blame",
];

/// The known-non-hashed allow-list: declared bookkeeping fields, validated but not hashed.
pub(crate) const KNOWN_NON_HASHED_KEYS: &[&str] = &[
    "authority",
    "jurisdiction",
    "source_ref",
    "provenance",
    "corrects",
    "ratifies",
];

/// Validate a relation-overlay back-link (`corrects` / `ratifies`): it must be a tick id — exactly 12
/// lowercase hex. (ev only ever writes a real target id via `ev correct` / `ev ratify`; the check
/// catches a hand-edited/typo'd reference.)
pub(crate) fn validate_edge_id(field: &str, val: &str) -> Result<(), String> {
    if is_tick_id(val) {
        Ok(())
    } else {
        Err(format!(
            "{field} must be a 12-char lowercase-hex tick id (got {val:?})"
        ))
    }
}

/// Whether `s` is a well-formed tick id: exactly 12 lowercase-hex chars. ev only ever writes ids it
/// computed, so a caller-supplied id that fails this is a typo or a path-injection attempt — it guards
/// the file lookups in `ev show` / `Store::read_tick` against `..` / absolute-path arguments.
pub(crate) fn is_tick_id(s: &str) -> bool {
    s.len() == 12
        && s.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

/// A tick's top-level keys that are neither hashed/identity nor a known-non-hashed field — the
/// truly-unknown, tolerated forward-compat keys (`from_value` parses them through; verify warns).
pub(crate) fn unknown_top_level_keys(obj: &Map<String, Value>) -> Vec<String> {
    obj.keys()
        .filter(|k| {
            !HASHED_TOP_LEVEL_KEYS.contains(&k.as_str())
                && !KNOWN_NON_HASHED_KEYS.contains(&k.as_str())
        })
        .cloned()
        .collect()
}

/// Strict parse of an on-disk tick — this IS the R1 (closed schema) + R2 (check shape) check.
///
/// Two-tier forward-compat (T3): the hashed/identity set (`HASHED_TOP_LEVEL_KEYS`) stays STRICT —
/// a missing one is an Err, and the nested grounds/check schemas reject any unknown key inside the
/// HASHED payload, so the content-addressed id can never carry an unvalidated field. The known
/// non-hashed fields are validated. A truly-unknown OTHER top-level key is TOLERATED (parsed
/// through, not rejected) so a newer writer's bookkeeping field does not brick an older reader;
/// `ev verify` surfaces it as a `warning:` so a typo'd field name stays visible.
pub fn from_value(v: &Value) -> Result<Tick, String> {
    let obj = v.as_object().ok_or("tick is not an object")?;
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
        source_ref: match obj.get("source_ref") {
            None => None,
            Some(rv) => {
                validate_source_ref(rv)?; // a non-empty string or object; ev never interprets it
                Some(rv.clone())
            }
        },
        provenance: match obj.get("provenance").and_then(|x| x.as_str()) {
            None => None,
            Some(p) => {
                validate_provenance(p)?; // out-of-vocab → Err
                Some(p.to_string())
            }
        },
        corrects: match obj.get("corrects").and_then(|x| x.as_str()) {
            None => None,
            Some(c) => {
                validate_edge_id("corrects", c)?; // must be a 12-hex tick id
                Some(c.to_string())
            }
        },
        ratifies: match obj.get("ratifies").and_then(|x| x.as_str()) {
            None => None,
            Some(r) => {
                validate_edge_id("ratifies", r)?; // must be a 12-hex tick id
                Some(r.to_string())
            }
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_only_relation_overlay_edges_are_corrects_and_ratifies() {
        // given/then: ev ships exactly TWO relation-overlay edges — `corrects` (`ev correct`) and
        // `ratifies` (`ev ratify`). They are specific, adopter-driven bridges, each for one real need;
        // the general case-law graph (governed-by / case-of / arbitrary typed edges) is the PARKED 0.2
        // work and is deliberately NOT built. This fence pins the overlay surface: the non-hashed key
        // set is exactly these six — four bookkeeping TAGS plus the TWO relation edges. Adding another
        // overlay field (a new relation especially) must be a DELIBERATE decision that updates this
        // fence, never an accident.
        assert_eq!(
            KNOWN_NON_HASHED_KEYS,
            &[
                "authority",
                "jurisdiction",
                "source_ref",
                "provenance",
                "corrects",
                "ratifies"
            ],
            "the relation-overlay surface changed — is this a deliberate new edge (the 0.2 graph)?"
        );
    }

    #[test]
    fn from_value_should_reject_a_corrects_that_is_not_a_tick_id() {
        // given: an on-disk tick whose corrects edge is hand-edited to a non-id value
        let mut v = genesis_full();
        v.as_object_mut()
            .unwrap()
            .insert("corrects".into(), json!("not-a-real-id"));

        // when/then: the read-path validator rejects it — the overlay edge must be a real tick id
        assert!(from_value(&v).is_err());
    }

    #[test]
    fn from_value_should_accept_a_valid_corrects_edge() {
        // given: an on-disk tick carrying a well-formed 12-hex corrects edge
        let mut v = genesis_full();
        v.as_object_mut()
            .unwrap()
            .insert("corrects".into(), json!("638c47b0c9dd"));

        // when/then: it parses and round-trips the edge
        let t = from_value(&v).expect("a valid corrects edge parses");
        assert_eq!(t.corrects.as_deref(), Some("638c47b0c9dd"));
    }

    #[test]
    fn from_value_should_accept_a_valid_ratifies_edge() {
        // given: an on-disk tick carrying a well-formed 12-hex ratifies edge
        let mut v = genesis_full();
        v.as_object_mut()
            .unwrap()
            .insert("ratifies".into(), json!("638c47b0c9dd"));

        // when/then: it parses and round-trips the edge
        let t = from_value(&v).expect("a valid ratifies edge parses");
        assert_eq!(t.ratifies.as_deref(), Some("638c47b0c9dd"));
    }

    #[test]
    fn an_overlay_edge_must_be_a_twelve_hex_tick_id() {
        // given/then: each edge validates as a tick id — a hand-edited/typo'd reference is rejected
        assert!(validate_edge_id("corrects", "e2b337f53a1f").is_ok());
        assert!(validate_edge_id("ratifies", "638c47b0c9dd").is_ok());
        assert!(validate_edge_id("corrects", "E2B337F53A1F").is_err()); // uppercase
        assert!(validate_edge_id("corrects", "e2b337").is_err()); // too short
        assert!(validate_edge_id("corrects", "not-hex-here!").is_err());
    }

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
    fn from_value_should_reject_the_tick_when_a_hashed_identity_field_is_missing() {
        // given: a tick value missing a hashed/identity field (the strict tier stays closed)
        let mut v = genesis_full();
        v.as_object_mut().unwrap().remove("decision");

        // when: it is parsed through from_value
        let result = from_value(&v);

        // then: parsing fails — the hashed/identity set is not forward-compat-tolerant
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
    fn from_value_should_round_trip_a_string_source_ref_when_present() {
        // given: a well-formed tick carrying a bare-string opaque source_ref
        let mut v = genesis_full();
        v.as_object_mut()
            .unwrap()
            .insert("source_ref".into(), json!("R2289"));

        // when: it is parsed
        let t = from_value(&v).expect("valid");

        // then: the source_ref is preserved verbatim (durable, non-hashed, opaque)
        assert_eq!(t.source_ref, Some(json!("R2289")));
    }

    #[test]
    fn from_value_should_round_trip_a_structured_source_ref_when_given_an_object() {
        // given: a tick whose source_ref is a STRUCTURED object (richer than a string)
        let mut v = genesis_full();
        v.as_object_mut().unwrap().insert(
            "source_ref".into(),
            json!({"round": "R2289", "ticket": "#1194"}),
        );

        // when: it is parsed
        let t = from_value(&v).expect("valid");

        // then: the whole object is carried opaquely (ev never interprets its fields)
        assert_eq!(
            t.source_ref,
            Some(json!({"round": "R2289", "ticket": "#1194"}))
        );
    }

    #[test]
    fn from_value_should_default_source_ref_to_none_when_absent() {
        // given: a tick with no source_ref field (the existing genesis shape)
        let v = genesis_full();

        // when: it is parsed
        let t = from_value(&v).expect("valid");

        // then: source_ref is None (absent = no claim)
        assert_eq!(t.source_ref, None);
    }

    #[test]
    fn from_value_should_reject_an_empty_source_ref_when_present() {
        // given: a tick whose source_ref is present but an empty string
        let mut v = genesis_full();
        v.as_object_mut()
            .unwrap()
            .insert("source_ref".into(), json!(""));

        // when: it is parsed
        let result = from_value(&v);

        // then: parsing fails (an empty identity is no identity)
        assert!(result.is_err());
    }

    #[test]
    fn from_value_should_reject_a_non_string_non_object_source_ref() {
        // given: a tick whose source_ref is a bare number (not a meaningful identity)
        let mut v = genesis_full();
        v.as_object_mut()
            .unwrap()
            .insert("source_ref".into(), json!(42));

        // when: it is parsed
        let result = from_value(&v);

        // then: parsing fails (only a non-empty string or object is accepted)
        assert!(result.is_err());
    }

    #[test]
    fn from_value_should_round_trip_provenance_when_present() {
        // given: a well-formed tick carrying a provenance tag in the vocabulary
        let mut v = genesis_full();
        v.as_object_mut()
            .unwrap()
            .insert("provenance".into(), json!("imported"));

        // when: it is parsed
        let t = from_value(&v).expect("valid");

        // then: the provenance tag is preserved (declared, non-hashed)
        assert_eq!(t.provenance.as_deref(), Some("imported"));
    }

    #[test]
    fn from_value_should_default_provenance_to_none_when_absent() {
        // given: a tick with no provenance field (the existing genesis shape = human-now)
        let v = genesis_full();

        // when: it is parsed
        let t = from_value(&v).expect("valid");

        // then: provenance is None (absent = human-now, no laundering possible)
        assert_eq!(t.provenance, None);
    }

    #[test]
    fn from_value_should_reject_an_out_of_vocab_provenance() {
        // given: a tick whose provenance is outside the closed vocabulary
        let mut v = genesis_full();
        v.as_object_mut()
            .unwrap()
            .insert("provenance".into(), json!("self-asserted"));

        // when: it is parsed
        let result = from_value(&v);

        // then: parsing fails (vocab-validated, like jurisdiction)
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

    #[test]
    fn from_value_should_tolerate_an_unknown_non_hashed_key_when_reading() {
        // given: a well-formed tick carrying a bogus extra top-level key (a forward-compat field)
        let mut v = genesis_full();
        v.as_object_mut()
            .unwrap()
            .insert("future_field".into(), json!("x"));

        // when: it is parsed through from_value
        let t = from_value(&v).expect("an unknown top-level key is tolerated (parsed-through)");

        // then: the known fields are intact (the unknown key is ignored, not rejected)
        assert_eq!(t.decision, "d");
        assert_eq!(t.observe, "o");
        assert_eq!(t.grounds.len(), 1);
    }

    #[test]
    fn from_value_should_still_reject_an_unknown_key_inside_the_hashed_payload() {
        // given: a well-formed tick whose ground (part of the hashed payload) carries an unknown key
        let mut v = genesis_full();
        v["grounds"][0]
            .as_object_mut()
            .unwrap()
            .insert("future_field".into(), json!("x"));

        // when: it is parsed through from_value
        let result = from_value(&v);

        // then: parsing fails — the hashed payload stays a strictly closed schema
        assert!(result.is_err());
    }
}
