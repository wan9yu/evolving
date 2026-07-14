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
    crate::verify::annotate(&mut d, root);
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

    // Screen 1.5 — code moved under these claims since the last look.
    // ev cannot know whether the movement matters — only that it happened. RE-READ.
    // The screen is admitted on the claim's WORST cell, ranked by the ONE ordering: a claim
    // has a cell that asks to be re-read exactly when its most severe one does. Carrying the
    // cell out of the filter is what leaves no claim on this screen whose cell ev never
    // classified — there is no phrase for such a claim, and ev prints none.
    let moved: Vec<(&crate::state::ClaimView, crate::verify::Cell)> = d
        .claims
        .iter()
        .filter_map(|c| {
            let worst = c.worst_cell()?;
            worst.asks_reread().then_some((c, worst))
        })
        .collect();
    if !moved.is_empty() {
        writeln!(
            out,
            "\n↗ code moved under these claims since the last look:"
        )?;
        for (c, worst) in &moved {
            // One phrasing for a cell wherever it is shown.
            writeln!(out, "  {} — {}", c.label, worst.why())?;
            // `k` (still stands → ack) is offered ONLY where an ack can clear the cell.
            // A changed or gone anchor is a broken pointer: `Cell::of` does not read drift
            // for it, so no ack moves it, and offering the key would invite the human to
            // clear a red that structurally cannot be cleared. The claim stays visible and
            // keeps h/d/skip; the honest move is to re-file the anchor.
            let ackable = worst.clearable_by_ack();
            if ackable {
                write!(
                    out,
                    "    [k] still stands · [h]old · [d]emand · enter to skip → "
                )?;
            } else {
                writeln!(
                    out,
                    "    the anchor itself is broken — no acknowledgement clears it. Re-file it: ev evidence {} <ref>",
                    crate::cmd::short(&c.id)
                )?;
                write!(out, "    [h]old · [d]emand · enter to skip → ")?;
            }
            out.flush()?;
            let ans = lines.next().transpose()?.unwrap_or_default();
            apply_moved_answer(root, &ledger, &c.id, ans.trim(), ackable)?;
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
        crate::cmd::dispose(
            ledger,
            root,
            "demand",
            &c.id,
            human,
            serde_json::json!({}),
            None,
        )?;
    } else if let Some(rest) = a.strip_prefix("a ") {
        crate::verify::verify_and_record(ledger, root, &c.id, rest.trim(), false, human)?;
    } else if a == "h" {
        crate::cmd::dispose(
            ledger,
            root,
            "hold",
            &c.id,
            human,
            serde_json::json!({ "reason": "held at pause" }),
            None,
        )?;
    } else if a == "x" {
        crate::cmd::dispose(
            ledger,
            root,
            "prune",
            &c.id,
            human,
            serde_json::json!({ "reason": "declared dead at pause" }),
            None,
        )?;
    }
    // "c" (carry) or anything else: no event written
    Ok(())
}

/// The human's verdict on a claim whose code moved. ev records what the human decided;
/// it never decides. `k` is the disposition the set was missing: looked, still stands.
///
/// Mirrors `cmd::ack` exactly: `head` is present only when git resolves HEAD. A
/// sentinel there (e.g. "ROOT") would make git fail to resolve it forever, poisoning
/// drift and permanently disarming the ratchet — the Critical Task 5 fixed.
///
/// `ackable` says whether an ack can clear this claim's cell. When it cannot, `k` writes
/// NOTHING: an ack recorded against a gone anchor would be an event whose only effect is
/// to make the human believe the red was handled.
fn apply_moved_answer(
    root: &Path,
    ledger: &Ledger,
    claim_id: &str,
    ans: &str,
    ackable: bool,
) -> Result<()> {
    let human = Actor::human();
    match ans {
        "k" if !ackable => {
            // The answer lands on the same line as the prompt, which is left un-newlined.
            println!(
                "\n    the anchor is broken; an acknowledgement cannot make the cited code exist. Nothing recorded."
            );
        }
        "k" => {
            let head = crate::git_output(root, &["rev-parse", "HEAD"]);
            let mut extra = serde_json::json!({});
            if let Some(h) = &head {
                extra["head"] = serde_json::json!(h);
            }
            crate::cmd::dispose(ledger, root, "ack", claim_id, human, extra, None)?;
        }
        "h" => {
            crate::cmd::dispose(
                ledger,
                root,
                "hold",
                claim_id,
                human,
                serde_json::json!({ "reason": "held at pause after movement" }),
                None,
            )?;
        }
        "d" => {
            crate::cmd::dispose(
                ledger,
                root,
                "demand",
                claim_id,
                human,
                serde_json::json!({}),
                None,
            )?;
        }
        _ => {} // enter, or anything else: carry. No event written.
    }
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
