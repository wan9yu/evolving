//! The verdict-cache read contract: results/state/<tick_id>.json — a per-host, gitignored
//! snapshot of each tick's per-ground verdicts that a consumer hook reads WITHOUT shelling
//! `ev check`. Facts, no scores; one row per ground.
use crate::store::Store;
use crate::tick::{Check, Ground};
use crate::verdict::Verdict;
use serde_json::{json, Map, Value};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

/// Write `results/state/<tick_id>.json`: one row per `(ground, verdict)` pair, the staleness
/// reference, and the time of computation. Pairing at the boundary keeps grounds and verdicts
/// from drifting out of alignment.
pub fn write_state(
    store: &Store,
    tick_id: &str,
    rows: &[(&Ground, Verdict)],
    staleness_policy: &str,
    staleness_sha: Option<&str>,
) -> std::io::Result<()> {
    let computed_at = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_default();
    let grounds: Vec<Value> = rows
        .iter()
        .map(|(g, v)| {
            let mut row = Map::new();
            row.insert("claim".into(), Value::String(g.claim.clone()));
            row.insert("supports".into(), Value::String(g.supports.clone()));
            let check = match &g.check {
                Some(Check::Test {
                    reference,
                    verified_at_sha,
                    ..
                }) => {
                    row.insert("ref".into(), Value::String(reference.clone()));
                    row.insert(
                        "verified_at_sha".into(),
                        Value::String(verified_at_sha.clone()),
                    );
                    "test"
                }
                Some(Check::Person { .. }) => "person",
                None => "none",
            };
            row.insert("check".into(), Value::String(check.into()));
            row.insert("verdict".into(), Value::String(v.label().into()));
            if let Verdict::NotRun { missing_platforms } = v {
                row.insert("missing_platforms".into(), json!(missing_platforms));
            }
            Value::Object(row)
        })
        .collect();
    let doc = json!({
        "tick_id": tick_id,
        "computed_at": computed_at,
        "staleness_ref": { "policy": staleness_policy, "sha": staleness_sha },
        "grounds": grounds,
    });
    let dir = store.root.join("results").join("state");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(
        dir.join(format!("{tick_id}.json")),
        serde_json::to_string_pretty(&doc).expect("serializable"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tick::{Ground, Liveness, Tick};

    fn store() -> (std::path::PathBuf, Store) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "ev-state-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        let s = Store::at(&p);
        s.init().unwrap();
        (p, s)
    }

    #[test]
    fn write_state_should_record_each_ground_verdict_when_a_tick_is_evaluated() {
        // given: a tick with a not-run test ground and a person ground, and their verdicts
        let (_p, s) = store();
        let tick = Tick {
            id: "abcabcabcabc".into(),
            parent_id: "".into(),
            observe: "o".into(),
            decision: "d".into(),
            grounds: vec![
                Ground {
                    claim: "no Redis".into(),
                    supports: "chosen".into(),
                    check: Some(Check::Test {
                        reference: "pytest x".into(),
                        verified_at_sha: "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901".into(),
                        counter_test: Some("ct".into()),
                        liveness: Liveness {
                            platforms: vec!["linux-ci".into()],
                            triggered_by: vec!["f".into()],
                            surfaces: vec!["s".into()],
                        },
                    }),
                },
                Ground {
                    claim: "team ok".into(),
                    supports: "chosen".into(),
                    check: Some(Check::Person {
                        reference: "Q3".into(),
                    }),
                },
            ],
            status: "live".into(),
            held_since: "".into(),
            blame: "Wang Yu".into(),
            authority: None,
            jurisdiction: None,
            source_ref: None,
            provenance: None,
            corrects: None,
            ratifies: None,
        };
        let rows = vec![
            (
                &tick.grounds[0],
                Verdict::NotRun {
                    missing_platforms: vec!["linux-ci".into()],
                },
            ),
            (&tick.grounds[1], Verdict::NotApplicable),
        ];

        // when: the state file is written
        write_state(&s, &tick.id, &rows, "live-origin", None).unwrap();

        // then: results/state/<id>.json records the tick id and each ground's verdict
        let text = std::fs::read_to_string(
            s.root
                .join("results")
                .join("state")
                .join("abcabcabcabc.json"),
        )
        .unwrap();
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["tick_id"], "abcabcabcabc");
        assert_eq!(v["grounds"][0]["check"], "test");
        assert_eq!(v["grounds"][0]["ref"], "pytest x");
        assert_eq!(v["grounds"][0]["verdict"], "not-run");
        assert_eq!(v["grounds"][0]["missing_platforms"][0], "linux-ci");
        assert_eq!(v["grounds"][1]["check"], "person");
        assert_eq!(v["grounds"][1]["verdict"], "n/a");
    }
}
