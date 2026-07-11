use crate::ledger::{Actor, Ledger, NewEvent};
use crate::Result;
use std::collections::HashSet;
use std::io::Read;
use std::path::Path;

fn write_settings(path: &Path, v: &serde_json::Value) -> Result<()> {
    std::fs::write(
        path,
        format!("{}\n", serde_json::to_string_pretty(v).unwrap()),
    )
    .map_err(crate::EvError::from)
}

/// Merge ev's SessionStart + SessionEnd hooks into .claude/settings.json, idempotently.
pub fn install(root: &Path) -> Result<()> {
    let settings_path = root.join(".claude/settings.json");
    std::fs::create_dir_all(root.join(".claude"))?;
    let mut v: serde_json::Value = std::fs::read_to_string(&settings_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}));

    let hooks = v
        .as_object_mut()
        .unwrap()
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}));
    // SessionStart fires on startup and resume; compact is excluded via the matcher.
    upsert_hook(
        hooks,
        "SessionStart",
        "ev hook session-start",
        Some("startup|resume"),
    );
    upsert_hook(hooks, "SessionEnd", "ev hook session-end", None);

    write_settings(&settings_path, &v)?;
    println!("installed ev hooks into {}", settings_path.display());
    Ok(())
}

/// Remove the idempotent marker so re-running adds exactly one entry per event.
fn upsert_hook(hooks: &mut serde_json::Value, event: &str, command: &str, matcher: Option<&str>) {
    let arr = hooks
        .as_object_mut()
        .unwrap()
        .entry(event)
        .or_insert_with(|| serde_json::json!([]));
    let list = arr.as_array_mut().unwrap();
    // drop any prior ev entry for this event so exactly one remains after insertion
    list.retain(|e| {
        !serde_json::to_string(e)
            .unwrap_or_default()
            .contains("ev hook")
    });
    let mut entry = serde_json::json!({ "hooks": [ { "type": "command", "command": command } ] });
    if let Some(m) = matcher {
        entry["matcher"] = serde_json::json!(m);
    }
    list.push(entry);
}

pub fn uninstall(root: &Path) -> Result<()> {
    let settings_path = root.join(".claude/settings.json");
    if let Ok(s) = std::fs::read_to_string(&settings_path) {
        if let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&s) {
            if let Some(hooks) = v.get_mut("hooks").and_then(|h| h.as_object_mut()) {
                for (_ev, arr) in hooks.iter_mut() {
                    if let Some(list) = arr.as_array_mut() {
                        list.retain(|e| {
                            !serde_json::to_string(e)
                                .unwrap_or_default()
                                .contains("ev hook")
                        });
                    }
                }
            }
            write_settings(&settings_path, &v)?;
        }
    }
    println!("removed ev hooks.");
    Ok(())
}

/// SessionStart: print the brief (injected as context by the host), then run the sweep.
/// Internal errors are silenced — a hook must never fail the host session.
pub fn session_start(root: &Path) -> Result<()> {
    let _ = drain_stdin();
    if let Ok(ledger) = Ledger::open(root) {
        // The text brief shows no drift, so no annotation here; if drift ever
        // reaches the text surface, annotate before rendering.
        if let Ok(events) = ledger.scan() {
            let d = crate::state::fold(&events);
            print!("{}", crate::render::brief(&d, false));
        }
        let _ = sweep(root, &ledger);
    }
    Ok(())
}

/// SessionEnd: append one marker recording the HEAD sha and `swept:false`.
/// One atomic batch write; survives being killed.
pub fn session_end(root: &Path) -> Result<()> {
    let payload = drain_stdin();
    let session = extract_session_id(&payload).unwrap_or_else(|| ulid::Ulid::new().to_string());
    let head = crate::git_output(root, &["rev-parse", "HEAD"]);
    if let Ok(ledger) = Ledger::open(root) {
        let mut body = serde_json::json!({ "marker": "end", "session": session, "swept": false });
        if let Some(h) = head {
            body["head"] = serde_json::json!(h);
        }
        let _ = ledger.append_batch(vec![NewEvent {
            etype: "session".into(),
            actor: Actor::engine(),
            body,
        }]);
    }
    Ok(())
}

