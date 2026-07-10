use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ActorKind {
    Human,
    Agent,
    Engine,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Actor {
    pub kind: ActorKind,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub via: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Envelope {
    pub v: u8,
    pub id: String,
    pub ts: String,
    pub writer: String,
    pub seq: u64,
    pub actor: Actor,
    #[serde(rename = "type")]
    pub etype: String,
    pub body: serde_json::Value,
}

/// Three-letter stable prefix per event type. The table is frozen from day one;
/// unknown types fall back to their first three letters.
pub fn prefix_for(etype: &str) -> String {
    let p = match etype {
        "thought" => "thk",
        "pull" => "pul",
        "promote" => "pro",
        "claim" => "clm",
        "evidence" => "evd",
        "verify" => "vfy",
        "close" => "cls",
        "hold" => "hld",
        "renew" => "ren",
        "prune" => "prn",
        "demand" => "dmd",
        "indicator" => "ind",
        "retire" => "ret",
        "repwindow" => "rpw",
        "repclose" => "rpc",
        "snapshot" => "snp",
        "pause" => "pau",
        "cadence" => "cad",
        "session" => "ses",
        other => &other[..other.len().min(3)],
    };
    p.to_string()
}

pub fn mint_id(etype: &str) -> String {
    format!("{}_{}", prefix_for(etype), ulid::Ulid::new())
}
