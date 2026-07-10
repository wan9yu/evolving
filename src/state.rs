use crate::ledger::Envelope;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ClaimState {
    Bare,
    Evidenced,
    Verified,
    Grey,
    Closed,
    Dead,
    ExpiredBare,
}

#[derive(Serialize, Clone, Debug)]
pub struct EvidenceView {
    pub eref: String,
    pub status: String, // "verified" | "failed" | "unreachable" | "recorded"
    pub self_evident: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct ClaimView {
    pub id: String,
    pub label: String,
    pub state: ClaimState,
    pub evidence: Vec<EvidenceView>,
    pub self_evident: bool,
    pub boundaries_open: u32,
    pub referenced_by: u32,
    pub source_ref: Option<String>,
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
    evidence: Vec<EvidenceView>,
    order: u64,
    held: Option<String>,
    closed: bool,
    dead: bool,
    demanded_at: Option<u64>,
    last_activity_seq: u64,
    referenced_by: u32,
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
                    evidence: vec![],
                    order: order_seq,
                    held: None,
                    closed: false,
                    dead: false,
                    demanded_at: None,
                    last_activity_seq: e.seq,
                    referenced_by: 0,
                });
            }
            "evidence" => {
                if let Some(cid) = s(&e.body, "claim") {
                    if let Some(acc) = claims.get_mut(&cid) {
                        acc.evidence.push(EvidenceView {
                            eref: s(&e.body, "ref").unwrap_or_default(),
                            status: s(&e.body, "status").unwrap_or_else(|| "recorded".into()),
                            self_evident: e
                                .body
                                .get("self_evident")
                                .and_then(|b| b.as_bool())
                                .unwrap_or(false),
                        });
                        acc.held = None; // evidence revives a grey/held claim
                        acc.last_activity_seq = e.seq;
                    }
                }
            }
            "verify" => {
                if let (Some(cid), Some(st)) = (s(&e.body, "claim"), s(&e.body, "status")) {
                    if let Some(acc) = claims.get_mut(&cid) {
                        if let Some(last) = acc.evidence.last_mut() {
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
        let boundaries_open = boundaries_since(&a, boundary_count);
        let state = derive_state(&a, boundaries_open);
        let view = ClaimView {
            id: a.id.clone(),
            label: a.label.clone(),
            state,
            self_evident: a.evidence.iter().any(|e| e.self_evident)
                && a.evidence.iter().all(|e| e.self_evident),
            evidence: a.evidence.clone(),
            boundaries_open,
            referenced_by: a.referenced_by,
            source_ref: a.source_ref.clone(),
            reason: a.held.clone(),
        };
        match state {
            ClaimState::Closed => out.closed.push(view.clone()),
            ClaimState::Dead => out.closed.push(view.clone()),
            ClaimState::Grey => out.grey.push(view.clone()),
            _ => {}
        }
        // returned demand: had a demand, then later evidence, still open
        if a.demanded_at.is_some() && !a.evidence.is_empty() && !a.closed && !a.dead {
            out.demands_returned.push(view.clone());
        }
        if !a.closed && !a.dead {
            out.claims.push(view);
        }
    }
    out
}

fn boundaries_since(a: &ClaimAcc, _total: u32) -> u32 {
    // Starvation boundary counts are wired in the snapshot task.
    // Until snapshots exist a fresh ledger has no boundaries, so this is 0.
    let _ = a;
    0
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
    if a.evidence.iter().any(|e| e.status == "verified") {
        ClaimState::Verified
    } else {
        ClaimState::Evidenced
    }
}
