//! The pure verdict engine: per Test-bound ground, the resurface precedence.
//! No I/O — receipts + the live-origin sha are passed in. Facts, not verdicts:
//! every not-green state is a co-equal fact, never ranked or scored.
//!
//! Stage 3a precedence (first match wins): sha-stale → not-run → gray→red → red → green.
//! (Age-staleness and L2 silently-unbound arrive in Stage 3b.)
use crate::receipt::Receipt;
use crate::tick::{Check, Ground};

#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    Green,
    Red,
    GrayRed,
    NotRun { missing_platforms: Vec<String> },
    Stale { reason: String },
    SilentlyUnbound,
    NotApplicable, // no check, or a person re-check
}

/// Verdict for one ground. `receipts` are the run-receipts for this ground's bound ref
/// (via receipt::read_for); `live_origin_sha` is the staleness reference — None means
/// sha-staleness is not evaluated.
pub fn verdict_for(
    ground: &Ground,
    receipts: &[Receipt],
    live_origin_sha: Option<&str>,
) -> Verdict {
    let (reference, verified_at_sha, platforms) = match &ground.check {
        Some(Check::Test {
            reference,
            verified_at_sha,
            liveness,
            ..
        }) => (
            reference.as_str(),
            verified_at_sha.as_str(),
            &liveness.platforms,
        ),
        _ => return Verdict::NotApplicable,
    };

    if let Some(origin) = live_origin_sha {
        if origin != verified_at_sha {
            return Verdict::Stale {
                reason: "verified_at_sha behind live origin".into(),
            };
        }
    }

    let mut missing = Vec::new();
    let mut deciding: Vec<&Receipt> = Vec::new();
    for p in platforms {
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
    if deciding.iter().any(|r| r.result == "gray") {
        return Verdict::GrayRed;
    }
    if deciding.iter().any(|r| r.result == "red") {
        return Verdict::Red;
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
        let v = verdict_for(&g, &[], None);

        // then: it is not applicable (person grounds never appear in check)
        assert_eq!(v, Verdict::NotApplicable);
    }

    #[test]
    fn verdict_for_should_be_not_run_when_a_declared_platform_has_no_receipt() {
        // given: a binding on two platforms with a receipt for only one
        let g = test_ground(&["linux-ci", "mac"]);
        let receipts = vec![rcpt("linux-ci", "2026-01-01T00:00:00Z", "green")];

        // when: its verdict is computed
        let v = verdict_for(&g, &receipts, None);

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
        let v = verdict_for(&g, &receipts, None);

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
        let v = verdict_for(&g, &receipts, None);

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
        let v = verdict_for(&g, &receipts, None);

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
        let v = verdict_for(&g, &receipts, Some(origin));

        // then: it is stale (binary, no grace) — never shown green
        assert!(matches!(v, Verdict::Stale { .. }));
    }
}