/// The primary exhaust path: windowed, writer-scoped, idempotent.
///
/// For each end-marker not yet swept (for this writer), file commits in the window
/// (previous_swept_head, this_head].  This ensures each session gets its own
/// non-overlapping range and no commit is double-filed.
pub fn sweep(root: &Path, ledger: &Ledger) -> Result<()> {
    let events = ledger.scan()?;
    let my_writer = ledger.writer_id();

    // collect session ids already marked swept by this writer
    let swept: HashSet<String> = events
        .iter()
        .filter(|e| {
            e.etype == "session"
                && e.writer == my_writer
                && e.body.get("marker").and_then(|s| s.as_str()) == Some("swept")
        })
        .filter_map(|e| {
            e.body
                .get("session")
                .and_then(|s| s.as_str())
                .map(|s| s.to_string())
        })
        .collect();

    // the watermark starts at the head recorded on the most-recent SWEPT marker (not the end
    // marker); swept markers always carry a concrete resolved sha, so this is unambiguous.
    // Fall back to ROOT when no swept markers exist yet.
    let mut watermark: String = events
        .iter()
        .filter(|e| {
            e.etype == "session"
                && e.writer == my_writer
                && e.body.get("marker").and_then(|s| s.as_str()) == Some("swept")
        })
        .max_by_key(|e| (e.ts.clone(), e.seq))
        .and_then(|e| {
            e.body
                .get("head")
                .and_then(|h| h.as_str())
                .map(|h| h.to_string())
        })
        .unwrap_or_else(|| "ROOT".to_string());

    // collect unswept end-markers for this writer, ordered by (ts, seq)
    let mut pending: Vec<_> = events
        .iter()
        .filter(|e| {
            e.etype == "session"
                && e.writer == my_writer
                && e.body.get("marker").and_then(|s| s.as_str()) == Some("end")
                && !e
                    .body
                    .get("session")
                    .and_then(|s| s.as_str())
                    .map(|id| swept.contains(id))
                    .unwrap_or(false)
        })
        .collect();
    pending.sort_by_key(|e| (e.ts.clone(), e.seq));

    for marker in pending {
        let session_id = match marker.body.get("session").and_then(|s| s.as_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let until = marker
            .body
            .get("head")
            .and_then(|h| h.as_str())
            .unwrap_or("HEAD")
            .to_string();

        // file the non-overlapping window (watermark, until]
        let window = crate::exhaust::discover(root, &watermark, &until, &session_id)?;
        let _ = crate::exhaust::file_window(ledger, root, &window, None)?;

        // resolve the raw ref to a concrete sha so the swept marker and next watermark are
        // never the ambiguous literal "HEAD"
        let resolved = resolve_sha(root, &until);

        // mark this session swept; a write failure must not abort the sweep loop
        let _ = ledger.append_batch(vec![NewEvent {
            etype: "session".into(),
            actor: Actor::engine(),
            body: serde_json::json!({ "marker": "swept", "session": session_id, "head": resolved }),
        }]);

        watermark = resolved;
    }
    Ok(())
}

/// Resolve a git ref (including the literal "HEAD") to its concrete sha.
/// Falls back to the input unchanged when git is unavailable or the ref is unknown.
fn resolve_sha(root: &Path, r: &str) -> String {
    crate::git_output(root, &["rev-parse", r]).unwrap_or_else(|| r.to_string())
}

fn drain_stdin() -> String {
    let mut s = String::new();
    let _ = std::io::stdin().read_to_string(&mut s);
    s
}

fn extract_session_id(payload: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(payload).ok()?;
    v.get("session_id")
        .and_then(|s| s.as_str())
        .map(|s| s.to_string())
}
