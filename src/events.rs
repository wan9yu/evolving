//! A local, append-only events log (results/events.jsonl) — the decision-data埋点 for
//! metrics. Gitignored, 0-network, best-effort (a write failure never fails the command).
use crate::store::Store;
use crate::tick::{source_ref_key, Tick};
use serde_json::{json, Value};
use std::io::Write;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

/// Append one event line: `{ts, op, tick_id?, source_ref?, age?, verdict?, masked_stale?}`. When a
/// deciding tick is given it contributes the join key (`source_ref`) and a coarse decision-AGE bucket
/// — together the prior-decision discriminator the metrics framework needs: a check firing on an OLD
/// decision is a prior-decision resurface; on a just-written one it is not. `masked_stale` carries a
/// stale sub-kind that the (worse) per-tick verdict hides, so a staleness-mask never silently drops.
pub fn append(
    store: &Store,
    op: &str,
    tick: Option<&Tick>,
    verdict: Option<&str>,
    masked_stale: Option<&str>,
    suppressed_from: Option<&str>,
) {
    let now = OffsetDateTime::now_utc();
    let ts = now.format(&Rfc3339).unwrap_or_default();
    let mut e = json!({ "ts": ts, "op": op });
    if let Some(o) = e.as_object_mut() {
        if let Some(t) = tick {
            o.insert("tick_id".into(), Value::String(t.id.clone()));
            // parent_id + the relation-overlay edges (supersedes/ratifies) are the HEED-edge: they let the
            // metrics harness join a heed (a correction/ratification) back to the decision whose check
            // went red — turning re-litigation-prevented into a real count. Non-hashed; emitted only
            // when present, so existing event lines are unchanged.
            if !t.parent_id.is_empty() {
                o.insert("parent_id".into(), Value::String(t.parent_id.clone()));
            }
            if let Some(sr) = &t.source_ref {
                o.insert("source_ref".into(), Value::String(source_ref_key(sr)));
            }
            if let Some(age) = age_bucket(&t.held_since, now.unix_timestamp()) {
                o.insert("age".into(), Value::String(age.into()));
            }
            if let Some(c) = &t.supersedes {
                o.insert("supersedes".into(), Value::String(c.clone()));
            }
            if let Some(r) = &t.ratifies {
                o.insert("ratifies".into(), Value::String(r.clone()));
            }
        }
        if let Some(v) = verdict {
            o.insert("verdict".into(), Value::String(v.into()));
        }
        if let Some(m) = masked_stale {
            o.insert("masked_stale".into(), Value::String(m.into()));
        }
        // The pre-remap verdict when a not-green was suppressed to memo (LOCK 1 detect-only / LOCK 3
        // agent-proposed). Additive: `verdict` stays "memo" unchanged; this lets the metrics harness
        // tell a suppressed-red CATCH from a benign memo. Absent on a non-suppressed event.
        if let Some(s) = suppressed_from {
            o.insert("suppressed_from".into(), Value::String(s.into()));
        }
    }
    write_line(store, &e);
}

/// Append one COST/metrics summary event: `{ts, op, ...fields}`. The validation harness joins these
/// against the per-decision catch events to compute NET effect (catch benefit MINUS injected cost) —
/// e.g. the `brief` boot-read injection size, the `check-run` wall-time. Additive (a new op line); same
/// best-effort, non-hashed, gitignored cache as `append`.
pub fn append_cost(store: &Store, op: &str, fields: &[(&str, Value)]) {
    let ts = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_default();
    let mut e = json!({ "ts": ts, "op": op });
    if let Some(o) = e.as_object_mut() {
        for (k, v) in fields {
            o.insert((*k).to_string(), v.clone());
        }
    }
    write_line(store, &e);
}

