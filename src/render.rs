use crate::state::{ClaimState, ClaimView, Derived};

pub const FOOTER: &str = "ev refreshes when invoked, not in the background.";

pub fn as_of(d: &Derived) -> String {
    d.last_event_id.clone().unwrap_or_else(|| "—".into())
}

pub fn brief(d: &Derived, json: bool) -> String {
    if json {
        let v = serde_json::json!({
            "demands_returned": ids(&d.demands_returned),
            "open": d.claims.iter().map(claim_json).collect::<Vec<_>>(),
            "grey": ids(&d.grey),
            "pinned": d.thoughts.iter().filter(|t| t.pinned).map(|t| &t.label).collect::<Vec<_>>(),
            "as_of": as_of(d),
        });
        return format!("{}\n", serde_json::to_string_pretty(&v).unwrap());
    }
    let mut out = String::new();
    if !d.demands_returned.is_empty() {
        out.push_str(&format!(
            "↩ {} demand(s) answered — review at pause\n",
            d.demands_returned.len()
        ));
    }
    let shown = d.claims.iter().take(12);
    out.push_str(&format!("open claims: {}\n", d.claims.len()));
    for c in shown {
        out.push_str(&format!(
            "  {} {}  [{}]\n",
            mark(c.self_evident, &c.state),
            truncate(&c.label, 60),
            state_word(&c.state)
        ));
    }
    if !d.grey.is_empty() {
        out.push_str(&format!("grey (held/starved): {}\n", d.grey.len()));
    }
    out.push_str(&format!("— as of {} · {}\n", short_id(&as_of(d)), FOOTER));
    out
}

fn claim_json(c: &ClaimView) -> serde_json::Value {
    let evidence: Vec<serde_json::Value> = c
        .evidence
        .iter()
        .map(|e| {
            let mut v = serde_json::json!({
                "ref": e.eref,
                "status": e.status,
                "self_evident": e.self_evident,
            });
            if let Some(base) = &e.base {
                v["base"] = serde_json::json!(base);
            }
            v
        })
        .collect();
    let mut v = serde_json::json!({
        "id": c.id,
        "label": c.label,
        "state": state_word(&c.state),
        "self_evident": c.self_evident,
        "evidence": evidence,
    });
    if let Some(kind) = &c.kind {
        v["kind"] = serde_json::json!(kind);
    }
    v
}

fn ids(v: &[ClaimView]) -> Vec<String> {
    v.iter().map(|c| c.id.clone()).collect()
}

pub fn mark(self_evident: bool, state: &ClaimState) -> char {
    match state {
        ClaimState::Anchored if self_evident => '⊙',
        ClaimState::Anchored => '✓',
        ClaimState::Bare | ClaimState::ExpiredBare => '·',
        _ => '–',
    }
}

pub fn state_word(s: &ClaimState) -> &'static str {
    match s {
        ClaimState::Bare => "bare",
        ClaimState::Evidenced => "evidenced",
        ClaimState::Anchored => "anchored",
        ClaimState::Grey => "grey",
        ClaimState::Closed => "closed",
        ClaimState::Dead => "dead",
        ClaimState::ExpiredBare => "expired-bare",
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(n - 1).collect::<String>())
    }
}

fn short_id(id: &str) -> String {
    crate::short_id(id)
}

/// Render the work line: closed-with-evidence and expired-bare counts, no percentage or composite.
pub fn line(d: &Derived, json: bool, stable: bool) -> String {
    // Current live counts. "closed" = claims whose state is Closed (have evidence).
    // The snapshot rows below carry the boundary history; they are not summed in here.
    let closed_now = d
        .closed
        .iter()
        .filter(|c| matches!(c.state, ClaimState::Closed) && !c.evidence.is_empty())
        .count() as u32;
    let expired_now = d
        .claims
        .iter()
        .filter(|c| matches!(c.state, ClaimState::ExpiredBare))
        .count() as u32;

    if json {
        let as_of_val = if stable { "<id>".to_string() } else { as_of(d) };
        let snaps: Vec<serde_json::Value> = if stable {
            vec![]
        } else {
            d.snapshots
                .iter()
                .map(|s| {
                    serde_json::json!({
                        "closed_with_evidence": s.closed_with_evidence,
                        "expired_bare": s.expired_bare,
                    })
                })
                .collect()
        };
        let v = serde_json::json!({
            "indicators": [
                {
                    "name": "work",
                    "closed_with_evidence": closed_now,
                    "expired_bare": expired_now,
                }
            ],
            "snapshots": snaps,
            "as_of": as_of_val,
        });
        return format!("{}\n", serde_json::to_string_pretty(&v).unwrap());
    }

    // Terminal form: one honest line per snapshot + a "now" line, no percentage.
    let mut out = String::new();
    out.push_str("work line\n");
    for s in &d.snapshots {
        out.push_str(&format!(
            "  ▪ closed {}  · expired-bare {}\n",
            s.closed_with_evidence, s.expired_bare
        ));
    }
    out.push_str(&format!(
        "  now: {closed_now} closed-with-evidence · {expired_now} expired-bare\n"
    ));
    out.push_str(&format!("— as of {} · {}\n", short_id(&as_of(d)), FOOTER));
    out
}
