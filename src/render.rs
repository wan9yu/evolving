//! The human-path renderer — colour, glyphs, and the unified line grammar for the read surfaces
//! (check / brief / list / log / show). It exists behind ONE gate, the `Painter`: when `rich` is
//! false every surface emits today's exact bytes (so a pipe / redirect / `NO_COLOR` / `--plain` / CI
//! stays byte-stable and scriptable); when `rich` is true the same data renders with style.
//!
//! The machine path (`--json`, `events.jsonl`, `state.json`) NEVER constructs a `Painter` and never
//! calls anything here — a colour escape is structurally impossible there.
//!
//! Colour is by NAMED ANSI slot by default, so ev adapts to the user's terminal theme; truecolor hex
//! is an opt-in fallback (`EV_TRUECOLOR`). Weight (bold/dim) carries the hierarchy, so the rich form
//! stays legible in monochrome. Glyphs are redundant with the word and degrade to ASCII (`EV_ASCII`).

use crate::verdict::Verdict;
use owo_colors::{AnsiColors, OwoColorize, Style};

/// The `--color` choice (mirrors the common `auto|always|never`). `auto` = colour only on a TTY;
/// `always` forces it (for `| less -R`); `never` is the same as `--plain` for colour purposes.
#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum ColorChoice {
    Auto,
    Always,
    Never,
}

/// The three verdict/provenance MEANING-classes — NOT a severity ladder. `Attention` ("a human
/// should look here") is shared by every gating-not-green verdict, all honest debt, and
/// agent-proposed/awaiting; the three are co-equal facts, never a graded scale.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Class {
    Attention,
    Clear,
    Informational,
}

impl Class {
    fn ansi(self) -> AnsiColors {
        match self {
            Class::Attention => AnsiColors::Yellow,          // slot 3
            Class::Clear => AnsiColors::Green,               // slot 2
            Class::Informational => AnsiColors::BrightBlack, // slot 8
        }
    }
    /// Dark-ground truecolor fallback (the design doc's hex). Light-ground hex is a future refinement;
    /// a light-terminal user keeps the named-ANSI default, which already adapts to the theme.
    fn rgb_dark(self) -> (u8, u8, u8) {
        match self {
            Class::Attention => (0xD6, 0xA0, 0x50),
            Class::Clear => (0x8B, 0xA4, 0x7C),
            Class::Informational => (0x85, 0x7F, 0x75),
        }
    }
}

/// The meaning-class of a verdict. Gating-not-green → Attention (decision-invitations, one calm hue);
/// green → Clear; the non-gating facts (exempt / memo / n/a) → Informational. Honest debt (the
/// harvested line, and `dirty` — a green resting on uncommitted code) is styled Attention so it is
/// never quieter than a green, even though `dirty` does not gate.
pub fn class_of(v: &Verdict) -> Class {
    match v {
        Verdict::Red
        | Verdict::GrayRed
        | Verdict::NotRun { .. }
        | Verdict::Stale { .. }
        | Verdict::Unproven
        | Verdict::Dirty
        | Verdict::SilentlyUnbound => Class::Attention,
        Verdict::Green => Class::Clear,
        Verdict::Exempt | Verdict::Memo | Verdict::NotApplicable => Class::Informational,
    }
}

/// The resolved render decision, computed ONCE (in `main`) from the flags + environment + whether
/// stdout is a TTY, then handed to each command. `rich == false` ⇒ the command emits today's bytes.
#[derive(Copy, Clone, Debug)]
pub struct Painter {
    pub rich: bool,
    truecolor: bool,
    ascii: bool,
}

impl Painter {
    /// Resolve against the real process: stdout-is-a-TTY + `NO_COLOR` / `EV_TRUECOLOR` / `EV_ASCII`.
    pub fn resolve(choice: ColorChoice, plain: bool) -> Painter {
        use std::io::IsTerminal;
        Painter::compute(
            choice,
            plain,
            std::io::stdout().is_terminal(),
            std::env::var_os("NO_COLOR").is_some(),
            std::env::var_os("EV_TRUECOLOR").is_some(),
            env_flag("EV_ASCII"),
        )
    }