/// Best-effort write of one JSON line to results/events.jsonl. A write failure never fails the command
/// (the log is a droppable, gitignored cache — deleting it never changes a tick id).
fn write_line(store: &Store, e: &Value) {
    let dir = store.root.join("results");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("events.jsonl"))
    {
        let _ = writeln!(f, "{}", serde_json::to_string(e).unwrap_or_default());
    }
}

/// A coarse decision-age bucket from a tick's `held_since` (RFC3339) to `now_unix`: `fresh` (<1d) /
/// `days` (<7d) / `weeks` (<30d) / `months` (<365d) / `year+` (>=365d). Coarse on purpose — the metric
/// thresholds it into prior-decision vs just-written; it leaks no precision the tick does not carry.
fn age_bucket(held_since: &str, now_unix: i64) -> Option<&'static str> {
    let then = OffsetDateTime::parse(held_since, &Rfc3339).ok()?;
    let secs = now_unix - then.unix_timestamp();
    let days = secs / 86_400;
    // Sub-day rungs split what used to all collapse to "fresh": a decision made earlier the SAME day is
    // a prior decision, not just-written. Without the split, the prior-decision catch-rate goes 0/0 on
    // any same-day run (e.g. a ~120-commit/day agent). `held_since` carries the precision; the old 1-day
    // floor threw it away. Existing day+ labels are unchanged, so older harness reads still parse.
    Some(if secs < 300 {
        "fresh"
    } else if secs < 3_600 {
        "minutes"
    } else if secs < 86_400 {
        "hours"
    } else if days < 7 {
        "days"
    } else if days < 30 {
        "weeks"
    } else if days < 365 {
        "months"
    } else {
        "year+"
    })
}

#[cfg(test)]
mod tests {
    use super::age_bucket;
    use time::{format_description::well_known::Rfc3339, OffsetDateTime};

    const NOW: i64 = 1_750_000_000; // a fixed clock so the boundary arithmetic is deterministic

    fn held(secs_ago: i64) -> String {
        OffsetDateTime::from_unix_timestamp(NOW - secs_ago)
            .unwrap()
            .format(&Rfc3339)
            .unwrap()
    }

    #[test]
    fn age_bucket_should_label_each_threshold() {
        let h = 3_600;
        let d = 86_400;
        // given/then: each side of every day-bucket boundary maps to the right coarse label
        assert_eq!(age_bucket(&held(0), NOW), Some("fresh"));
        assert_eq!(age_bucket(&held(23 * h), NOW), Some("hours"));
        assert_eq!(age_bucket(&held(25 * h), NOW), Some("days"));
        assert_eq!(age_bucket(&held(6 * d), NOW), Some("days"));
        assert_eq!(age_bucket(&held(8 * d), NOW), Some("weeks"));
        assert_eq!(age_bucket(&held(29 * d), NOW), Some("weeks"));
        assert_eq!(age_bucket(&held(31 * d), NOW), Some("months"));
        assert_eq!(age_bucket(&held(364 * d), NOW), Some("months"));
        assert_eq!(age_bucket(&held(366 * d), NOW), Some("year+"));
    }

    #[test]
    fn age_bucket_should_resolve_within_the_day() {
        // a decision made EARLIER the same day is a prior decision, not just-written — the sub-day
        // rungs must distinguish it, else a same-day run's prior-decision catch-rate is 0/0 (all "fresh")
        let m = 60;
        let h = 3_600;
        assert_eq!(age_bucket(&held(2 * m), NOW), Some("fresh")); // <=5 min — written this round
        assert_eq!(age_bucket(&held(30 * m), NOW), Some("minutes")); // a few rounds back, same session
        assert_eq!(age_bucket(&held(5 * h), NOW), Some("hours")); // earlier today — a prior decision
    }

    #[test]
    fn age_bucket_should_be_none_when_held_since_is_unparseable() {
        // given/then: a garbage or empty timestamp yields no bucket (a data fault, never a wrong label)
        assert_eq!(age_bucket("not a timestamp", NOW), None);
        assert_eq!(age_bucket("", NOW), None);
    }
}
