use crate::ledger::{Actor, Ledger, NewEvent};
use crate::Result;
use std::path::Path;

// stub — real verification arrives in a later task
pub fn verify_and_record(
    ledger: &Ledger,
    _repo_root: &Path,
    claim_id: &str,
    raw_ref: &str,
    self_evident: bool,
    actor: Actor,
) -> Result<String> {
    ledger.append_batch(vec![NewEvent {
        etype: "evidence".into(),
        actor,
        body: serde_json::json!({
            "claim": claim_id, "ref": raw_ref, "status": "recorded", "self_evident": self_evident,
        }),
    }])?;
    Ok("recorded".into())
}
