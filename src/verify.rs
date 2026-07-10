use crate::ledger::{Actor, Ledger, NewEvent};
use crate::{EvError, Result};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefKind {
    Commit,
    Test,
    File,
    Artifact,
    Metric,
    Url,
}

#[derive(Debug, Clone)]
pub struct EvRef {
    pub kind: RefKind,
    pub payload: String,
    pub passline: Option<String>,
}

impl EvRef {
    pub fn parse(raw: &str) -> Result<EvRef> {
        let (scheme, rest) = raw.split_once(':').ok_or_else(|| {
            EvError::Refusal(format!(
                "ref must be typed (commit:/test:/file:/artifact:/metric:/url:): {raw}"
            ))
        })?;
        let kind = match scheme {
            "commit" => RefKind::Commit,
            "test" => RefKind::Test,
            "file" => RefKind::File,
            "artifact" => RefKind::Artifact,
            "metric" => RefKind::Metric,
            "url" => RefKind::Url,
            other => return Err(EvError::Refusal(format!("unknown ref type: {other}:"))),
        };
        // test/file/artifact refs may carry a "::passline" match target
        let (payload, passline) = match kind {
            RefKind::Test | RefKind::File | RefKind::Artifact => match rest.split_once("::") {
                Some((p, line)) => (p.to_string(), Some(line.to_string())),
                None => (rest.to_string(), None),
            },
            _ => (rest.to_string(), None),
        };
        Ok(EvRef {
            kind,
            payload,
            passline,
        })
    }
}

pub fn status_str(raw: &str) -> &'static str {
    match raw {
        "verified" => "verified",
        "failed" => "failed",
        "unreachable" => "unreachable",
        _ => "recorded",
    }
}

/// Verify a ref against `repo_root`.
/// V1: Commit → `git rev-parse --verify`; Metric/Url → "recorded" (self-asserted).
/// V2: Test/File/Artifact → exists→sha256→pass-line check.
/// Never touches the network.
pub fn verify_ref(r: &EvRef, repo_root: &Path) -> String {
    match r.kind {
        RefKind::Commit => verify_commit(&r.payload, repo_root),
        RefKind::Metric | RefKind::Url => "recorded".into(),
        RefKind::Test | RefKind::File | RefKind::Artifact => verify_v2(r, repo_root),
    }
}

fn verify_v2(r: &EvRef, repo_root: &Path) -> String {
    let path = if r.kind == RefKind::Artifact {
        repo_root.join(".evolving/artifacts").join(&r.payload)
    } else {
        repo_root.join(&r.payload)
    };
    if !path.exists() {
        return "unreachable".into();
    }
    // existence is established; hash it (proves readability), then the pass-line.
    let content = match std::fs::read(&path) {
        Ok(c) => c,
        Err(_) => return "unreachable".into(),
    };
    let _digest = {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(&content);
        format!("{:x}", h.finalize())
    };
    match &r.passline {
        None => "verified".into(),
        Some(pattern) => {
            let text = String::from_utf8_lossy(&content);
            if text.lines().any(|l| l.contains(pattern.as_str())) {
                "verified".into()
            } else {
                "failed".into()
            }
        }
    }
}

/// Copy a matched pass-line region (±20 lines) into `.evolving/artifacts/` and
/// return the artifact ref that replaces a fragile transcript ref. Used by exhaust.
pub fn archive_region(repo_root: &Path, source: &Path, pattern: &str) -> Result<Option<String>> {
    let text = std::fs::read_to_string(source).map_err(|e| EvError::Failure(e.to_string()))?;
    let lines: Vec<&str> = text.lines().collect();
    let Some(hit) = lines.iter().position(|l| l.contains(pattern)) else {
        return Ok(None);
    };
    let lo = hit.saturating_sub(20);
    let hi = (hit + 21).min(lines.len());
    let region = lines[lo..hi].join("\n");
    let name = format!("region-{}.txt", ulid::Ulid::new());
    let dir = repo_root.join(".evolving/artifacts");
    std::fs::create_dir_all(&dir).map_err(|e| EvError::Failure(e.to_string()))?;
    std::fs::write(dir.join(&name), region).map_err(|e| EvError::Failure(e.to_string()))?;
    Ok(Some(format!("artifact:{name}::{pattern}")))
}

fn verify_commit(sha: &str, repo_root: &Path) -> String {
    let out = Command::new("git")
        .args([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{sha}^{{commit}}"),
        ])
        .current_dir(repo_root)
        .output();
    match out {
        Ok(o) if o.status.success() => "verified".into(),
        Ok(_) => "failed".into(),
        Err(_) => "unreachable".into(),
    }
}

/// Attach evidence to a claim and record a verify verdict, in one atomic batch.
pub fn verify_and_record(
    ledger: &Ledger,
    repo_root: &Path,
    claim_id: &str,
    raw_ref: &str,
    self_evident: bool,
    actor: Actor,
) -> Result<String> {
    let r = EvRef::parse(raw_ref)?;
    let status = verify_ref(&r, repo_root);
    ledger.append_batch(vec![NewEvent {
        etype: "evidence".into(),
        actor,
        body: serde_json::json!({
            "claim": claim_id,
            "ref": raw_ref,
            "status": status,
            "self_evident": self_evident,
        }),
    }])?;
    Ok(status)
}
