use serde::Serialize;

/// The comprehension depth of a reading slot. Ordered: a maintainer statement is the claim
/// body itself, `plain` is a non-author's read, `ground` assumes zero background. A fact about
/// which register a pointer is for — never a judgment on the pointer.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Depth {
    Maintainer,
    Plain,
    Ground,
}

impl Depth {
    pub fn parse(raw: &str) -> Option<Depth> {
        match raw {
            "maintainer" => Some(Depth::Maintainer),
            "plain" => Some(Depth::Plain),
            "ground" => Some(Depth::Ground),
            _ => None,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Depth::Maintainer => "maintainer",
            Depth::Plain => "plain",
            Depth::Ground => "ground",
        }
    }
    /// One step deeper, or `None` at the floor. `maintainer` (the claim proper) is where a
    /// drill starts; `ground` is where it stops.
    pub fn deeper(self) -> Option<Depth> {
        match self {
            Depth::Maintainer => Some(Depth::Plain),
            Depth::Plain => Some(Depth::Ground),
            Depth::Ground => None,
        }
    }
}

/// The language axis. `{zh, en}` for 0.2.4; the set can extend later.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    Zh,
    En,
}

impl Lang {
    pub fn parse(raw: &str) -> Option<Lang> {
        match raw {
            "zh" => Some(Lang::Zh),
            "en" => Some(Lang::En),
            _ => None,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Lang::Zh => "zh",
            Lang::En => "en",
        }
    }
    pub fn other(self) -> Lang {
        match self {
            Lang::Zh => Lang::En,
            Lang::En => Lang::Zh,
        }
    }
}

/// One filled slot: a pointer for one (depth, lang). The value is a REF — a `thk_` note id or a
/// `url:`/`artifact:` ref (R1) — never prose. ev stores this string and resolves it for display;
/// it never holds the explanation itself.
#[derive(Serialize, Clone, Debug)]
pub struct Slot {
    pub depth: Depth,
    pub lang: Lang,
    pub reference: String,
}

/// A claim's `reading`: a grid of POINTERS over depth × language, plus concept pointers. Only
/// FILLED slots are stored; the absence of a `(depth, lang)` is EMPTY — a fact, never a grade
/// (R2). `maintainer` is not a storable slot: it is the claim's own label/body (implicit).
#[derive(Serialize, Clone, Debug, Default)]
pub struct ReadingView {
    pub slots: Vec<Slot>,
    pub concepts: Vec<String>,
}

impl ReadingView {
    /// The four storable (depth, lang) positions. `maintainer` is excluded on purpose — it is
    /// the claim proper, filed nowhere else.
    pub const STORABLE: [(Depth, Lang); 4] = [
        (Depth::Plain, Lang::Zh),
        (Depth::Plain, Lang::En),
        (Depth::Ground, Lang::Zh),
        (Depth::Ground, Lang::En),
    ];

    pub fn get(&self, depth: Depth, lang: Lang) -> Option<&str> {
        self.slots
            .iter()
            .find(|s| s.depth == depth && s.lang == lang)
            .map(|s| s.reference.as_str())
    }

    /// Fill a slot, replacing any prior pointer for the same (depth, lang). This is the fold's
    /// "latest wins" reduction over an append-only stream — the prior EVENT is never rewritten.
    pub fn set(&mut self, depth: Depth, lang: Lang, reference: String) {
        if let Some(s) = self
            .slots
            .iter_mut()
            .find(|s| s.depth == depth && s.lang == lang)
        {
            s.reference = reference;
        } else {
            self.slots.push(Slot {
                depth,
                lang,
                reference,
            });
        }
    }

    /// The storable positions with no pointer. Present/absent only — never a quality word.
    pub fn empties(&self) -> Vec<(Depth, Lang)> {
        Self::STORABLE
            .iter()
            .copied()
            .filter(|(d, l)| self.get(*d, *l).is_none())
            .collect()
    }

    /// Filled storable slots. A count, never a score.
    pub fn present(&self) -> usize {
        Self::STORABLE
            .iter()
            .filter(|(d, l)| self.get(*d, *l).is_some())
            .count()
    }
}

/// The resolved face of a pointer, produced only at display time. ev holds the pointer; this is
/// what it shows when asked. `Dangling` is a fact (the pointer resolves to nothing), never a
/// verdict on the slot's content.
pub enum SlotDisplay<'a> {
    Note(&'a str),
    Link(String),
    Dangling(&'a str),
}

/// Resolve a slot's pointer for display. A `thk_` id resolves through the thoughts the fold
/// already carries; a `url:`/`artifact:` ref resolves to its link/path; anything else, or a
/// `thk_` id with no note, is `Dangling`. No model is called and no prose is stored — ev shows
/// what the pointer names, or states that it names nothing.
pub fn resolve_slot<'a>(
    reference: &'a str,
    thoughts: &'a [crate::state::ThoughtView],
) -> SlotDisplay<'a> {
    if reference.starts_with("thk_") {
        return match thoughts.iter().find(|t| t.id == reference) {
            Some(t) => SlotDisplay::Note(&t.label),
            None => SlotDisplay::Dangling(reference),
        };
    }
    match crate::verify::EvRef::parse(reference) {
        Ok(r) if r.kind == crate::verify::RefKind::Url => SlotDisplay::Link(r.payload),
        Ok(r) if r.kind == crate::verify::RefKind::Artifact => {
            SlotDisplay::Link(format!(".evolving/artifacts/{}", r.payload))
        }
        _ => SlotDisplay::Dangling(reference),
    }
}

