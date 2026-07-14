use crate::ledger::{Actor, Ledger, NewEvent};
use crate::{EvError, Result};
use serde::{Deserialize, Serialize};
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

/// What ev found when it looked at the anchor. A fact about the pointer — never a
/// verdict on the claim. `Failed` is the pre-0.2.3 conflated value: it is READ from
/// older ledgers and never written, because it hid three different findings behind
/// one word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    /// The anchor holds.
    Resolves,
    /// The file is there; the cited text is not. The line ev was pointed at changed.
    Changed,
    /// The path is absent, or the commit is absent from this clone.
    Gone,
    /// The path exists but ev could not read it — not a fact about the code.
    Unreachable,
    /// `metric:` / `url:` — self-asserted; cannot fail by construction.
    Recorded,
    /// Legacy only. Written by ev before 0.2.3; never produced by this version.
    Failed,
}

impl Status {
    /// Read a status out of a ledger event. `verified` is the 0.1.x spelling of
    /// `resolves`. An unrecognised value reads as `Failed` — ev does not guess.
    pub fn parse(raw: &str) -> Status {
        match raw {
            "resolves" | "verified" => Status::Resolves,
            "changed" => Status::Changed,
            "gone" => Status::Gone,
            "unreachable" => Status::Unreachable,
            "recorded" => Status::Recorded,
            _ => Status::Failed,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Status::Resolves => "resolves",
            Status::Changed => "changed",
            Status::Gone => "gone",
            Status::Unreachable => "unreachable",
            Status::Recorded => "recorded",
            Status::Failed => "failed",
        }
    }
}

/// The join of what ev found (`Status`) and how far the world moved under the anchor
/// (`drift`, counted from the human's last look). ev has always emitted both facts and
/// never put them side by side — so a whole class of movement, the one a content anchor
/// is blind to, went unread. A cell is a fact, never a verdict: it says RE-READ, never
/// "resolved".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Cell {
    /// Drift was measured, and it is zero: nothing this anchor can see has moved.
    /// An UNMEASURED drift is not this cell — it is no cell at all.
    Still,
    /// The cited line stands; code moved beside it. The content anchor's blind spot.
    NeighborhoodMoved,
    /// The cited line itself changed.
    AnchorChanged,
    /// The container is gone.
    FileGone,
    /// A pre-0.2.3 conflated status. ev cannot classify it without re-verifying, and
    /// does not guess. The next `ev verify` resolves it.
    Legacy,
}

