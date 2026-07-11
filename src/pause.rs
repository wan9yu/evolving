use crate::ledger::{Actor, Ledger, NewEvent};
use crate::state::{ClaimState, Derived};
use crate::Result;
use std::io::{BufRead, Write};
use std::path::Path;
use std::time::Instant;

pub struct PauseOpts {
    pub boundary: bool,
    pub script: bool, // non-tty scripted stdin, no fancy prompts
}

pub fn run_pause(root: &Path, opts: PauseOpts) -> Result<()> {
    let ledger = Ledger::open(root)?;
    let mut d = crate::state::fold(&ledger.scan()?);
    crate::verify::annotate_drift(&mut d, root);
    let started = Instant::now();
    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();
    let mut out = std::io::stdout();

    // Screen 0 — the day's shape
    writeln!(out, "— pause —")?;
    writeln!(
        out,
        "{} open · {} grey · {} demand(s) answered",
        d.claims.len(),
        d.grey.len(),
        d.demands_returned.len()
    )?;

    // Screen 1 — returned demands (the payoff)
    if !d.demands_returned.is_empty() {
        writeln!(out, "\n↩ answered demands:")?;
        for c in &d.demands_returned {
            let drifted = c.evidence.iter().filter_map(|e| e.drift).max().unwrap_or(0);
            if drifted > 0 {
                writeln!(
                    out,
                    "  {} {} — now has {} evidence · {}",
                    crate::render::mark(c.self_evident, &c.state),
                    c.label,
                    c.evidence.len(),
                    crate::verify::drift_phrase(drifted)
                )?;
            } else {
                writeln!(
                    out,
                    "  {} {} — now has {} evidence",
                    crate::render::mark(c.self_evident, &c.state),
                    c.label,
                    c.evidence.len()
                )?;
            }
        }
    }

    // Screen 2 — the exhaust batch (self-evident work), honest acknowledge wording
    let batch: Vec<_> = d.claims.iter().filter(|c| c.self_evident).collect();
    if !batch.is_empty() {
        writeln!(out, "\n⊙ work recorded this window ({}):", batch.len())?;
        for c in &batch {
            writeln!(
                out,
                "  ⊙ {}  [{} boundaries old]  → acknowledge",
                c.label, c.boundaries_open
            )?;
        }
        writeln!(
            out,
            "  (acknowledging records that work happened; it does not verify the assertions)"
        )?;
    }

    // Screen 3 — bare claims, one at a time (the sting budget)
    let bare: Vec<_> = d
        .claims
        .iter()
        .filter(|c| matches!(c.state, ClaimState::Bare | ClaimState::ExpiredBare))
        .cloned()
        .collect();
    for c in &bare {
        match &c.kind {
            Some(k) => writeln!(out, "\nbare claim [{k}]: {}", c.label)?,
            None => writeln!(out, "\nbare claim: {}", c.label)?,
        }
        writeln!(
            out,
            "  recommended: demand evidence (d) · attach (a <ref>) · hold (h) · dead (x) · carry (c)"
        )?;
        out.flush()?;
        let ans = lines
            .next()
            .transpose()
            .ok()
            .flatten()
            .unwrap_or_else(|| "c".into());
        apply_bare_answer(&ledger, root, c, &ans)?;
    }

    // Screen 4 — grey forks (presentation only; carry unless told)
    if !d.grey.is_empty() {
        writeln!(
            out,
            "\ngrey: {} held/starved (carry — review when you can)",
            d.grey.len()
        )?;
    }

    // Boundary: write the counted-set snapshot before the pause event
    if opts.boundary {
        write_boundary(&ledger, &d)?;
    }

    // Screen 5 — the receipt
    let secs = started.elapsed().as_secs();
    writeln!(
        out,
        "\nreceipt: {} bare handled · {}s elapsed",
        bare.len(),
        secs
    )?;
    writeln!(out, "labels legible? (y/n)")?;
    out.flush()?;
    let legible = lines
        .next()
        .transpose()
        .ok()
        .flatten()
        .unwrap_or_else(|| "y".into());
    ledger.append_batch(vec![NewEvent {
        etype: "pause".into(),
        actor: Actor::human(),
        body: serde_json::json!({
            "boundary": opts.boundary,
            "seconds": secs,
            "legible": legible.trim() == "y",
        }),
    }])?;
    writeln!(
        out,
        "— done. ev refreshes when invoked, not in the background."
    )?;
    let _ = opts.script;
    Ok(())
}

pub fn apply_bare_answer(
    ledger: &Ledger,
    root: &Path,
    c: &crate::state::ClaimView,
    ans: &str,
) -> Result<()> {
    let a = ans.trim();
    let human = Actor::human();
    if a == "d" {
        ledger.append_batch(vec![NewEvent {
            etype: "demand".into(),
            actor: human,
            body: serde_json::json!({ "claim": c.id }),
        }])?;
    } else if let Some(rest) = a.strip_prefix("a ") {
        crate::verify::verify_and_record(ledger, root, &c.id, rest.trim(), false, human)?;
    } else if a == "h" {
        ledger.append_batch(vec![NewEvent {
            etype: "hold".into(),
            actor: human,
            body: serde_json::json!({ "claim": c.id, "reason": "held at pause" }),
        }])?;
    } else if a == "x" {
        ledger.append_batch(vec![NewEvent {
            etype: "prune".into(),
            actor: human,
            body: serde_json::json!({ "claim": c.id, "reason": "declared dead at pause" }),
        }])?;
    }
    // "c" (carry) or anything else: no event written
    Ok(())
}

pub fn write_boundary(ledger: &Ledger, d: &Derived) -> Result<()> {
    // counted-set delta: total closed/expired now minus what prior snapshots already counted
    let prior_closed: u32 = d.snapshots.iter().map(|s| s.closed_with_evidence).sum();
    let prior_expired: u32 = d.snapshots.iter().map(|s| s.expired_bare).sum();
    let closed_now = d
        .closed
        .iter()
        .filter(|c| matches!(c.state, ClaimState::Closed))
        .count() as u32;
    let expired_now = d
        .claims
        .iter()
        .filter(|c| matches!(c.state, ClaimState::ExpiredBare))
        .count() as u32;
    let delta_closed = closed_now.saturating_sub(prior_closed);
    let delta_expired = expired_now.saturating_sub(prior_expired);
    ledger.append_batch(vec![NewEvent {
        etype: "snapshot".into(),
        actor: Actor::engine(),
        body: serde_json::json!({
            "closed_with_evidence": delta_closed,
            "expired_bare": delta_expired,
        }),
    }])?;
    Ok(())
}
