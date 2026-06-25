//! `ev ratify` — a human ratifies an agent proposal. It mints a CHILD copying the proposal's HASHED
//! payload (decision / observe / grounds) verbatim, flips `provenance → human-now` and
//! `authority → user-ruled`, and attaches the `ratifies:<proposal-id>` edge. The proposal itself is
//! NEVER rewritten — it stays immutable, thereafter shown "ratified by <child>". This is the ONLY
//! bridge from `agent-proposed` to a user-ruled ruling (the §五 line), and `--blame` is REQUIRED and
//! never auto-filled — ratification is the one op where a `git config` fallback would forge a human.
//! Same mint-a-child mechanics as `ev correct`; the child's id is content-addressed over the copied
//! payload, so the proposal and its ratified child are recognizably the same decision.

use crate::capture::{append, Decision};
use crate::store::Store;
use crate::tick::Tick;
use std::path::Path;

pub struct RatifyArgs {
    pub id: String,
    /// The ratifying human. REQUIRED + explicit — never auto-filled from git.
    pub blame: String,
}

pub fn run(repo: &Path, a: RatifyArgs) -> Result<Tick, String> {
    let store = Store::at(repo);
    if !store.exists() {
        return Err("no .evolving/ store here — run `ev init` first".into());
    }
    let target = store
        .read_tick(&a.id)
        .map_err(|e| format!("reading {}: {e}", a.id))?
        .ok_or_else(|| format!("no such tick: {}", a.id))?;

    // Only an agent proposal can be ratified — ratify is the agent→human bridge, not a re-stamp of a
    // human ruling or an imported record.
    if target.provenance.as_deref() != Some("agent-proposed") {
        return Err(format!(
            "ev ratify only ratifies an agent proposal; tick {} is {} (nothing to ratify)",
            a.id,
            target.provenance.as_deref().unwrap_or("human-now")
        ));
    }

    // --blame is REQUIRED + explicit: ratification is the one op where a git-config fallback would
    // forge a human, so it never auto-fills.
    let blame = a.blame.trim();
    if blame.is_empty() {
        return Err("ev ratify requires a non-empty --blame <human> (never auto-filled)".into());
    }

    // The ratified child: the proposal's HASHED payload verbatim + human-now / user-ruled + the
    // ratifies edge, appended at HEAD (a new id). The proposal is never rewritten — immutability
    // intact. provenance=None is the absent default (= human-now), exactly as `ev decide` writes it.
    let child = append(
        repo,
        Decision {
            observe: target.observe.clone(),
            decision: target.decision.clone(),
            grounds: target.grounds.clone(),
            blame: blame.to_string(),
            authority: Some("user-ruled".into()),
            jurisdiction: target.jurisdiction.clone(),
            source_ref: target.source_ref.clone(),
            provenance: None, // absent = human-now (same representation as ev decide)
            supersedes: None,
            ratifies: Some(target.id.clone()),
        },
    )?;
    crate::events::append(&store, "ratify", Some(&child), None, None, None);
    Ok(child)
}