impl Cell {
    /// THE ONE AND ONLY derivation. A second site is a second source of truth.
    ///
    /// No cell is emitted when drift could not be measured (`drift == None` under a
    /// `Resolves`): `still` would assert that nothing moved, and ev did not look. The
    /// absent cell is the same convention `Recorded` and `Unreachable` already carry —
    /// no cell means ev asserts nothing. A `commit:` ref, whose drift is None by
    /// construction, therefore carries no cell either; `Liveness::Immutable` already
    /// states why nothing can move under it.
    pub fn of(status: Status, drift: Option<u32>) -> Option<Cell> {
        match status {
            // `still` means MEASURED, AND ZERO.
            Status::Resolves => drift.map(|k| {
                if k > 0 {
                    Cell::NeighborhoodMoved
                } else {
                    Cell::Still
                }
            }),
            Status::Changed => Some(Cell::AnchorChanged),
            Status::Gone => Some(Cell::FileGone),
            Status::Failed => Some(Cell::Legacy),
            // A self-asserted ref has no world under it; an unreadable one is not a
            // fact about the code. Neither has a cell.
            Status::Recorded | Status::Unreachable => None,
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
/// Commit → `git rev-parse --verify`; Metric/Url → `Recorded` (self-asserted);
/// Test/File/Artifact → exists → pass-line check.
///
/// The finding is a class, not a word: a `Gone` container and a `Changed` line
/// are different facts and read differently. Never touches the network.
pub fn verify_ref(r: &EvRef, repo_root: &Path) -> Status {
    match r.kind {
        RefKind::Commit => verify_commit(&r.payload, repo_root),
        RefKind::Metric | RefKind::Url => Status::Recorded,
        RefKind::Test | RefKind::File | RefKind::Artifact => verify_v2(r, repo_root),
    }
}

fn verify_v2(r: &EvRef, repo_root: &Path) -> Status {
    let path = if r.kind == RefKind::Artifact {
        repo_root.join(".evolving/artifacts").join(&r.payload)
    } else {
        repo_root.join(&r.payload)
    };
    // The container is absent — a rename, a delete. Distinct from a path ev can
    // see but cannot read, which is a fact about ev's reach, not about the code.
    if !path.exists() {
        return Status::Gone;
    }
    let content = match std::fs::read(&path) {
        Ok(c) => c,
        Err(_) => return Status::Unreachable,
    };
    match &r.passline {
        None => Status::Resolves,
        Some(pattern) => {
            let text = String::from_utf8_lossy(&content);
            if text.lines().any(|l| l.contains(pattern.as_str())) {
                Status::Resolves
            } else {
                // The file is there; the cited text is not. The line moved.
                Status::Changed
            }
        }
    }
}

fn verify_commit(sha: &str, repo_root: &Path) -> Status {
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
        Ok(o) if o.status.success() => Status::Resolves,
        // The object is absent from this clone: a rewritten history, a shallow
        // clone, an un-fetched branch all read the same way.
        Ok(_) => Status::Gone,
        // ev could not run git — not a fact about the object.
        Err(_) => Status::Unreachable,
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
) -> Result<Status> {
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

/// THE ONE AND ONLY reference rule: drift is counted from the HUMAN'S LAST LOOK
/// (`last_ack`, the head of the most recent `ack`) when there is one, else from the
/// filing `base`. Without the ack reference, `neighborhood-moved` is a ratchet — it
/// rises once and no human can ever clear it, and a permanent red carries no
/// information.
///
/// This is NOT a re-base: the evidence `base` stays pinned forever and is never
/// written to. `last_ack` is a second, human-relative reference point, read here at
/// annotation time. Auto re-basing would zero drift on every commit — a structural
/// false-green.
///
/// The reference order is `last_ack` FIRST, chosen and not overlooked. When evidence is
/// filed AFTER the last ack, the filing `base` is newer than `last_ack` and the count
/// still runs from `last_ack` — so it includes commits that predate the anchor's own
/// existence. That errs safe: it over-flags (says RE-READ), never under-flags, and a
/// fresh ack clears it. Counting from the newer of the two would risk the opposite
/// error, and a silent under-flag is the one failure a ratchet cannot survive.
///
/// Every surface that reports drift calls this; a second rule elsewhere would be the
/// second source of truth the cell exists to prevent.
pub fn drift_since(
    repo_root: &Path,
    last_ack: Option<&str>,
    base: Option<&str>,
    r: &EvRef,
) -> Option<u32> {
    let reference = last_ack.or(base)?;
    drift(repo_root, reference, r)
}

/// Fill in drift and the cell on every evidence view that can carry them (path-bearing
/// ref with a reference point). An explicit read-time step so the fold stays pure.
/// One git subprocess per annotated item; if claim counts grow, batching by
/// unique (reference, path) pairs is the natural next step.
pub fn annotate_drift(d: &mut crate::state::Derived, repo_root: &Path) {
    let fill = |claims: &mut Vec<crate::state::ClaimView>| {
        for c in claims.iter_mut() {
            let last_ack = c.last_ack.clone();
            for ev in c.evidence.iter_mut() {
                if let Ok(r) = EvRef::parse(&ev.eref) {
                    ev.drift = drift_since(repo_root, last_ack.as_deref(), ev.base.as_deref(), &r);
                }
                ev.cell = Cell::of(ev.status, ev.drift);
            }
        }
    };
    fill(&mut d.claims);
    fill(&mut d.closed);
    fill(&mut d.grey);
    fill(&mut d.demands_returned);
}
