//! `ev decide` — walk the trailing args left-to-right into a draft, validate, append a child.
use crate::canonical::compute_id;
use crate::store::Store;
use crate::tick::{Check, Ground, Tick};
use std::path::Path;
use std::process::Command;

#[derive(Default)]
struct DraftGround {
    claim: String,
    supports: String, // "chosen" | "rejected:<opt>"
    revisit: Option<String>,
    test_ref: Option<String>,
    counter_test: Option<String>,
    platforms: Vec<String>,
    triggered_by: Vec<String>,
    surfaces: Vec<String>,
}

fn need(args: &[String], i: usize, flag: &str) -> Result<String, String> {
    args.get(i + 1)
        .cloned()
        .ok_or(format!("{flag} requires a value"))
}

fn last<'a>(g: &'a mut [DraftGround], flag: &str) -> Result<&'a mut DraftGround, String> {
    g.last_mut()
        .ok_or(format!("{flag} has no preceding --assume/--reject ground"))
}

/// Resolve the declared author: --blame, else `git config user.name`.
pub(crate) fn resolve_blame(repo: &Path, blame_override: Option<String>) -> Result<String, String> {
    if let Some(b) = blame_override {
        let b = b.trim();
        if b.is_empty() {
            return Err("--blame must be non-empty".into());
        }
        return Ok(b.to_string());
    }
    let out = Command::new("git")
        .arg("config")
        .arg("user.name")
        .current_dir(repo)
        .output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if name.is_empty() {
        return Err("no author: pass --blame, or set git config user.name".into());
    }
    Ok(name)
}

pub(crate) fn resolve_sha(repo: &Path, sha_override: &Option<String>) -> Result<String, String> {
    let sha = match sha_override {
        Some(s) => s.trim().to_string(),
        None => {
            let out = std::process::Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(repo)
                .output()
                .map_err(|e| format!("cannot run git: {e}"))?;
            if !out.status.success() {
                return Err(
                    "cannot resolve verified_at_sha (not a git repo?) — pass --verified-at-sha"
                        .into(),
                );
            }
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        }
    };
    if !crate::tick::is_40_lower_hex(&sha) {
        return Err(format!("verified_at_sha must be 40 lowercase hex: {sha}"));
    }
    Ok(sha)
}

fn t_grounds_text(grounds: &[Ground]) -> Vec<String> {
    grounds.iter().map(|g| g.claim.clone()).collect()
}

