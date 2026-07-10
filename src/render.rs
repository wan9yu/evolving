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
    serde_json::json!({
        "id": c.id,
        "label": c.label,
        "state": state_word(&c.state),
        "self_evident": c.self_evident,
        "evidence": c.evidence.len(),
    })
}

fn ids(v: &[ClaimView]) -> Vec<String> {
    v.iter().map(|c| c.id.clone()).collect()
}

pub fn mark(self_evident: bool, state: &ClaimState) -> char {
    match state {
        ClaimState::Verified if self_evident => '⊙',
        ClaimState::Verified => '✓',
        ClaimState::Bare | ClaimState::ExpiredBare => '·',
        _ => '–',
    }
}

pub fn state_word(s: &ClaimState) -> &'static str {
    match s {
        ClaimState::Bare => "bare",
        ClaimState::Evidenced => "evidenced",
        ClaimState::Verified => "verified",
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
    match id.split_once('_') {
        Some((p, r)) => format!("{p}_{}", &r[..r.len().min(6)]),
        None => id.to_string(),
    }
}