/// The one flatten of a resolved pointer to its displayed string — a note's label, a link's
/// path, or the fact that the pointer resolves to nothing. Both the `ev reading` listing and the
/// pause drill show a filled slot through this, so the two cannot phrase a slot two ways.
pub fn render_slot(reference: &str, thoughts: &[crate::state::ThoughtView]) -> String {
    match resolve_slot(reference, thoughts) {
        SlotDisplay::Note(t) => t.to_string(),
        SlotDisplay::Link(l) => l,
        SlotDisplay::Dangling(p) => format!("(pointer resolves to nothing: {p})"),
    }
}

/// What the human's navigation observed on one claim this pause — a fact the disposition records
/// (Task 6), never a judgment. `none()` is the reading a disposition outside a pause carries: the
/// claim proper was seen, no language was switched, no empty slot was hit.
#[derive(Clone, Copy)]
pub struct ReadingNav {
    pub viewed_depth: Depth,
    pub lang: Option<Lang>,
    pub hit_empty: bool,
}

impl ReadingNav {
    pub fn none() -> ReadingNav {
        ReadingNav {
            viewed_depth: Depth::Maintainer,
            lang: None,
            hit_empty: false,
        }
    }
}

/// The cognitive-debt count for a claim: how many commits have touched its anchored path since the
/// human last understood it — its most recent `ack`, or its filing `base` if never acked. READS
/// the `drift` the pair already computed on each `EvidenceView` (drift is counted from `last_ack`
/// first, else `base` — the pair's own rule); it NEVER reads `cell`/`neighborhood-moved`, NEVER
/// modifies the pair, and NEVER re-decides earn (R3). `None` when nothing moved: a non-moved claim
/// carries no debt and there is nothing to state.
pub fn cognitive_debt(c: &crate::state::ClaimView) -> Option<u32> {
    c.max_drift().filter(|&n| n > 0)
}

/// One phrasing for the debt fact everywhere it is shown. A count, never a verdict.
pub fn debt_phrase(n: u32) -> String {
    format!("last understood {n} commit(s) ago — re-read")
}

/// The empty-slot census over a set of claims: counts, never a grade (R2). `present`/`empty`
/// are totals of filled/unfilled storable slots; `by_slot` is the EMPTY count at each storable
/// position. The D4 instrument — recorded per round (at each boundary pause) and printed by
/// `ev doctor`. Emit-only where it is recorded on the ledger: nothing in this crate reads a
/// `reading_census` event back.
pub struct ReadingCensus {
    pub claims: usize,
    pub empty: usize,
    pub by_slot: Vec<(Depth, Lang, usize)>,
}

/// Count the reading grid over these claims. No claim is dropped from the denominator: a claim
/// with no reading at all counts as four empty slots, which is the fact the census exists to
/// surface.
pub fn census_of(claims: &[crate::state::ClaimView]) -> ReadingCensus {
    let mut empty = 0usize;
    let mut by_slot: Vec<(Depth, Lang, usize)> = ReadingView::STORABLE
        .iter()
        .map(|(d, l)| (*d, *l, 0usize))
        .collect();
    for c in claims {
        for (i, (d, l)) in ReadingView::STORABLE.iter().enumerate() {
            if c.reading.get(*d, *l).is_none() {
                empty += 1;
                by_slot[i].2 += 1;
            }
        }
    }
    ReadingCensus {
        claims: claims.len(),
        empty,
        by_slot,
    }
}

impl ReadingCensus {
    /// Filled storable slots across the censused claims. Derived: every claim contributes
    /// `STORABLE.len()` slots by construction, so present is the complement of empty. A count,
    /// never a score.
    pub fn present(&self) -> usize {
        self.claims * ReadingView::STORABLE.len() - self.empty
    }

    /// The ledger event body. Facts only — present/empty totals and per-slot empty counts.
    pub fn to_body(&self) -> serde_json::Value {
        let by_slot: serde_json::Map<String, serde_json::Value> = self
            .by_slot
            .iter()
            .map(|(d, l, n)| {
                (
                    format!("{}/{}", d.as_str(), l.as_str()),
                    serde_json::json!(n),
                )
            })
            .collect();
        serde_json::json!({
            "claims": self.claims,
            "present": self.present(),
            "empty": self.empty,
            "by_slot": by_slot,
        })
    }
}