/// One `git show -s --format=<fmt> <commit>` field, run in `repo`. Returns the trimmed
/// stdout, or an error if git can't resolve the commit (the caller maps this to a clear message).
fn git_show(repo: &Path, fmt: &str, commit: &str) -> Result<String, String> {
    let out = Command::new("git")
        .args(["show", "-s", fmt, commit])
        .current_dir(repo)
        .output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    if !out.status.success() {
        return Err(format!("decide: cannot read commit {commit}"));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The commit ENVELOPE we are allowed to seed from: subject (the decision text), author name
/// (the default blame), and any `Refs #<n>` provenance lines from the body. The body is scanned
/// ONLY for Refs lines — never parsed for grounds (those stay human-authored via --assume/--reject).
struct Envelope {
    subject: String,
    author: String,
    refs: Vec<String>,
}

/// The closed set of authoring roles a commit subject may declare, leading + `:`.
const SUBJECT_ROLES: &[&str] = &["Dev", "QA", "Product", "Mac", "User"];

/// The canonical role declared by a leading `<Role>:` prefix on the subject, if any
/// (case-insensitive match against the closed vocabulary). The subject is otherwise untouched.
fn subject_role(subject: &str) -> Option<&'static str> {
    let head = subject.split_whitespace().next()?;
    let word = head.strip_suffix(':')?;
    SUBJECT_ROLES
        .iter()
        .find(|r| r.eq_ignore_ascii_case(word))
        .copied()
}

/// Every `#<digits>` / `R<digits>` provenance token found in the subject, in order — the
/// issue + round-id references a commit subject may carry (`re-milestone #1194 R2415`).
fn subject_refs(subject: &str) -> Vec<String> {
    subject
        .split_whitespace()
        .filter(|tok| {
            let rest = tok
                .strip_prefix('#')
                .or_else(|| tok.strip_prefix('R'))
                .or_else(|| tok.strip_prefix('r'));
            matches!(rest, Some(d) if !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
        })
        .map(|t| t.to_string())
        .collect()
}

fn read_envelope(repo: &Path, commit: &str) -> Result<Envelope, String> {
    let subject = git_show(repo, "--format=%s", commit)?;
    let author = git_show(repo, "--format=%an", commit)?;
    let body = git_show(repo, "--format=%b", commit)?;
    let refs = body
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("Refs #"))
        .map(|l| l.to_string())
        .collect();
    Ok(Envelope {
        subject,
        author,
        refs,
    })
}

/// Validate a declared authority value against the closed vocabulary.
pub(crate) fn validate_authority(val: &str) -> Result<(), String> {
    if val == "user-ruled" || val == "agent-disposable" {
        Ok(())
    } else {
        Err("authority must be user-ruled or agent-disposable".into())
    }
}

/// The migrate-only harvested-binding constructor: build a `Check::Test` carrying NO counter-test
/// (`counter_test: None`), as used when backfilling an existing `test_invariant_*`/`test_br_*` test
/// whose falsifiability was never proven. You cannot half-harvest: the FULL 3-key liveness
/// (≥1 platform, triggered-by, surface) stays MANDATORY — only the counter-test is dropped. There is
/// no `--counter-test` flag on this path; the decide (capture.rs) and `ev guard` (guard.rs) paths
/// stay byte-for-byte strict and still reject a vacuous binding. The honesty debt (the missing
/// falsifiability proof) is surfaced later at `ev check`, never hidden.
pub fn harvested_test_check(
    reference: String,
    verified_at_sha: String,
    platforms: Vec<String>,
    triggered_by: Vec<String>,
    surfaces: Vec<String>,
) -> Result<Check, String> {
    use crate::tick::Liveness;
    if reference.trim().is_empty() {
        return Err("a harvested binding requires a non-empty test reference".into());
    }
    if !crate::tick::is_40_lower_hex(&verified_at_sha) {
        return Err(format!(
            "verified_at_sha must be 40 lowercase hex: {verified_at_sha}"
        ));
    }
    if platforms.is_empty() || triggered_by.is_empty() || surfaces.is_empty() {
        return Err(
            "a harvested binding requires at least one platform, triggered-by, and surface (no half-harvest)"
                .into(),
        );
    }
    Ok(Check::Test {
        reference,
        verified_at_sha,
        counter_test: None, // harvested: falsifiability not yet proven
        liveness: Liveness {
            platforms,
            triggered_by,
            surfaces,
        },
    })
}

/// An assembled, validated decision ready to be appended to the ledger — the single shape both
/// `ev decide` (capture.rs) and `ev migrate` (migrate.rs) hand to `append`. It carries exactly the
/// hashed payload (observe, decision, grounds) plus the bookkeeping fields; `append` owns the one
/// compute_id / write_tick / R3-lint path so neither caller can fork the hashing.
pub struct Decision {
    pub observe: String,
    pub decision: String,
    pub grounds: Vec<Ground>,
    pub blame: String,
    pub authority: Option<String>,
    pub jurisdiction: Option<String>,
    pub source_ref: Option<serde_json::Value>,
    pub provenance: Option<String>,
}

/// THE one place a decision becomes a tick: R3-lint the free text, read HEAD as the parent, stamp
/// `held_since`, build the Tick, compute its content-addressed id, and write+advance HEAD. `ev decide`
/// and `ev migrate` BOTH funnel through here so there is a single hashing path — a golden id can only
/// move if this function moves (guarded by golden_vectors + the capture/migrate tests). The caller is
/// responsible for having already resolved blame and validated the grounds.
pub fn append(repo: &Path, d: Decision) -> Result<Tick, String> {
    for field in std::iter::once(d.decision.clone())
        .chain(std::iter::once(d.observe.clone()))
        .chain(t_grounds_text(&d.grounds))
    {
        for verb in crate::lint::r3_self_evolve(&field) {
            eprintln!("warning: \"{verb}\" should take a human subject, not the system (best-effort lint; a re-wording evades it)");
        }
    }
    let store = Store::at(repo);
    if !store.exists() {
        return Err("no .evolving/ store here — run `ev init` first".into());
    }
    let parent_id = store
        .read_head()
        .map_err(|e| format!("reading HEAD: {e}"))?;
    let held_since = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|e| format!("timestamp: {e}"))?;
    let mut t = Tick {
        id: String::new(),
        parent_id,
        observe: d.observe,
        decision: d.decision,
        grounds: d.grounds,
        status: "live".into(),
        held_since,
        blame: d.blame,
        authority: d.authority,
        jurisdiction: d.jurisdiction,
        source_ref: d.source_ref,
        provenance: d.provenance,
    };
    t.id = compute_id(&t);
    store
        .write_tick(&t)
        .map_err(|e| format!("writing tick: {e}"))?;
    Ok(t)
}