    /// The pure gate (testable). Rich is OFF for `--plain`, for `NO_COLOR`, for `--color=never`, and
    /// for a non-TTY under `auto` — every one of those paths emits today's exact bytes. Rich is ON
    /// only on a colour TTY (`auto`) or when explicitly forced (`always`, e.g. piping to `less -R`).
    pub fn compute(
        choice: ColorChoice,
        plain: bool,
        stdout_tty: bool,
        no_color: bool,
        truecolor: bool,
        ascii: bool,
    ) -> Painter {
        let rich = !plain
            && !no_color
            && match choice {
                ColorChoice::Never => false,
                ColorChoice::Always => true,
                ColorChoice::Auto => stdout_tty,
            };
        Painter {
            rich,
            truecolor: truecolor && rich,
            ascii,
        }
    }

    fn class_style(&self, class: Class) -> Style {
        if self.truecolor {
            let (r, g, b) = class.rgb_dark();
            Style::new().truecolor(r, g, b)
        } else {
            Style::new().color(class.ansi())
        }
    }

    /// The verdict glyph that leads a check row (◆ attention / ◇ clear / · informational). Single-cell,
    /// redundant with the word, ASCII when `EV_ASCII` is set.
    pub fn verdict_glyph(&self, class: Class) -> &'static str {
        match (class, self.ascii) {
            (Class::Attention, false) => "◆",
            (Class::Attention, true) => "*",
            (Class::Clear, false) => "◇",
            (Class::Clear, true) => "o",
            (Class::Informational, false) => "·",
            (Class::Informational, true) => ".",
        }
    }

    /// The provenance glyph that leads a decision row: solid ● = human-authored, hollow ○ = an
    /// agent proposal awaiting ratification. A DIFFERENT family from the verdict glyphs (they never
    /// share a line), so the two honesty axes never blur.
    pub fn prov_glyph(&self, agent_proposed: bool) -> &'static str {
        match (agent_proposed, self.ascii) {
            (false, false) => "●",
            (false, true) => "*",
            (true, false) => "○",
            (true, true) => "o",
        }
    }

    /// Colour `text` by its meaning-class (the only coloured word on a check row is the verdict).
    pub fn class(&self, text: &str, class: Class) -> String {
        text.style(self.class_style(class)).to_string()
    }

    /// The decision name — the headline: bold, default colour (never hue-coloured).
    pub fn name(&self, text: &str) -> String {
        text.style(Style::new().bold()).to_string()
    }

    /// sha · time · authority · blame — dim, recedes behind the name.
    pub fn meta(&self, text: &str) -> String {
        text.style(Style::new().dimmed()).to_string()
    }

    /// The full 12-hex id, never truncated: a weighted prefix (bold) + a dim tail, a visual aid for
    /// copy-paste, never an abbreviation. (A fixed 4-char prefix approximates the shortest-unique
    /// prefix; a true shortest-unique computation across the shown set is a later refinement.)
    pub fn id(&self, id: &str) -> String {
        let n = id.len().min(4);
        let (prefix, tail) = id.split_at(n);
        format!(
            "{}{}",
            prefix.style(Style::new().bold()),
            tail.style(Style::new().dimmed())
        )
    }
}

