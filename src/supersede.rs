//! `ev supersede` — replace a prior ruling under ev's append-only law. Two branches, dispatched in
//! `cmd::supersede` by whether a NEW ruling text is given:
//!
//!   * RE-TAG (id + tags, no new ruling): append a CHILD that copies the target's HASHED payload
//!     (decision / observe / grounds) verbatim and carries a corrected non-hashed tag — recognizably
//!     the same decision, its standing fixed. (This module.)
//!   * OVERTURN (id + a new ruling + `--assume` why): a fresh decision that REPLACES the prior ruling
//!     — built by `capture::overturn`, which reuses the full `ev decide` grammar + injects the edge.
//!
//! Both append a child carrying a `supersedes:<id>` edge through `capture::append`, so the target tick
//! is NEVER rewritten. A human authors it (blame required); it is UNREACHABLE from migrate /
//! canonical-intake, so an adapter can never launder a tag or overturn an import. `current_decisions`
//! then drops the superseded tick from every current view; `ev reopen <id>` marks it `superseded by`.

use crate::capture::{append, Decision};
use crate::store::Store;
use crate::tick::Tick;
use std::path::Path;

pub struct RetagArgs {
    pub id: String,
    pub authority: Option<String>,
    pub jurisdiction: Option<String>,
    pub provenance: Option<String>,
    pub blame: Option<String>,
}

/// The RE-TAG branch: copy the target's hashed payload verbatim + a corrected non-hashed tag, appended
/// as a child carrying the `supersedes` edge. The decision is not re-authored, so an unspecified tag
/// (including provenance) carries over unchanged.
pub fn retag(repo: &Path, a: RetagArgs) -> Result<Tick, String> {
    let store = Store::at(repo);
    if !store.exists() {
        return Err("no .evolving/ store here — run `ev init` first".into());
    }
    let target = store
        .read_tick(&a.id)
        .map_err(|e| format!("reading {}: {e}", a.id))?
        .ok_or_else(|| format!("no such tick: {}", a.id))?;

    // At least one tag must be supplied (else there is nothing to re-tag), and each is vocab-checked.
    if a.authority.is_none() && a.jurisdiction.is_none() && a.provenance.is_none() {
        return Err(
            "ev supersede (re-tag) needs at least one of --authority / --jurisdiction / --provenance \
             — or pass a new ruling text to overturn"
                .into(),
        );
    }
    if let Some(v) = &a.authority {
        crate::capture::validate_authority(v)?;
    }
    if let Some(v) = &a.jurisdiction {
        crate::tick::validate_jurisdiction(v)?;
    }
    if let Some(v) = &a.provenance {
        crate::tick::validate_provenance(v)?;
    }

    // The corrected tags: an override wins; otherwise inherit the target's.
    let authority = a.authority.clone().or_else(|| target.authority.clone());
    let jurisdiction = a
        .jurisdiction
        .clone()
        .or_else(|| target.jurisdiction.clone());
    let provenance = a.provenance.clone().or_else(|| target.provenance.clone());

    // Refuse a no-op: if nothing actually changes, there is nothing to re-tag.
    if authority == target.authority
        && jurisdiction == target.jurisdiction
        && provenance == target.provenance
    {
        return Err(format!(
            "tick {} already carries those tags — nothing to re-tag",
            a.id
        ));
    }
    // A detect-only (C/D) decision may carry no runnable Test check — refuse a re-tag that would make
    // the tick violate that structural lock (the same shared predicate verify + ingest use).
    if crate::tick::detect_only_carries_test(jurisdiction.as_deref(), &target.grounds) {
        return Err(format!(
            "cannot set jurisdiction {} on a decision that carries a test check (detect-only)",
            jurisdiction.as_deref().unwrap_or("")
        ));
    }

    let blame = crate::capture::resolve_blame(repo, a.blame.clone())?;

    // The corrective child: the target's HASHED payload verbatim + the corrected tags + the edge,
    // appended at HEAD (a new id, since parent_id differs). The target is never rewritten.
    append(
        repo,
        Decision {
            observe: target.observe.clone(),
            decision: target.decision.clone(),
            grounds: target.grounds.clone(),
            blame,
            authority,
            jurisdiction,
            source_ref: target.source_ref.clone(),
            provenance,
            supersedes: Some(target.id.clone()),
            ratifies: None, // a re-tag is not a ratification
        },
    )
}