fn build_ground(
    repo: &Path,
    d: DraftGround,
    sha_override: &Option<String>,
    authority: Option<&str>,
) -> Result<Ground, String> {
    use crate::tick::Liveness;
    if d.claim.is_empty() {
        return Err("ground claim is empty".into());
    }
    // A road-not-taken is closed: it never carries a human re-check (you do not schedule someone to
    // re-confirm a non-choice). This stays a hard refusal regardless of authority.
    if d.supports.starts_with("rejected:") && d.revisit.is_some() {
        return Err("a road-not-taken (rejected) ground cannot carry a human re-check".into());
    }
    // 0.1.8 tripwire: a rejected road MAY carry a falsifiable test that trips when the closed road is
    // re-walked — but ONLY when a human deliberately ruled the road closed (--authority user-ruled).
    // The counter-test stays REQUIRED via the shared strict path below (no harvested rejected-road
    // tripwire). HONESTY: this binds only a STRUCTURAL token (the test/counter-test grep a real
    // artifact); a PROSE re-walk with no token (e.g. #1194's milestone re-assignment) has nothing to
    // bind and STAYS surface-only — the tripwire does not and cannot catch it. An agent cannot author
    // a gating tripwire: --authority is declared, not verified (the banked signing boundary), and the
    // gate-path LOCK 3 excludes agent-proposed from gating.
    if d.supports.starts_with("rejected:")
        && d.test_ref.is_some()
        && authority != Some("user-ruled")
    {
        return Err(
            "a rejected road can carry a tripwire test only when the decision is --authority user-ruled"
                .into(),
        );
    }
    if d.revisit.is_some() && d.test_ref.is_some() {
        return Err("a ground cannot be both --revisit and --assume-test (R2)".into());
    }
    let has_test_fields = d.counter_test.is_some()
        || !d.platforms.is_empty()
        || !d.triggered_by.is_empty()
        || !d.surfaces.is_empty();
    let check = match (d.test_ref, d.revisit) {
        (Some(reference), _) => {
            let counter_test = d
                .counter_test
                .ok_or("a test binding requires --counter-test (no vacuous binding)".to_string())?;
            if counter_test.trim().is_empty() {
                // write/read symmetry: from_value rejects an empty counter_test, so the decide
                // write path must too — never persist a tick its own parser would refuse.
                return Err("a test binding requires --counter-test (no vacuous binding)".into());
            }
            if d.platforms.is_empty() || d.triggered_by.is_empty() || d.surfaces.is_empty() {
                return Err("a test binding requires at least one --on-platform, --triggered-by, and --surface".into());
            }
            let verified_at_sha = resolve_sha(repo, sha_override)?;
            Some(Check::Test {
                reference,
                verified_at_sha,
                counter_test: Some(counter_test),
                liveness: Liveness {
                    platforms: d.platforms,
                    triggered_by: d.triggered_by,
                    surfaces: d.surfaces,
                },
            })
        }
        (None, Some(when)) => {
            if has_test_fields {
                return Err(
                    "--counter-test/--on-platform/--triggered-by/--surface require --assume-test"
                        .into(),
                );
            }
            Some(Check::Person { reference: when })
        }
        (None, None) => {
            if has_test_fields {
                return Err(
                    "--counter-test/--on-platform/--triggered-by/--surface require --assume-test"
                        .into(),
                );
            }
            None
        }
    };
    Ok(Ground {
        claim: d.claim,
        supports: d.supports,
        check,
    })
}

