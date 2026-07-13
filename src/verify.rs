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

/// What it would take for an anchor to go red. A fact about the pointer's
/// shape — never a judgement about the claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Liveness {
    /// Fails when the cited text changes. The only class that can go red in a
    /// read-only audit of a tree the agent never writes.
    Content,
    /// Fails only if the cited path disappears.
    Existence,
    /// Content-addressed; fails only if the commit is absent from this clone.
    /// `verify_commit` asks this clone's object store, so a rewritten history, a
    /// shallow clone or an un-fetched branch all read the same way: absent.
    Immutable,
    /// Self-asserted; cannot fail by construction.
    Asserted,
}

impl Liveness {
    pub fn of(r: &EvRef) -> Liveness {
        match r.kind {
            RefKind::Metric | RefKind::Url => Liveness::Asserted,
            RefKind::Commit => Liveness::Immutable,
            RefKind::Test | RefKind::File | RefKind::Artifact => match r.passline {
                Some(_) => Liveness::Content,
                None => Liveness::Existence,
            },
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Liveness::Content => "content",
            Liveness::Existence => "existence",
            Liveness::Immutable => "immutable",
            Liveness::Asserted => "asserted",
        }
    }

    /// One phrasing for the liveness fact everywhere it is shown.
    pub fn why(&self) -> &'static str {
        match self {
            Liveness::Content => "fails when the cited text changes",
            Liveness::Existence => "fails only if the cited path disappears",
            Liveness::Immutable => {
                "content-addressed; fails only if the commit is absent from this clone"
            }
            Liveness::Asserted => "self-asserted; cannot fail by construction",
        }
    }
}

/// The attach-time guard. Refuses shapes ev cannot mean, and teaches the form
/// it can. Called ONLY from `verify_and_record` — never from `EvRef::parse`,
/// which must stay total so a 0.2.1 ledger holding `file:<path>:150` still
/// reads back (as `unreachable`) instead of erroring.
pub fn guard_attach(raw: &str) -> Result<()> {
    let r = EvRef::parse(raw)?;
    if !matches!(r.kind, RefKind::Test | RefKind::File | RefKind::Artifact) {
        return Ok(());
    }
    if r.passline.is_some() {
        return Ok(());
    }
    // A single-colon `<path>:<N>` tail: the caller almost certainly meant a line
    // number. ev anchors by content, so `:N` would silently become part of the
    // path and the anchor would resolve to nothing.
    if let Some((path, tail)) = r.payload.rsplit_once(':') {
        if !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()) {
            let scheme = match r.kind {
                RefKind::Test => "test",
                RefKind::Artifact => "artifact",
                _ => "file",
            };
            return Err(EvError::Refusal(format!(
                "{raw} — refused: looks like a line number, not a content anchor.\n    \
                 ev anchors by content, not by line (a line number stays green after the code moves).\n    \
                 Use {scheme}:{path}::<text on that line>."
            )));
        }
    }
    Ok(())
}

/// Check whether a ref's anchor resolves against `repo_root`. Resolution is a
/// fact about the pointer (exists, matches) — never a verdict on the claim.
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
        None => "resolves".into(),
        Some(pattern) => {
            let text = String::from_utf8_lossy(&content);
            if text.lines().any(|l| l.contains(pattern.as_str())) {
                "resolves".into()
            } else {
                "failed".into()
            }
        }
    }
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
        Ok(o) if o.status.success() => "resolves".into(),
        Ok(_) => "failed".into(),
        Err(_) => "unreachable".into(),
    }
}

/// Attach evidence to a claim and record whether its anchor resolves, in one
/// atomic batch. The filing also records `base` — the repo state (HEAD sha)
/// the anchor was filed against — so drift can be computed later.
pub fn verify_and_record(
    ledger: &Ledger,
    repo_root: &Path,
    claim_id: &str,
    raw_ref: &str,
    self_evident: bool,
    actor: Actor,
) -> Result<String> {
    guard_attach(raw_ref)?;
    let r = EvRef::parse(raw_ref)?;
    let status = verify_ref(&r, repo_root);
    let mut body = serde_json::json!({
        "claim": claim_id,
        "ref": raw_ref,
        "status": status,
        "self_evident": self_evident,
    });
    if let Some(base) = crate::git_output(repo_root, &["rev-parse", "HEAD"]) {
        body["base"] = serde_json::json!(base);
    }
    ledger.append_batch(vec![NewEvent {
        etype: "evidence".into(),
        actor,
        body,
    }])?;
    Ok(status)
}

/// Drift: how far the world has moved under a path-bearing anchor — the number
/// of commits between the recorded filing base and HEAD that touch the cited
/// path. A structural fact (no clocks, no dates); zero means the cited path is
/// exactly as the anchor saw it. None when the ref carries no path, the base
/// is unknown, or git cannot answer here.
pub fn drift(repo_root: &Path, base: &str, r: &EvRef) -> Option<u32> {
    let path = match r.kind {
        RefKind::Test | RefKind::File => r.payload.clone(),
        RefKind::Artifact => format!(".evolving/artifacts/{}", r.payload),
        RefKind::Commit | RefKind::Metric | RefKind::Url => return None,
    };
    let range = format!("{base}..HEAD");
    crate::git_output(repo_root, &["rev-list", "--count", &range, "--", &path])
        .and_then(|n| n.parse::<u32>().ok())
}

/// One phrasing for drift everywhere it is shown.
pub fn drift_phrase(k: u32) -> String {
    format!("drift: cited path changed in {k} commit(s) beyond the anchor")
}

/// Fill in drift on every evidence view that can carry it (path-bearing ref
/// with a recorded base). An explicit read-time step so the fold stays pure.
/// One git subprocess per annotated item; if claim counts grow, batching by
/// unique (base, path) pairs is the natural next step.
pub fn annotate_drift(d: &mut crate::state::Derived, repo_root: &Path) {
    let fill = |claims: &mut Vec<crate::state::ClaimView>| {
        for c in claims.iter_mut() {
            for ev in c.evidence.iter_mut() {
                if let (Some(base), Ok(r)) = (ev.base.as_deref(), EvRef::parse(&ev.eref)) {
                    ev.drift = drift(repo_root, base, &r);
                }
            }
        }
    };
    fill(&mut d.claims);
    fill(&mut d.grey);
    fill(&mut d.demands_returned);
}