/// `EV_ASCII` (and friends) are true when set to a non-empty, non-`0` value.
fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    // The gate is the load-bearing contract: anything that is NOT a colour TTY must stay byte-stable.
    #[test]
    fn painter_should_be_rich_on_a_color_tty_under_auto() {
        let p = Painter::compute(ColorChoice::Auto, false, true, false, false, false);
        assert!(p.rich);
    }
    #[test]
    fn painter_should_be_plain_on_a_pipe_under_auto() {
        // the bare `ev check | grep` case — today's exact bytes
        let p = Painter::compute(ColorChoice::Auto, false, false, false, false, false);
        assert!(!p.rich);
    }
    #[test]
    fn painter_should_be_plain_when_plain_flag_is_set_even_on_a_tty() {
        let p = Painter::compute(ColorChoice::Auto, true, true, false, false, false);
        assert!(!p.rich);
    }
    #[test]
    fn painter_should_be_plain_when_no_color_is_set_even_on_a_tty() {
        let p = Painter::compute(ColorChoice::Auto, false, true, true, false, false);
        assert!(!p.rich);
    }
    #[test]
    fn painter_should_be_rich_under_always_even_on_a_pipe() {
        // forcing colour for `| less -R`
        let p = Painter::compute(ColorChoice::Always, false, false, false, false, false);
        assert!(p.rich);
    }
    #[test]
    fn painter_should_be_plain_under_never_even_on_a_tty() {
        let p = Painter::compute(ColorChoice::Never, false, true, false, false, false);
        assert!(!p.rich);
    }
    #[test]
    fn truecolor_should_only_arm_when_rich() {
        let off = Painter::compute(ColorChoice::Auto, false, false, false, true, false);
        assert!(!off.truecolor, "truecolor is meaningless when not rich");
        let on = Painter::compute(ColorChoice::Auto, false, true, false, true, false);
        assert!(on.truecolor);
    }
    #[test]
    fn class_of_should_map_gating_verdicts_to_attention() {
        assert_eq!(class_of(&Verdict::Red), Class::Attention);
        assert_eq!(class_of(&Verdict::SilentlyUnbound), Class::Attention);
        assert_eq!(
            class_of(&Verdict::NotRun {
                missing_platforms: vec![]
            }),
            Class::Attention
        );
    }
    #[test]
    fn class_of_should_map_green_to_clear_and_non_gating_to_informational() {
        assert_eq!(class_of(&Verdict::Green), Class::Clear);
        assert_eq!(class_of(&Verdict::Memo), Class::Informational);
        assert_eq!(class_of(&Verdict::Exempt), Class::Informational);
        assert_eq!(class_of(&Verdict::NotApplicable), Class::Informational);
    }
    // Glyph families never collide on one line: verdict ◆◇· vs provenance ●○.
    #[test]
    fn glyph_families_should_be_disjoint() {
        let p = Painter::compute(ColorChoice::Auto, false, true, false, false, false);
        let verdicts = [
            p.verdict_glyph(Class::Attention),
            p.verdict_glyph(Class::Clear),
            p.verdict_glyph(Class::Informational),
        ];
        let provs = [p.prov_glyph(false), p.prov_glyph(true)];
        for v in verdicts {
            assert!(
                !provs.contains(&v),
                "verdict glyph {v} collides with a provenance glyph"
            );
        }
    }
    #[test]
    fn ascii_glyphs_should_be_single_cell_ascii_when_ev_ascii_set() {
        let p = Painter::compute(ColorChoice::Auto, false, true, false, false, true);
        assert_eq!(p.verdict_glyph(Class::Attention), "*");
        assert_eq!(p.prov_glyph(true), "o");
    }
    /// Strip CSI sequences (`ESC [ … m`) so we can assert on the visible text alone — the escape
    /// codes themselves contain digits, so a naive hex filter would read them as id chars.
    fn strip_ansi(s: &str) -> String {
        let mut out = String::new();
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                for d in chars.by_ref() {
                    if d == 'm' {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    #[test]
    fn id_should_keep_all_twelve_hex_chars() {
        let p = Painter::compute(ColorChoice::Auto, false, true, false, false, false);
        let painted = p.id("9053dc3c9ef3");
        // the visible text (ANSI stripped) must be the full id, in order, never truncated
        assert_eq!(
            strip_ansi(&painted),
            "9053dc3c9ef3",
            "the full 12-hex id must never be truncated"
        );
    }
}
