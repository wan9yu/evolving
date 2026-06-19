//! The pure verdict engine: per Test-bound ground, the resurface precedence.
//! No I/O — receipts, the live-origin sha, and the selected-list are passed in. Facts,
//! not verdicts: every not-green state is a co-equal fact, never ranked or scored.
//!
//! Precedence (first match wins): sha-stale → not-run → age-stale → unproven → gray→red →
//! red → silently-unbound → green.
use crate::receipt::Receipt;
use crate::selected::SelectedList;
use crate::tick::{Check, Ground};

#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    Green,
    Red,
    GrayRed,
    Unproven,
    NotRun { missing_platforms: Vec<String> },
    Stale { reason: String },
    SilentlyUnbound,
    Exempt,        // this runner attests none of the binding's declared platforms (non-gating)
    NotApplicable, // no check, or a person re-check
}

impl Verdict {
    /// The flat, human-facing label — facts, not verdicts (no score, no rank).
    pub fn label(&self) -> &'static str {
        match self {
            Verdict::Green => "green",
            Verdict::Red => "red",
            Verdict::GrayRed => "gray->red",
            Verdict::Unproven => "unproven",
            Verdict::NotRun { .. } => "not-run",
            Verdict::Stale { .. } => "stale",
            Verdict::SilentlyUnbound => "silently-unbound",
            Verdict::Exempt => "exempt",
            Verdict::NotApplicable => "n/a",
        }
    }
}

use time::{format_description::well_known::Rfc3339, OffsetDateTime};

/// The evaluation context, built once per `ev check` / `ev reopen` invocation:
/// the staleness reference sha, the selected-list, and the clock for age-staleness.
pub struct Ctx {
    pub live_origin_sha: Option<String>, // None ⇒ sha-staleness not evaluated
    pub selected: Option<SelectedList>,  // None ⇒ L2 not evaluated
    pub now_unix: i64,                   // current time, unix seconds
    pub staleness_secs: i64, // a deciding receipt older than this is stale; i64::MAX disables
    pub attest: Option<Vec<String>>, // platforms this runner speaks for; None ⇒ attest all
}

