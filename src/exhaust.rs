use crate::ledger::{Actor, Ledger, NewEvent};
use crate::{EvError, Result};
use std::path::Path;
use std::process::Command;

pub struct Window {
    pub session: String,
    pub shas: Vec<String>,
    pub subjects: Vec<String>,
    pub branch: String,
}

/// Discover commits in the range (since, until].
/// When `since == "ROOT"` the range is the full history up to `until`.
/// When `since` is a sha the range is `since..until`.
pub fn discover(repo_root: &Path, since: &str, until: &str, session: &str) -> Result<Window> {
    let range = if since == "ROOT" {
        until.to_string()
    } else {
        format!("{since}..{until}")
    };
    let log = Command::new("git")
        .args(["log", "--format=%H%x1f%s", &range])
        .current_dir(repo_root)
        .output()
        .map_err(|e| EvError::Failure(e.to_string()))?;
    let mut shas = Vec::new();
    let mut subjects = Vec::new();
    for line in String::from_utf8_lossy(&log.stdout).lines() {
        if let Some((h, s)) = line.split_once('\u{1f}') {
            shas.push(h.to_string());
            subjects.push(s.to_string());
        }
    }
    let branch = crate::git_output(repo_root, &["rev-parse", "--abbrev-ref", "HEAD"])
        .unwrap_or_else(|| "HEAD".into());
    Ok(Window {
        session: session.to_string(),
        shas,
        subjects,
        branch,
    })
}

/// The label rule: the commit subject when the window carries exactly one commit;
/// otherwise the first non-boilerplate summary line; else a shas-count fallback.
pub fn label(w: &Window, closing_summary: Option<&str>) -> String {
    if w.shas.len() == 1 {
        return w.subjects[0].clone();
    }
    if let Some(summary) = closing_summary {
        for line in summary.lines() {
            let t = line.trim();
            if t.is_empty() || is_boilerplate(t) {
                continue;
            }
            return t.to_string();
        }
    }
    format!(
        "session {}: {} commits on {}",
        short_session(&w.session),
        w.shas.len(),
        w.branch
    )
}

fn is_boilerplate(line: &str) -> bool {
    let l = line.to_lowercase();
    (l.starts_with("round ") && l.contains("complete"))
        || l == "done."
        || l.starts_with("session complete")
}

fn short_session(s: &str) -> String {
    s.chars().take(8).collect()
}

/// File one claim for a session window, with all shas as self-evident evidence.
/// Idempotent on the session id (used as source_ref).
pub fn file_window(
    ledger: &Ledger,
    repo_root: &Path,
    w: &Window,
    closing_summary: Option<&str>,
) -> Result<Option<String>> {
    if w.shas.is_empty() {
        return Ok(None);
    }
    let source_ref = format!("session:{}", w.session);
    let events = ledger.scan()?;
    if events.iter().any(|e| {
        e.etype == "claim"
            && e.body.get("source_ref").and_then(|s| s.as_str()) == Some(source_ref.as_str())
    }) {
        return Ok(None);
    }
    let actor = Actor::agent("exhaust");
    let minted = ledger.append_batch(vec![NewEvent {
        etype: "claim".into(),
        actor: actor.clone(),
        body: serde_json::json!({
            "label": label(w, closing_summary),
            "source_ref": source_ref,
        }),
    }])?;
    let claim_id = minted[0].id.clone();
    // one evidence event per sha, all self_evident (verified against this repo)
    let mut batch = Vec::new();
    for sha in &w.shas {
        let status = crate::verify::verify_ref(
            &crate::verify::EvRef::parse(&format!("commit:{sha}"))?,
            repo_root,
        );
        batch.push(NewEvent {
            etype: "evidence".into(),
            actor: actor.clone(),
            body: serde_json::json!({
                "claim": claim_id,
                "ref": format!("commit:{sha}"),
                "status": status,
                "self_evident": true,
            }),
        });
    }
    ledger.append_batch(batch)?;
    Ok(Some(claim_id))
}
