use crate::ledger::{Actor, Ledger, NewEvent};
use crate::{EvError, Result};
use serde::Serialize;
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

impl RefKind {
    /// The scheme word a ref of this kind is written with — one spelling for every
    /// site that prints one back. Exhaustive: a new variant names itself here
    /// rather than being printed as some other scheme's word.
    pub fn scheme(&self) -> &'static str {
        match self {
            RefKind::Commit => "commit",
            RefKind::Test => "test",
            RefKind::File => "file",
            RefKind::Artifact => "artifact",
            RefKind::Metric => "metric",
            RefKind::Url => "url",
        }
    }
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
/// shape — never a judgement about the claim. The five names are the ones the
/// JSON surfaces carry; kebab-case leaves each single word exactly as written.
#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
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
    /// A ref no current grammar accepts — the honest reading of a shape an older
    /// ledger recorded. `Liveness::of` never returns it; the fold assigns it where
    /// the parse itself fails, so the class is carried rather than dropped.
    Unparseable,
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
            Liveness::Unparseable => "unparseable",
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
            Liveness::Unparseable => "no current ref grammar accepts this pointer",
        }
    }
}

/// The attach-time guard. Refuses anchors ev cannot mean, and anchors that
/// cannot carry a signal. Called ONLY from `verify_and_record` and `cmd::claim`
/// — never from `EvRef::parse`, which must stay total so a ledger written by an
/// older version still reads back instead of erroring.
pub fn guard_attach(raw: &str, repo_root: &Path) -> Result<EvRef> {
    let r = EvRef::parse(raw)?;
    if !matches!(r.kind, RefKind::Test | RefKind::File | RefKind::Artifact) {
        return Ok(r);
    }

    match &r.passline {
        // A single-colon `<path>:<N>` tail: the caller almost certainly meant a
        // line number. ev anchors by content, so `:N` would silently become
        // part of the path and the anchor would resolve to nothing.
        None => {
            if let Some((path, tail)) = r.payload.rsplit_once(':') {
                if !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()) {
                    let scheme = r.kind.scheme();
                    return Err(EvError::Refusal(format!(
                        "{raw} — refused: looks like a line number, not a content anchor.\n    \
                         ev anchors by content, not by line (a line number stays green after the code moves).\n    \
                         Use {scheme}:{path}::<text on that line>."
                    )));
                }
            }
            Ok(r)
        }
        Some(text) if text.is_empty() => Err(EvError::Refusal(format!(
            "{raw} — refused: the pass-line after `::` is empty.\n    \
             An empty pass-line matches every line, so the anchor can never go red."
        ))),
        Some(text) => {
            // The cited text must exist NOW. An anchor on absent text is born red and
            // stays red forever — it carries no signal and never will.
            let path = if r.kind == RefKind::Artifact {
                repo_root.join(".evolving/artifacts").join(&r.payload)
            } else {
                repo_root.join(&r.payload)
            };
            let present = std::fs::read(&path)
                .map(|c| {
                    String::from_utf8_lossy(&c)
                        .lines()
                        .any(|l| l.contains(text.as_str()))
                })
                .unwrap_or(false);
            if !present {
                return Err(EvError::Refusal(format!(
                    "{raw} — the cited text is not in {} at this commit.\n    \
                     A content anchor must quote text that exists now; it goes red when that text changes.",
                    r.payload
                )));
            }
            Ok(r)
        }
    }
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
    let r = guard_attach(raw_ref, repo_root)?;
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