pub fn run(repo: &Path, decision: Option<&str>, args: &[String]) -> Result<Tick, String> {
    let mut observe = String::new();
    let mut blame_override: Option<String> = None;
    let mut sha_override: Option<String> = None;
    let mut authority: Option<String> = None;
    let mut jurisdiction: Option<String> = None;
    let mut source_ref: Option<serde_json::Value> = None;
    let mut from_git: Option<String> = None;
    let mut drafts: Vec<DraftGround> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let flag = args[i].clone();
        match flag.as_str() {
            "--from-git" => {
                from_git = Some(need(args, i, &flag)?);
            }
            "--observe" => {
                observe = need(args, i, &flag)?;
            }
            "--blame" => {
                blame_override = Some(need(args, i, &flag)?);
            }
            "--verified-at-sha" => {
                sha_override = Some(need(args, i, &flag)?);
            }
            "--authority" => {
                let v = need(args, i, &flag)?;
                validate_authority(&v)?;
                authority = Some(v);
            }
            "--jurisdiction" => {
                let v = need(args, i, &flag)?;
                crate::tick::validate_jurisdiction(&v)?;
                jurisdiction = Some(v);
            }
            "--source-ref" => {
                // a durable, non-hashed, opaque source identity ev never interprets. On the interactive
                // path it is a plain string; the canonical intake additionally accepts a structured object.
                let v = need(args, i, &flag)?;
                if v.is_empty() {
                    return Err("--source-ref needs a non-empty value".into());
                }
                source_ref = Some(serde_json::Value::String(v));
            }
            "--reject" => {
                let v = need(args, i, &flag)?;
                let (opt, why) = v
                    .split_once(':')
                    .ok_or("--reject expects \"<option>: <why>\"".to_string())?;
                let (opt, why) = (opt.trim(), why.trim());
                if opt.is_empty() || why.is_empty() {
                    return Err("--reject needs non-empty <option> and <why>".into());
                }
                drafts.push(DraftGround {
                    claim: why.into(),
                    supports: format!("rejected:{opt}"),
                    ..Default::default()
                });
            }
            "--assume" => {
                let claim = need(args, i, &flag)?;
                drafts.push(DraftGround {
                    claim,
                    supports: "chosen".into(),
                    ..Default::default()
                });
            }
            "--revisit" => {
                last(&mut drafts, &flag)?.revisit = Some(need(args, i, &flag)?);
            }
            "--assume-test" => {
                last(&mut drafts, &flag)?.test_ref = Some(need(args, i, &flag)?);
            }
            "--counter-test" => {
                last(&mut drafts, &flag)?.counter_test = Some(need(args, i, &flag)?);
            }
            "--on-platform" => {
                let v = need(args, i, &flag)?;
                last(&mut drafts, &flag)?.platforms.push(v);
            }
            "--triggered-by" => {
                let v = need(args, i, &flag)?;
                last(&mut drafts, &flag)?.triggered_by.push(v);
            }
            "--surface" => {
                let v = need(args, i, &flag)?;
                last(&mut drafts, &flag)?.surfaces.push(v);
            }
            other => return Err(format!("decide: unknown flag {other}")),
        }
        i += 2;
    }

    // Decision source: exactly one of {a positional decision, --from-git}. When --from-git is
    // used, the decision text is the commit subject, the default blame is the commit author, and
    // any `Refs #<n>` body lines are appended to observe as provenance (grounds stay human-authored).
    let (decision, observe) = match (decision, &from_git) {
        (Some(_), Some(_)) => {
            return Err("decide: decision given twice (positional and --from-git)".into())
        }
        (None, None) => return Err("decide: needs a decision (positional) or --from-git".into()),
        (Some(d), None) => (d.to_string(), observe),
        (None, Some(commit)) => {
            let env = read_envelope(repo, commit)?;
            // A leading `<Role>:` on the subject declares the author (unless --blame overrides);
            // otherwise the default blame is the commit author. The subject is left untouched.
            if blame_override.is_none() {
                blame_override = Some(match subject_role(&env.subject) {
                    Some(role) => role.to_string(),
                    None => env.author,
                });
            }
            // Provenance from the subject's own #issue / R<round> tokens, plus body Refs lines.
            let observe = std::iter::once(observe)
                .chain(subject_refs(&env.subject))
                .chain(env.refs)
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            (env.subject, observe)
        }
    };
    if decision.trim().is_empty() {
        return Err("decision text is empty".into());
    }
    let blame = resolve_blame(repo, blame_override)?;
    let mut grounds = Vec::new();
    for d in drafts {
        // authority (parsed + validated above) is decision-global; the rejected-road tripwire lift
        // gates on it, so thread it in. It is non-hashed, so gating on it never moves a tick id.
        grounds.push(build_ground(repo, d, &sha_override, authority.as_deref())?);
    }
    // The single hashing path: decide hands its assembled Decision to the shared append, exactly as
    // migrate does, so there is one compute_id / write_tick / R3-lint site (no per-caller fork).
    append(
        repo,
        Decision {
            observe,
            decision: decision.to_string(),
            grounds,
            blame,
            authority,
            jurisdiction,
            source_ref,
            // Fresh authorship is hard-stamped human-now (the absent default); decide takes no
            // provenance from the caller, so an importer can never launder a forbidden op as imported.
            provenance: None,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tick::Check;

    fn repo() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "ev-capture-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Store::at(&p).init().unwrap();
        p
    }
    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn decide_should_record_a_chosen_a_revisit_and_a_rejected_road_when_all_are_passed() {
        // given: a store and decide args with a chosen+revisit ground and a rejected road
        let r = repo();

        // when: the decision is captured
        let t = run(
            &r,
            Some("build our own retrieval; reject pgvector"),
            &s(&[
                "--observe",
                "evaluating backend",
                "--assume",
                "team has bandwidth long-term",
                "--revisit",
                "Q3 review",
                "--reject",
                "pgvector: would lock our schema",
                "--blame",
                "Wang Yu",
            ]),
        )
        .expect("ok");

        // then: both grounds, the person check, the rejected support, blame, and HEAD all hold
        assert_eq!(t.grounds.len(), 2);
        assert!(matches!(t.grounds[0].check, Some(Check::Person { .. })));
        assert_eq!(t.grounds[1].supports, "rejected:pgvector");
        assert_eq!(t.blame, "Wang Yu");
        assert_eq!(Store::at(&r).read_head().unwrap(), t.id);
    }

    #[test]
    fn decide_should_stamp_held_since_with_a_nonempty_rfc3339_time_when_recording() {
        // given: a store
        let r = repo();

        // when: run records a decision
        run(&r, Some("ship it"), &s(&["--blame", "Wang Yu"])).expect("ok");

        // then: the stored HEAD tick's held_since is non-empty and parses as RFC 3339
        let head = Store::at(&r).read_head().unwrap();
        let tick = Store::at(&r).read_tick(&head).unwrap().unwrap();
        assert!(!tick.held_since.is_empty());
        time::OffsetDateTime::parse(
            &tick.held_since,
            &time::format_description::well_known::Rfc3339,
        )
        .expect("held_since parses as RFC 3339");
    }

    #[test]
    fn decide_should_store_a_trimmed_blame_when_the_blame_is_padded() {
        // given: a store and decide args with a padded --blame
        let r = repo();

        // when: the decision is captured
        let t = run(
            &r,
            Some("d"),
            &s(&["--assume", "c", "--blame", "  Wang Yu  "]),
        )
        .expect("ok");

        // then: the stored blame is trimmed
        assert_eq!(t.blame, "Wang Yu");
    }

    #[test]
    fn decide_should_refuse_the_ground_when_it_is_both_revisit_and_assume_test() {
        // given: a store and decide args binding one ground to both --revisit and --assume-test
        let r = repo();

        // when: the decision is captured
        let e = run(
            &r,
            Some("d"),
            &s(&[
                "--assume",
                "c",
                "--revisit",
                "Q3",
                "--assume-test",
                "pytest x",
                "--blame",
                "Wang Yu",
            ]),
        );

        // then: it is refused
        assert!(e.is_err());
    }

    #[test]
    fn decide_should_refuse_a_tripwire_on_a_rejected_road_when_authority_is_absent() {
        // given: a store and decide args attaching a test tripwire to a --reject road, NO --authority
        let r = repo();

        // when: the decision is captured
        let e = run(
            &r,
            Some("d"),
            &s(&[
                "--reject",
                "pgvector: would lock our schema",
                "--assume-test",
                "pytest x",
                "--counter-test",
                "ct",
                "--on-platform",
                "linux-ci",
                "--triggered-by",
                "f",
                "--surface",
                "s",
                "--verified-at-sha",
                "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901",
                "--blame",
                "Wang Yu",
            ]),
        );

        // then: it is refused — a rejected-road tripwire is allowed only when --authority user-ruled
        assert!(e.is_err());
    }

    #[test]
    fn decide_should_refuse_a_tripwire_on_a_rejected_road_when_authority_is_agent_disposable() {
        // given: the same tripwire but declared agent-disposable (not a human's closed-road ruling)
        let r = repo();

        // when: the decision is captured
        let e = run(
            &r,
            Some("d"),
            &s(&[
                "--reject",
                "pgvector: would lock our schema",
                "--assume-test",
                "pytest x",
                "--counter-test",
                "ct",
                "--on-platform",
                "linux-ci",
                "--triggered-by",
                "f",
                "--surface",
                "s",
                "--verified-at-sha",
                "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901",
                "--authority",
                "agent-disposable",
                "--blame",
                "Wang Yu",
            ]),
        );

        // then: refused — only a user-ruled closed road earns a gating tripwire
        assert!(e.is_err());
    }

    #[test]
    fn decide_should_accept_a_tripwire_on_a_rejected_road_when_authority_is_user_ruled() {
        // given: a user-ruled decision binding a falsifiable tripwire to the road it closed
        let r = repo();

        // when: captured with --authority user-ruled + a full test binding on the --reject road
        let t = run(
            &r,
            Some("keep Redis out"),
            &s(&[
                "--reject",
                "Redis: a new infra dependency",
                "--assume-test",
                "! grep -q redis pyproject.toml",
                "--counter-test",
                "grep -q redis pyproject.toml",
                "--on-platform",
                "linux-ci",
                "--triggered-by",
                "pyproject.toml",
                "--surface",
                "pyproject-deps",
                "--verified-at-sha",
                "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901",
                "--authority",
                "user-ruled",
                "--blame",
                "Wang Yu",
            ]),
        )
        .expect("a user-ruled rejected-road tripwire is allowed");

        // then: the closed road carries the test tripwire
        let g = t
            .grounds
            .iter()
            .find(|g| g.supports.starts_with("rejected:"))
            .expect("a rejected road");
        assert!(
            matches!(g.check, Some(Check::Test { .. })),
            "the closed road carries a tripwire"
        );
    }

    #[test]
    fn decide_should_refuse_a_user_ruled_rejected_road_tripwire_when_the_counter_test_is_missing() {
        // given: a user-ruled rejected-road tripwire with NO --counter-test (no falsifiability proof)
        let r = repo();

        // when: the decision is captured
        let e = run(
            &r,
            Some("keep Redis out"),
            &s(&[
                "--reject",
                "Redis: a new infra dependency",
                "--assume-test",
                "! grep -q redis pyproject.toml",
                "--on-platform",
                "linux-ci",
                "--triggered-by",
                "pyproject.toml",
                "--surface",
                "pyproject-deps",
                "--verified-at-sha",
                "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901",
                "--authority",
                "user-ruled",
                "--blame",
                "Wang Yu",
            ]),
        );

        // then: refused — a tripwire stays falsifiable; counter-test is required even for a closed road
        assert!(e.is_err());
    }

    #[test]
    fn decide_should_still_refuse_a_revisit_on_a_rejected_road_even_when_user_ruled() {
        // given: a user-ruled --reject road with a --revisit human re-check (not a test tripwire)
        let r = repo();

        // when: the decision is captured
        let e = run(
            &r,
            Some("keep Redis out"),
            &s(&[
                "--reject",
                "Redis: a new infra dependency",
                "--revisit",
                "Q3 infra review",
                "--authority",
                "user-ruled",
                "--blame",
                "Wang Yu",
            ]),
        );

        // then: refused — a closed road is not re-confirmed by a human re-check; only a structural tripwire
        assert!(e.is_err());
    }

    #[test]
    fn decide_should_error_when_there_is_no_store() {
        // given: a directory with no .evolving/ store
        let p = std::env::temp_dir().join(format!("ev-nostore-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();

        // when: a decision is captured there
        let e = run(&p, Some("d"), &s(&["--blame", "x"]));

        // then: it errors
        assert!(e.is_err());
    }

    #[test]
    fn decide_should_build_a_self_verifying_test_binding_when_all_test_fields_are_present() {
        // given: a store and decide args with a fully specified test binding plus a rejected road
        let r = repo();

        // when: the decision is captured
        let t = run(
            &r,
            Some("restore-safety counter DB-backed; reject Redis"),
            &s(&[
                "--assume",
                "Argus introduces no Redis; multi-pod coord via existing DB",
                "--assume-test",
                "pytest tests/test_redis_absent.py",
                "--counter-test",
                "pytest tests/test_redis_absent.py::test_redis_injection_flips_red",
                "--on-platform",
                "linux-ci",
                "--triggered-by",
                "pyproject.toml",
                "--surface",
                "pyproject-deps",
                "--verified-at-sha",
                "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901",
                "--reject",
                "Redis: a new infra dependency",
                "--blame",
                "Wang Yu",
            ]),
        )
        .expect("ok");

        // then: the first ground carries a fully populated test check
        match &t.grounds[0].check {
            Some(Check::Test {
                reference,
                counter_test,
                liveness,
                verified_at_sha,
            }) => {
                assert_eq!(reference, "pytest tests/test_redis_absent.py");
                assert!(counter_test
                    .as_deref()
                    .is_some_and(|c| c.contains("flips_red")));
                assert_eq!(liveness.platforms, vec!["linux-ci".to_string()]);
                assert_eq!(verified_at_sha.len(), 40);
            }
            _ => panic!("expected a test check"),
        }
    }

    #[test]
    fn decide_should_reject_a_test_binding_when_there_is_no_counter_test() {
        // given: a store and a test binding missing --counter-test
        let r = repo();

        // when: the decision is captured
        let e = run(
            &r,
            Some("d"),
            &s(&[
                "--assume",
                "c",
                "--assume-test",
                "pytest x",
                "--on-platform",
                "linux-ci",
                "--triggered-by",
                "f",
                "--surface",
                "s",
                "--verified-at-sha",
                "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901",
                "--blame",
                "Wang Yu",
            ]),
        );

        // then: it is rejected
        assert!(e.is_err());
    }

    #[test]
    fn decide_should_reject_a_test_binding_when_the_counter_test_is_empty() {
        // given: a store and a test binding whose --counter-test is empty
        let r = repo();

        // when: the decision is captured with an empty counter-test
        let e = run(
            &r,
            Some("d"),
            &s(&[
                "--assume",
                "c",
                "--assume-test",
                "pytest x",
                "--counter-test",
                "",
                "--on-platform",
                "linux-ci",
                "--triggered-by",
                "f",
                "--surface",
                "s",
                "--verified-at-sha",
                "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901",
                "--blame",
                "Wang Yu",
            ]),
        );

        // then: an empty counter-test is a vacuous binding — rejected at the write path too
        assert!(e.is_err());
    }

    #[test]
    fn decide_should_reject_a_test_binding_when_there_is_no_verified_at_sha_and_no_git() {
        // given: a store and a test binding with no --verified-at-sha in a non-git dir
        let r = repo();

        // when: the decision is captured
        let e = run(
            &r,
            Some("d"),
            &s(&[
                "--assume",
                "c",
                "--assume-test",
                "pytest x",
                "--counter-test",
                "ct",
                "--on-platform",
                "linux-ci",
                "--triggered-by",
                "f",
                "--surface",
                "s",
                "--blame",
                "Wang Yu",
            ]),
        );

        // then: it is rejected
        assert!(e.is_err());
    }

    #[test]
    fn migrate_bind_should_build_a_harvested_test_check_when_no_counter_test() {
        // given: the migrate-only inputs — a ref, a sha, and the FULL 3-key liveness, but NO
        // counter-test (you cannot half-harvest: liveness stays mandatory, falsifiability does not)
        let check = harvested_test_check(
            "pytest tests/test_invariant_no_redis.py".into(),
            "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901".into(),
            vec!["linux-ci".into()],
            vec!["pyproject.toml".into()],
            vec!["pyproject-deps".into()],
        )
        .expect("the full liveness is present, so the harvested binding is well-formed");

        // then: it is a Test check carrying counter_test None (harvested) with liveness intact
        match check {
            Check::Test {
                reference,
                counter_test,
                liveness,
                verified_at_sha,
            } => {
                assert_eq!(reference, "pytest tests/test_invariant_no_redis.py");
                assert_eq!(counter_test, None); // harvested: falsifiability not yet proven
                assert_eq!(liveness.platforms, vec!["linux-ci".to_string()]);
                assert_eq!(liveness.triggered_by, vec!["pyproject.toml".to_string()]);
                assert_eq!(liveness.surfaces, vec!["pyproject-deps".to_string()]);
                assert_eq!(verified_at_sha.len(), 40);
            }
            _ => panic!("expected a harvested test check"),
        }
    }

    #[test]
    fn migrate_bind_should_reject_a_harvested_binding_when_a_liveness_key_is_missing() {
        // given: the migrate-only inputs with an empty surfaces key (a half-harvest attempt)
        let e = harvested_test_check(
            "pytest x".into(),
            "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901".into(),
            vec!["linux-ci".into()],
            vec!["pyproject.toml".into()],
            vec![], // no --surface: the 3-key liveness is incomplete
        );

        // then: it is rejected — harvesting drops the counter-test, never the liveness
        assert!(e.is_err());
    }

    #[test]
    fn decide_should_still_error_without_a_counter_test() {
        // given: the migrate-only harvested path now exists; pin that the decide path is UNCHANGED
        // — a `--assume-test` binding with full liveness but no --counter-test STILL errors (the
        // strict capture.rs guard stays byte-for-byte; harvesting is migrate-only, not decide-wide).
        let r = repo();

        // when: a decision binds a test with full liveness but omits --counter-test
        let e = run(
            &r,
            Some("d"),
            &s(&[
                "--assume",
                "c",
                "--assume-test",
                "pytest x",
                "--on-platform",
                "linux-ci",
                "--triggered-by",
                "f",
                "--surface",
                "s",
                "--verified-at-sha",
                "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901",
                "--blame",
                "Wang Yu",
            ]),
        );

        // then: it errors — no vacuous binding on the decide path
        assert!(e.is_err());
    }

    #[test]
    fn append_should_compute_the_frozen_genesis_id_when_given_the_genesis_decision() {
        // given: a store and the genesis decision assembled as a Decision (the SAME fields the
        // golden vector freezes) — proving decide + migrate share one compute_id / write_tick path.
        let r = repo();
        let d = Decision {
            observe: "evaluating retrieval backend".into(),
            decision: "freeze the retrieval schema for v2".into(),
            grounds: vec![
                Ground {
                    claim: "team still wants a frozen schema".into(),
                    supports: "chosen".into(),
                    check: Some(Check::Person {
                        reference: "Q3 infra review".into(),
                    }),
                },
                Ground {
                    claim: "pgvector would lock our schema".into(),
                    supports: "rejected:pgvector".into(),
                    check: None,
                },
            ],
            blame: "Wang Yu".into(),
            authority: None,
            jurisdiction: None,
            source_ref: None,
            provenance: None,
        };

        // when: it is appended onto the empty store (genesis: parent_id == "")
        let t = append(&r, d).expect("ok");

        // then: the content-addressed id matches the frozen genesis golden — the shared append
        // hashes byte-identically to the legacy decide tail (no golden drift from the refactor).
        assert_eq!(t.id, "e2b337f53a1f");
        assert_eq!(t.parent_id, "");
        assert_eq!(Store::at(&r).read_head().unwrap(), t.id);
    }

    #[test]
    fn decide_should_take_blame_from_git_config_when_no_blame_flag_is_given() {
        // given: a store inside a git repo with a configured author, and no --blame
        let r = repo();
        for a in [
            ["init"].as_slice(),
            ["config", "user.name", "Ada Lovelace"].as_slice(),
        ] {
            std::process::Command::new("git")
                .args(a)
                .current_dir(&r)
                .output()
                .unwrap();
        }

        // when: a decision is captured without --blame
        let t = run(&r, Some("d"), &s(&["--assume", "c"])).expect("ok");

        // then: blame is resolved from git config user.name
        assert_eq!(t.blame, "Ada Lovelace");
    }
}
