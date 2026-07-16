use serde::{Deserialize, Serialize};

/// The comprehension depth of a reading slot. Ordered: a maintainer statement is the claim
/// body itself, `plain` is a non-author's read, `ground` assumes zero background. A fact about
/// which register a pointer is for — never a judgment on the pointer.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
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
    /// A rank for "deepest viewed this pause" — a fact recorded by the instrumentation, not a
    /// score. `maintainer` 0, `plain` 1, `ground` 2.
    pub fn ordinal(self) -> u8 {
        match self {
            Depth::Maintainer => 0,
            Depth::Plain => 1,
            Depth::Ground => 2,
        }
    }
}

/// The language axis. `{zh, en}` for 0.2.4; the set can extend later.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
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
