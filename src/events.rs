//! A local, append-only events log (results/events.jsonl) — the decision-data埋点 for
//! metrics. Gitignored, 0-network, best-effort (a write failure never fails the command).
use crate::store::Store;
use serde_json::{json, Value};
use std::io::Write;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

/// Append one event line: {ts, op, tick_id?, verdict?}.
pub fn append(store: &Store, op: &str, tick_id: Option<&str>, verdict: Option<&str>) {
    let ts = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_default();
    let mut e = json!({ "ts": ts, "op": op });
    if let Some(o) = e.as_object_mut() {
        if let Some(id) = tick_id {
            o.insert("tick_id".into(), Value::String(id.into()));
        }
        if let Some(v) = verdict {
            o.insert("verdict".into(), Value::String(v.into()));
        }
    }
    let dir = store.root.join("results");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("events.jsonl"))
    {
        let _ = writeln!(f, "{}", serde_json::to_string(&e).unwrap_or_default());
    }
}
