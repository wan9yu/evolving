use crate::ledger::Envelope;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ClaimState {
    Bare,
    Evidenced,
    Anchored,
    Grey,
    Closed,
    Dead,
    ExpiredBare,
}

#[derive(Serialize, Clone, Debug)]
pub struct EvidenceView {
    pub eref: String,
    /// What ev found at the anchor. Carried as the class itself, so a reader that
    /// buckets it cannot silently fold a value it does not know into one it does.
    pub status: crate::verify::Status,
    pub self_evident: bool,
    /// The repo state (HEAD sha) the anchor was filed against — drift's zero point.
    pub base: Option<String>,
    /// World movement under the anchor: commits touching the cited path beyond
    /// the base. Filled by drift annotation at read time; the fold leaves it None.
    pub drift: Option<u32>,
    /// What it would take for this anchor to go red — a fact about the pointer's
    /// shape. Derived from the ref, so the fold stays pure. Carried as the class
    /// itself, so a reader that counts them cannot silently bucket an unknown one.
    pub liveness: crate::verify::Liveness,
}

#[derive(Serialize, Clone, Debug)]
pub struct ClaimView {
    pub id: String,
    pub label: String,
    pub state: ClaimState,
    pub evidence: Vec<EvidenceView>,
    pub self_evident: bool,
    pub boundaries_open: u32,
    pub source_ref: Option<String>,
    /// Declared claim kind (e.g. "defect", "priority") — a filing fact, not a verdict.
    pub kind: Option<String>,
    pub reason: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct ThoughtView {
    pub id: String,
    pub label: String,
    pub pinned: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct IndicatorView {
    pub id: String,
    pub name: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct Snapshot {
    pub id: String,
    pub ts: String,
    pub closed_with_evidence: u32,
    pub expired_bare: u32,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Derived {
    pub claims: Vec<ClaimView>,
    pub closed: Vec<ClaimView>,
    pub grey: Vec<ClaimView>,
    pub thoughts: Vec<ThoughtView>,
    pub demands_returned: Vec<ClaimView>,
    pub indicators: Vec<IndicatorView>,
    pub snapshots: Vec<Snapshot>,
    pub last_event_id: Option<String>,
    pub boundary_count: u32,
}

// internal accumulator per claim during the fold
struct ClaimAcc {
    id: String,
    label: String,
    source_ref: Option<String>,
    kind: Option<String>,
    evidence: Vec<EvidenceView>,
    order: u64,
    held: Option<String>,
    closed: bool,
    dead: bool,
    demanded_at: Option<u64>,
    last_activity_seq: u64,
    opened_at_boundary: u32,
}

fn s(v: &serde_json::Value, k: &str) -> Option<String> {
    v.get(k).and_then(|x| x.as_str()).map(|x| x.to_string())
}

pub fn fold(events: &[Envelope]) -> Derived {
    let mut claims: HashMap<String, ClaimAcc> = HashMap::new();
    let mut order_seq = 0u64;
    let mut thoughts: Vec<ThoughtView> = Vec::new();
    let mut indicators: Vec<IndicatorView> = Vec::new();
    let mut snapshots: Vec<Snapshot> = Vec::new();
    let mut boundary_count = 0u32;
    let mut last_event_id = None;

    for e in events {
        last_event_id = Some(e.id.clone());
        match e.etype.as_str() {
            "claim" => {
                order_seq += 1;
                let id = e.id.clone();
                claims.entry(id.clone()).or_insert(ClaimAcc {
                    id,
                    label: s(&e.body, "label").unwrap_or_default(),
                    source_ref: s(&e.body, "source_ref"),
                    kind: s(&e.body, "kind"),
                    evidence: vec![],
                    order: order_seq,
                    held: None,
                    closed: false,
                    dead: false,
                    demanded_at: None,
                    last_activity_seq: e.seq,
                    opened_at_boundary: boundary_count,
                });
            }
            "evidence" => {
                if let Some(cid) = s(&e.body, "claim") {
                    if let Some(acc) = claims.get_mut(&cid) {
                        let eref = s(&e.body, "ref").unwrap_or_default();
                        // Unparseable is the honest fallback for a ref no current
                        // grammar accepts — an old ledger must never panic the fold.
                        let liveness = crate::verify::EvRef::parse(&eref)
                            .map(|r| crate::verify::Liveness::of(&r))
                            .unwrap_or(crate::verify::Liveness::Unparseable);
                        acc.evidence.push(EvidenceView {
                            eref,
                            // An event with no status at all predates the field:
                            // it recorded the ref and nothing more.
                            status: s(&e.body, "status")
                                .map(|raw| crate::verify::Status::parse(&raw))
                                .unwrap_or(crate::verify::Status::Recorded),
                            self_evident: e
                                .body
                                .get("self_evident")
                                .and_then(|b| b.as_bool())
                                .unwrap_or(false),
                            base: s(&e.body, "base"),
                            drift: None,
                            liveness,
                        });
                        acc.held = None; // evidence revives a grey/held claim
                        acc.last_activity_seq = e.seq;
                    }
                }
            }
            "verify" => {
                if let (Some(cid), Some(st)) = (s(&e.body, "claim"), s(&e.body, "status")) {
                    if let Some(acc) = claims.get_mut(&cid) {
                        // refs should be unique per claim; rev() picks the most recent if not.
                        let st = crate::verify::Status::parse(&st);
                        if let Some(r) = e.body.get("ref").and_then(|v| v.as_str()) {
                            if let Some(item) =
                                acc.evidence.iter_mut().rev().find(|ev| ev.eref == r)
                            {
                                item.status = st;
                            }
                        } else if let Some(last) = acc.evidence.last_mut() {
                            last.status = st;
                        }
                        acc.last_activity_seq = e.seq;
                    }
                }
            }
            "hold" => {
                if let Some(cid) = s(&e.body, "claim") {
                    if let Some(acc) = claims.get_mut(&cid) {
                        acc.held = Some(s(&e.body, "reason").unwrap_or_default());
                        acc.last_activity_seq = e.seq;
                    }
                }
            }
            "close" => {
                if let Some(cid) = s(&e.body, "claim") {
                    if let Some(acc) = claims.get_mut(&cid) {
                        acc.closed = true;
                        acc.last_activity_seq = e.seq;
                    }
                }
            }
            "prune" => {
                if let Some(cid) = s(&e.body, "claim") {
                    if let Some(acc) = claims.get_mut(&cid) {
                        acc.dead = true;
                    }
                }
            }
            "demand" => {
                if let Some(cid) = s(&e.body, "claim") {
                    if let Some(acc) = claims.get_mut(&cid) {
                        acc.demanded_at = Some(e.seq);
                    }
                }
            }
            "thought" => {
                thoughts.push(ThoughtView {
                    id: e.id.clone(),
                    label: s(&e.body, "label").unwrap_or_default(),
                    pinned: e
                        .body
                        .get("pinned")
                        .and_then(|b| b.as_bool())
                        .unwrap_or(false),
                });
            }
            "indicator" => {
                indicators.push(IndicatorView {
                    id: e.id.clone(),
                    name: s(&e.body, "name").unwrap_or_default(),
                });
            }
            "retire" => {
                if let Some(id) = e.body.get("indicator").and_then(|v| v.as_str()) {
                    indicators.retain(|i| i.id != id);
                }
            }
            "snapshot" => {
                snapshots.push(Snapshot {
                    id: e.id.clone(),
                    ts: e.ts.clone(),
                    closed_with_evidence: e
                        .body
                        .get("closed_with_evidence")
                        .and_then(|n| n.as_u64())
                        .unwrap_or(0) as u32,
                    expired_bare: e
                        .body
                        .get("expired_bare")
                        .and_then(|n| n.as_u64())
                        .unwrap_or(0) as u32,
                });
            }
            "pause"
                if e.body
                    .get("boundary")
                    .and_then(|b| b.as_bool())
                    .unwrap_or(false) =>
            {
                boundary_count += 1;
            }
            _ => {}
        }
    }

    let mut accs: Vec<ClaimAcc> = claims.into_values().collect();
    accs.sort_by_key(|a| a.order);

    let mut out = Derived {
        boundary_count,
        last_event_id,
        thoughts,
        indicators,
        snapshots,
        ..Default::default()
    };

    for a in accs {
        let boundaries_open = boundary_count.saturating_sub(a.opened_at_boundary);
        let state = derive_state(&a, boundaries_open);
        let view = ClaimView {
            id: a.id.clone(),
            label: a.label.clone(),
            state,
            self_evident: !a.evidence.is_empty() && a.evidence.iter().all(|e| e.self_evident),
            evidence: a.evidence.clone(),
            boundaries_open,
            source_ref: a.source_ref.clone(),
            kind: a.kind.clone(),
            reason: a.held.clone(),
        };
        match state {
            ClaimState::Closed | ClaimState::Dead => out.closed.push(view.clone()),
            ClaimState::Grey => out.grey.push(view.clone()),
            _ => {}
        }
        // returned demand: had a demand, then later evidence, still open
        if a.demanded_at.is_some() && !a.evidence.is_empty() && !a.closed && !a.dead {
            out.demands_returned.push(view.clone());
        }
        if !a.closed && !a.dead && state != ClaimState::Grey {
            out.claims.push(view);
        }
    }
    out
}

// ── the baseline: where the ledger began ──────────────────────────────────────
// A fact about an event stream, not a verb — so it reads off `&[Envelope]` here,
// beside the fold, rather than from the CLI layer that happens to print it.

/// The `head` recorded by the most recent baseline marker — a `session` event
/// whose body is `{"marker":"baseline","head":<sha|"ROOT">}`. `None` when the
/// ledger carries no baseline at all.
///
/// The one lookup for every site that must agree on where the ledger began:
/// `hooks::sweep` (the watermark's fallback), `cmd::exhaust` (the start of a
/// `--since ROOT` window), `cmd::init` (skip a redundant write) and `cmd::doctor`
/// (report the ledger's shape). Two lookups could disagree; one cannot.
///
/// `"ROOT"` is a truthful value, not an absence: an `ev init` in a repo with no
/// commits records it, and a window that starts there covers the whole history
/// precisely because the ledger predates none of it.
pub(crate) fn baseline_head(events: &[Envelope]) -> Option<String> {
    events
        .iter()
        .filter(|e| {
            e.etype == "session"
                && e.body.get("marker").and_then(|s| s.as_str()) == Some("baseline")
        })
        .max_by_key(|e| (e.ts.clone(), e.seq))
        .and_then(|e| {
            e.body
                .get("head")
                .and_then(|h| h.as_str())
                .map(String::from)
        })
}

/// Whether the ledger already carries a baseline marker. Expressed in terms of
/// `baseline_head` so the predicate and the value can never disagree.
pub(crate) fn has_baseline(events: &[Envelope]) -> bool {
    baseline_head(events).is_some()
}

/// The refusal a ledger with no baseline earns: the shape fact ev has checked
/// (the marker is absent), never a consequence it has not (what would be filed).
/// One string, so `cmd::exhaust` and `hooks::sweep` refuse in the same words.
pub(crate) fn no_baseline_refusal() -> crate::EvError {
    crate::EvError::Refusal(
        "this ledger carries no baseline marker; ev cannot tell where the session's own \
         commits begin.\n    \
         Run `ev baseline [<sha>]` to record where the ledger began."
            .into(),
    )
}

fn derive_state(a: &ClaimAcc, boundaries_open: u32) -> ClaimState {
    if a.dead {
        return ClaimState::Dead;
    }
    if a.closed {
        return ClaimState::Closed;
    }
    if a.held.is_some() {
        return ClaimState::Grey;
    }
    if a.evidence.is_empty() {
        if boundaries_open >= 2 {
            return ClaimState::ExpiredBare;
        }
        return ClaimState::Bare;
    }
    if a.evidence
        .iter()
        .any(|e| e.status == crate::verify::Status::Resolves)
    {
        ClaimState::Anchored
    } else {
        ClaimState::Evidenced
    }
}
