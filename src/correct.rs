//! `ev correct` — append a corrective CHILD tick that fixes a stale non-hashed tag (authority /
//! jurisdiction / provenance) on an existing decision, under ev's append-only law.
//!
//! The child copies the target's HASHED payload (decision / observe / grounds) verbatim — so it is
//! recognizably the same decision — and carries the corrected tag; it is funneled through
//! `capture::append` (a new id at HEAD), so the target tick is NEVER rewritten. A human authors it
//! (blame required); it is UNREACHABLE from migrate / canonical-intake, so an adapter can never
//! launder a tag. `brief`/`list` then collapse the lineage to its current (latest) state, so the
//! corrected child surfaces and the stale parent stays as honest history.

use crate::capture::{append, Decision};
use crate::store::Store;
use crate::tick::Tick;
use std::path::Path;

pub struct CorrectArgs {
    pub id: String,
    pub authority: Option<String>,
    pub jurisdiction: Option<String>,
    pub provenance: Option<String>,
    pub blame: Option<String>,
}

pub fn run(repo: &Path, a: CorrectArgs) -> Result<Tick, String> {
    let store = Store::at(repo);
    if !store.exists() {
        return Err("no .evolving/ store here — run `ev init` first".into());
    }
    let target = store
        .read_tick(&a.id)
        .map_err(|e| format!("reading {}: {e}", a.id))?
        .ok_or_else(|| format!("no such tick: {}", a.id))?;

    // At least one tag must be supplied (else there is nothing to correct), and each is vocab-checked.
    if a.authority.is_none() && a.jurisdiction.is_none() && a.provenance.is_none() {
        return Err(
            "ev correct needs at least one of --authority / --jurisdiction / --provenance".into(),
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

    // The corrected tags: an override wins; otherwise inherit the target's (a tag-correction does not
    // re-author the decision, so an unspecified tag — including provenance — carries over unchanged).
    let authority = a.authority.clone().or_else(|| target.authority.clone());
    let jurisdiction = a
        .jurisdiction
        .clone()
        .or_else(|| target.jurisdiction.clone());
    let provenance = a.provenance.clone().or_else(|| target.provenance.clone());

    // Refuse a no-op: if nothing actually changes, there is nothing to correct.
    if authority == target.authority
        && jurisdiction == target.jurisdiction
        && provenance == target.provenance
    {
        return Err(format!(
            "tick {} already carries those tags — nothing to correct",
            a.id
        ));
    }
    // A detect-only (C/D) decision may carry no runnable Test check — refuse a correction that would
    // make the tick violate that structural lock (the same shared predicate verify + ingest use).
    if crate::tick::detect_only_carries_test(jurisdiction.as_deref(), &target.grounds) {
        return Err(format!(
            "cannot set jurisdiction {} on a decision that carries a test check (detect-only)",
            jurisdiction.as_deref().unwrap_or("")
        ));
    }

    let blame = crate::capture::resolve_blame(repo, a.blame.clone())?;

    // The corrective child: the target's HASHED payload verbatim + the corrected tags, appended at
    // HEAD (a new id, since parent_id differs). The target is never rewritten — immutability intact.
    let child = append(
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
        },
    )?;
    crate::events::append(&store, "correct", Some(&child), None, None);
    Ok(child)
}