/// Verdict for one ground against `receipts` (this ground's run-receipts) and `ctx`.
pub fn verdict_for(
    ground: &Ground,
    receipts: &[Receipt],
    ctx: &Ctx,
    triggered_since: bool,
) -> Verdict {
    let (reference, verified_at_sha, liveness) = match &ground.check {
        Some(Check::Test {
            reference,
            verified_at_sha,
            liveness,
            ..
        }) => (reference.as_str(), verified_at_sha.as_str(), liveness),
        _ => return Verdict::NotApplicable,
    };

    if let Some(origin) = ctx.live_origin_sha.as_deref() {
        if origin != verified_at_sha {
            return Verdict::Stale {
                reason: "verified_at_sha behind live origin".into(),
            };
        }
    }

    // Per-runner attestation: only platforms this runner speaks for count toward not-run.
    // None ⇒ attest all (cross-platform audit / default).
    let attested: Vec<&String> = match &ctx.attest {
        Some(set) => liveness
            .platforms
            .iter()
            .filter(|p| set.contains(p))
            .collect(),
        None => liveness.platforms.iter().collect(),
    };
    if ctx.attest.is_some() && attested.is_empty() {
        return Verdict::Exempt; // this runner speaks for none of the declared platforms
    }
    let mut missing = Vec::new();
    let mut deciding: Vec<&Receipt> = Vec::new();
    for p in attested {
        // RFC-3339 UTC timestamps sort chronologically, so the lexicographic max is the latest run.
        let latest = receipts
            .iter()
            .filter(|r| r.test == reference && &r.platform == p)
            .max_by(|a, b| a.ran_at.cmp(&b.ran_at));
        match latest {
            None => missing.push(p.clone()),
            Some(r) => deciding.push(r),
        }
    }
    if !missing.is_empty() {
        return Verdict::NotRun {
            missing_platforms: missing,
        };
    }

    // Event-driven freshness: a commit touching a declared trigger landed after the last run,
    // so the green is for a stale world. A not-green fact (the count-N window is rejected — a
    // refactor moving the assumption out of triggered_by would otherwise stay green forever).
    if triggered_since {
        return Verdict::Stale {
            reason: "a triggering change landed after the last run".into(),
        };
    }

    // Age-staleness: a deciding receipt older than the staleness window is too old to trust.
    // An unparseable ran_at is skipped (a data fault, not a freshness signal).
    let stale_by_age = deciding.iter().any(|r| {
        OffsetDateTime::parse(&r.ran_at, &Rfc3339)
            .map(|dt| ctx.now_unix - dt.unix_timestamp() > ctx.staleness_secs)
            .unwrap_or(false)
    });
    if stale_by_age {
        return Verdict::Stale {
            reason: "deciding receipt older than the staleness window".into(),
        };
    }

    // The check could not be shown to flip (its counter-test failed to produce the opposite) —
    // its green/red reading is untrustworthy, so this overrides them.
    if deciding.iter().any(|r| r.falsifiable == Some(false)) {
        return Verdict::Unproven;
    }

    if deciding.iter().any(|r| r.result == "gray") {
        return Verdict::GrayRed;
    }
    if deciding.iter().any(|r| r.result == "red") {
        return Verdict::Red;
    }

    // L2 selected: the latest diff touched a declared trigger but did not select this ref —
    // the receipts look green but were not re-run for the change that touched the assumption.
    if let Some(sl) = ctx.selected.as_ref() {
        let touched = liveness
            .triggered_by
            .iter()
            .any(|t| sl.changed.iter().any(|c| c == t));
        let was_selected = sl.selected.iter().any(|s| s == reference);
        if touched && !was_selected {
            return Verdict::SilentlyUnbound;
        }
    }

    Verdict::Green
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::receipt::Receipt;
    use crate::tick::{Check, Ground, Liveness};

    fn test_ground(platforms: &[&str]) -> Ground {
        Ground {
            claim: "no Redis".into(),
            supports: "chosen".into(),
            check: Some(Check::Test {
                reference: "pytest x".into(),
                verified_at_sha: "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901".into(),
                counter_test: "pytest x::flips".into(),
                liveness: Liveness {
                    platforms: platforms.iter().map(|s| s.to_string()).collect(),
                    triggered_by: vec!["pyproject.toml".into()],
                    surfaces: vec!["pyproject-deps".into()],
                },
            }),
        }
    }
    fn rcpt(platform: &str, ran_at: &str, result: &str) -> Receipt {
        Receipt {
            test: "pytest x".into(),
            platform: platform.into(),
            commit: "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901".into(),
            ran_at: ran_at.into(),
            result: result.into(),
            falsifiable: None,
        }
    }
    // A Ctx with age-staleness DISABLED (staleness_secs = i64::MAX), for the non-age tests.
    fn ctx(live_origin_sha: Option<&str>, selected: Option<SelectedList>) -> Ctx {
        Ctx {
            live_origin_sha: live_origin_sha.map(|s| s.to_string()),
            selected,
            now_unix: 0,
            staleness_secs: i64::MAX,
            attest: None,
        }
    }
    fn ctx_attest(attest: Option<&[&str]>) -> Ctx {
        Ctx {
            live_origin_sha: None,
            selected: None,
            now_unix: 0,
            staleness_secs: i64::MAX,
            attest: attest.map(|a| a.iter().map(|s| s.to_string()).collect()),
        }
    }

    #[test]
    fn verdict_for_should_be_not_applicable_when_the_ground_has_a_person_check() {
        // given: a person-rechecked ground
        let g = Ground {
            claim: "c".into(),
            supports: "chosen".into(),
            check: Some(Check::Person {
                reference: "Q3".into(),
            }),
        };

        // when: its verdict is computed
        let v = verdict_for(&g, &[], &ctx(None, None), false);

        // then: it is not applicable (person grounds never appear in check)
        assert_eq!(v, Verdict::NotApplicable);
    }

    #[test]
    fn verdict_for_should_be_not_run_when_a_declared_platform_has_no_receipt() {
        // given: a binding on two platforms with a receipt for only one
        let g = test_ground(&["linux-ci", "mac"]);
        let receipts = vec![rcpt("linux-ci", "2026-01-01T00:00:00Z", "green")];

        // when: its verdict is computed
        let v = verdict_for(&g, &receipts, &ctx(None, None), false);

        // then: it is not-run, naming the missing platform
        assert_eq!(
            v,
            Verdict::NotRun {
                missing_platforms: vec!["mac".into()]
            }
        );
    }

    #[test]
    fn verdict_for_should_promote_gray_to_red_when_the_deciding_receipt_is_gray() {
        // given: a single-platform binding whose latest receipt is gray
        let g = test_ground(&["linux-ci"]);
        let receipts = vec![rcpt("linux-ci", "2026-01-01T00:00:00Z", "gray")];

        // when: its verdict is computed
        let v = verdict_for(&g, &receipts, &ctx(None, None), false);

        // then: gray is promoted to red, never dropped
        assert_eq!(v, Verdict::GrayRed);
    }

    #[test]
    fn verdict_for_should_be_red_when_the_latest_receipt_is_red() {
        // given: a binding whose later receipt (by ran_at) is red, an earlier one green
        let g = test_ground(&["linux-ci"]);
        let receipts = vec![
            rcpt("linux-ci", "2026-01-01T00:00:00Z", "green"),
            rcpt("linux-ci", "2026-02-01T00:00:00Z", "red"),
        ];

        // when: its verdict is computed
        let v = verdict_for(&g, &receipts, &ctx(None, None), false);

        // then: the latest (red) decides
        assert_eq!(v, Verdict::Red);
    }

    #[test]
    fn verdict_for_should_be_green_when_every_platform_has_a_fresh_green_receipt() {
        // given: a two-platform binding green on both, no stale reference
        let g = test_ground(&["linux-ci", "mac"]);
        let receipts = vec![
            rcpt("linux-ci", "2026-01-01T00:00:00Z", "green"),
            rcpt("mac", "2026-01-01T00:00:00Z", "green"),
        ];

        // when: its verdict is computed
        let v = verdict_for(&g, &receipts, &ctx(None, None), false);

        // then: it is green
        assert_eq!(v, Verdict::Green);
    }

    #[test]
    fn verdict_for_should_be_stale_when_verified_at_sha_is_behind_the_live_origin() {
        // given: a green binding whose verified_at_sha differs from the live-origin sha
        let g = test_ground(&["linux-ci"]);
        let receipts = vec![rcpt("linux-ci", "2026-01-01T00:00:00Z", "green")];
        let origin = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

        // when: its verdict is computed against that origin
        let v = verdict_for(&g, &receipts, &ctx(Some(origin), None), false);

        // then: it is stale (binary, no grace) — never shown green
        assert!(matches!(v, Verdict::Stale { .. }));
    }

    #[test]
    fn verdict_for_should_be_silently_unbound_when_a_touched_trigger_was_not_selected() {
        // given: a green-otherwise binding whose declared trigger the diff changed but did not select
        let g = test_ground(&["linux-ci"]);
        let receipts = vec![rcpt("linux-ci", "2026-01-01T00:00:00Z", "green")];
        let sl = crate::selected::SelectedList {
            commit: "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901".into(),
            changed: vec!["pyproject.toml".into()],
            selected: vec![],
        };

        // when: its verdict is computed against that selected-list
        let v = verdict_for(&g, &receipts, &ctx(None, Some(sl)), false);

        // then: it is silently-unbound (never counted green)
        assert_eq!(v, Verdict::SilentlyUnbound);
    }

    #[test]
    fn verdict_for_should_be_green_when_the_touched_trigger_was_selected() {
        // given: the same binding, but the diff did select its ref
        let g = test_ground(&["linux-ci"]);
        let receipts = vec![rcpt("linux-ci", "2026-01-01T00:00:00Z", "green")];
        let sl = crate::selected::SelectedList {
            commit: "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901".into(),
            changed: vec!["pyproject.toml".into()],
            selected: vec!["pytest x".into()],
        };

        // when: its verdict is computed
        let v = verdict_for(&g, &receipts, &ctx(None, Some(sl)), false);

        // then: it is green (selected, so not silently-unbound)
        assert_eq!(v, Verdict::Green);
    }

    #[test]
    fn verdict_for_should_be_stale_when_the_deciding_receipt_is_older_than_the_window() {
        // given: a green receipt from 2026-01-01 evaluated ~5 months later, 7-day window
        let g = test_ground(&["linux-ci"]);
        let receipts = vec![rcpt("linux-ci", "2026-01-01T00:00:00Z", "green")];
        let c = Ctx {
            live_origin_sha: None,
            selected: None,
            now_unix: OffsetDateTime::parse("2026-06-01T00:00:00Z", &Rfc3339)
                .unwrap()
                .unix_timestamp(),
            staleness_secs: 7 * 86_400,
            attest: None,
        };

        // when: its verdict is computed against that clock
        let v = verdict_for(&g, &receipts, &c, false);

        // then: it is stale (too old to trust), never green
        assert!(matches!(v, Verdict::Stale { .. }));
    }

    #[test]
    fn verdict_for_should_be_green_when_the_deciding_receipt_is_within_the_window() {
        // given: a green receipt one hour before now, 7-day window
        let g = test_ground(&["linux-ci"]);
        let receipts = vec![rcpt("linux-ci", "2026-06-01T00:00:00Z", "green")];
        let c = Ctx {
            live_origin_sha: None,
            selected: None,
            now_unix: OffsetDateTime::parse("2026-06-01T01:00:00Z", &Rfc3339)
                .unwrap()
                .unix_timestamp(),
            staleness_secs: 7 * 86_400,
            attest: None,
        };

        // when: its verdict is computed
        let v = verdict_for(&g, &receipts, &c, false);

        // then: it is green (fresh)
        assert_eq!(v, Verdict::Green);
    }

    #[test]
    fn verdict_for_should_be_stale_when_a_triggering_change_landed_after_the_last_run() {
        // given: a green binding whose deciding receipt is behind a triggering change
        let g = test_ground(&["linux-ci"]);
        let receipts = vec![rcpt("linux-ci", "2026-01-01T00:00:00Z", "green")];

        // when: its verdict is computed with triggered_since = true
        let v = verdict_for(&g, &receipts, &ctx(None, None), true);

        // then: it is stale — the green is for a stale world, never shown green
        assert!(matches!(v, Verdict::Stale { .. }));
    }

    #[test]
    fn verdict_for_should_ignore_triggered_since_when_a_platform_is_already_not_run() {
        // given: a two-platform binding missing a receipt on one platform, and a triggering change
        let g = test_ground(&["linux-ci", "mac"]);
        let receipts = vec![rcpt("linux-ci", "2026-01-01T00:00:00Z", "green")];

        // when: its verdict is computed with triggered_since = true
        let v = verdict_for(&g, &receipts, &ctx(None, None), true);

        // then: absence-not-run still wins (precedence: not-run before triggering-stale)
        assert_eq!(
            v,
            Verdict::NotRun {
                missing_platforms: vec!["mac".into()]
            }
        );
    }

    #[test]
    fn verdict_for_should_exempt_a_binding_whose_platforms_this_runner_does_not_attest() {
        // given: a mac-only binding, a runner attesting only linux-ci, no receipts
        let g = test_ground(&["mac"]);
        // then: exempt (a non-gating fact), NOT not-run
        assert_eq!(
            verdict_for(&g, &[], &ctx_attest(Some(&["linux-ci"])), false),
            Verdict::Exempt
        );
    }

    #[test]
    fn verdict_for_should_ignore_an_unattested_platform_when_an_attested_one_is_green() {
        // given: a [linux-ci, mac] binding, runner attests linux-ci, a green linux-ci receipt
        let g = test_ground(&["linux-ci", "mac"]);
        let receipts = vec![rcpt("linux-ci", "2026-01-01T00:00:00Z", "green")];
        // then: green — mac is exempt, only the attested linux-ci needs a receipt
        assert_eq!(
            verdict_for(&g, &receipts, &ctx_attest(Some(&["linux-ci"])), false),
            Verdict::Green
        );
    }

    #[test]
    fn verdict_for_should_be_unproven_when_a_deciding_receipt_is_not_falsifiable() {
        // given: a single-platform binding whose green receipt failed its falsifiability proof
        let g = test_ground(&["linux-ci"]);
        let receipts = vec![Receipt {
            test: "pytest x".into(),
            platform: "linux-ci".into(),
            commit: "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901".into(),
            ran_at: "2026-01-01T00:00:00Z".into(),
            result: "green".into(),
            falsifiable: Some(false),
        }];
        // when: its verdict is computed
        let v = verdict_for(&g, &receipts, &ctx(None, None), false);
        // then: it is unproven (the check can't be shown to flip), never shown green
        assert_eq!(v, Verdict::Unproven);
    }

    #[test]
    fn verdict_for_should_be_green_when_a_deciding_receipt_is_proven_falsifiable() {
        // given: a green receipt that passed its falsifiability proof
        let g = test_ground(&["linux-ci"]);
        let receipts = vec![Receipt {
            test: "pytest x".into(),
            platform: "linux-ci".into(),
            commit: "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901".into(),
            ran_at: "2026-01-01T00:00:00Z".into(),
            result: "green".into(),
            falsifiable: Some(true),
        }];
        // when: its verdict is computed
        let v = verdict_for(&g, &receipts, &ctx(None, None), false);
        // then: it is green (proven + passing)
        assert_eq!(v, Verdict::Green);
    }
}
