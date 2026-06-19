//! `ev migrate` — backfill an existing decision history into the ledger.
//!
//! Four PURE, format-aware extractors turn a source substrate (`&str`) into a `Vec<MigrationRecord>`:
//! a chat-room/git log (`## R<N>` records), the `to-human` RESOLVED/FLAG markdown blocks (the
//! authority substrate), a `decisions-immutable` §N document, and an `escalation` log (the SAME
//! RESOLVED/FLAG reader, path-parameterized). The extractors parse **rulings + structured
//! rejected-roads only** — they NEVER NLP a free-text reason into a ground (`grounds_are_never_
//! synthesized`): a road becomes a ground iff the source declares it structurally (a `rejected:`
//! token), otherwise the record carries zero grounds and stays an honest capture.
//!
//! The command driver then runs an IDEMPOTENT backfill loop (deterministic source_key sort →
//! prospective-parent compute_id → ticks_dir pre-check → skip-if-present) on top of the shared
//! `capture::append`, plus a `--reconcile` join and a `--bind-check` harvest.

use crate::canonical::compute_id;
use crate::capture::{harvested_test_check, Decision};
use crate::store::Store;
use crate::tick::{Ground, Tick};
use std::path::Path;

/// One extracted, not-yet-appended decision from a source substrate. `source_key` is the stable,
/// deterministic dedup/sort key (e.g. `R2289`, `#555`, `§3`) used to order the backfill and to
/// reconcile against the store; `observe` carries that key as a durable token so reconcile can read
/// it back from the HASHED payload, not from the events log. Grounds are ONLY the structurally
/// declared rejected-roads — never synthesized from prose.
#[derive(Debug, Clone, PartialEq)]
pub struct MigrationRecord {
    pub source_key: String,
    pub decision: String,
    pub observe: String,
    pub blame: Option<String>,
    pub grounds: Vec<Ground>,
}

/// A `#<n>` / `R<n>` provenance token (issue or round id), leading-char + all-digits. Mirrors the
/// `subject_refs` vocabulary in capture.rs but returns the FIRST `R<n>`/`#<n>` as a stable key.
fn first_round_or_issue_token(text: &str) -> Option<String> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '#'))
        .find(|tok| {
            let rest = tok
                .strip_prefix('#')
                .or_else(|| tok.strip_prefix('R'))
                .or_else(|| tok.strip_prefix('r'));
            matches!(rest, Some(d) if !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
        })
        .map(|t| t.to_string())
}

/// Parse the structurally-declared rejected-roads out of a block's lines. A road is declared ONLY by
/// an explicit `rejected: <option>: <why>` (or `reject <option>: <why>`) line — never inferred from
/// prose. Returns one `rejected:<option>` ground per declared road, in source order. A block with no
/// such line yields zero grounds (the honesty contract: no synthesis).
fn structured_rejected_roads(block: &str) -> Vec<Ground> {
    let mut out = Vec::new();
    for line in block.lines() {
        let l = line.trim_start_matches(['-', '*', ' ', '\t']).trim();
        let body = l
            .strip_prefix("rejected:")
            .or_else(|| l.strip_prefix("rejected "))
            .or_else(|| l.strip_prefix("reject:"))
            .or_else(|| l.strip_prefix("reject "));
        if let Some(rest) = body {
            if let Some((opt, why)) = rest.split_once(':') {
                let (opt, why) = (opt.trim(), why.trim());
                if !opt.is_empty() && !why.is_empty() {
                    out.push(Ground {
                        claim: why.to_string(),
                        supports: format!("rejected:{opt}"),
                        check: None,
                    });
                }
            }
        }
    }
    out
}

/// Build one MigrationRecord from a parsed (key, decision) header + its block body: observe carries the
/// source_key as durable provenance, grounds are the structurally-declared rejected-roads only (never
/// synthesized), blame is left for the backfill's `--blame` fallback. Shared by all three block extractors.
fn flush_record(header: &Option<(String, String)>, body: &str, out: &mut Vec<MigrationRecord>) {
    if let Some((key, decision)) = header {
        out.push(MigrationRecord {
            source_key: key.clone(),
            decision: decision.clone(),
            observe: key.clone(),
            blame: None,
            grounds: structured_rejected_roads(body),
        });
    }
}

/// The store-side durable key for a tick: its `round_id` if present, else the first round/`#<n>` token
/// in the hashed `observe` — never the non-hashed events log. Shared by the idempotency index + reconcile,
/// so the two never disagree on key precedence.
fn store_key(raw: &serde_json::Value) -> Option<String> {
    raw.get("round_id")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            raw.get("observe")
                .and_then(|x| x.as_str())
                .and_then(first_round_or_issue_token)
        })
}

/// Extractor 1 — **gitlog / chat-room**: each `## R<N> …` header is one decision; the header text
/// after the round token (and an optional `— ` em-dash separator) is the decision; any structurally
/// declared rejected-road line in that record's body becomes a ground. The `R<N>`/`#<n>` token is the
/// source_key and is carried into observe as a durable provenance token. Reasons are NEVER NLP'd.
pub fn extract_gitlog(text: &str) -> Vec<MigrationRecord> {
    let mut records = Vec::new();
    let mut header: Option<(String, String)> = None; // (source_key, decision)
    let mut body = String::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("## ") {
            flush_record(&header, &body, &mut records);
            body.clear();
            let key = first_round_or_issue_token(rest);
            // The decision text is the header with the leading round token stripped + em-dash trimmed.
            let decision = match key.as_deref() {
                Some(k) => rest
                    .split_once(k)
                    .map(|x| x.1)
                    .unwrap_or(rest)
                    .trim_start_matches([' ', '—', '-', ':'])
                    .trim()
                    .to_string(),
                None => rest.trim().to_string(),
            };
            header = key.map(|k| {
                (
                    k,
                    if decision.is_empty() {
                        rest.trim().into()
                    } else {
                        decision
                    },
                )
            });
        } else {
            body.push_str(line);
            body.push('\n');
        }
    }
    flush_record(&header, &body, &mut records);
    records
}

/// The shared RESOLVED / FLAG block reader (the authority substrate). A `### RESOLVED <key>: <decision>`
/// or `### FLAG <key>: <decision>` header opens a block; the block's body is scanned for structured
/// rejected-roads only. RESOLVED marks a user-ruled decision; FLAG marks an open one — both are
/// captured (the ruling state is provenance, not a reason to drop the record). PATH-PARAMETERIZED by
/// the caller: `to-human` and `escalation` are the SAME reader over different files (no hardcoded
/// layout). Returns records in source order.
fn read_resolved_flag_blocks(text: &str) -> Vec<MigrationRecord> {
    let mut records = Vec::new();
    let mut header: Option<(String, String)> = None;
    let mut body = String::new();
    for line in text.lines() {
        let stripped = line
            .trim_start_matches(['#', ' '])
            .strip_prefix("RESOLVED")
            .or_else(|| line.trim_start_matches(['#', ' ']).strip_prefix("FLAG"));
        if let Some(rest) = stripped {
            flush_record(&header, &body, &mut records);
            body.clear();
            let rest = rest.trim();
            // `<key>: <decision>` — the key is the leading token before the first colon.
            if let Some((key, decision)) = rest.split_once(':') {
                let key = key.trim();
                let source_key = first_round_or_issue_token(key).unwrap_or_else(|| key.to_string());
                header = Some((source_key, decision.trim().to_string()));
            } else {
                let source_key =
                    first_round_or_issue_token(rest).unwrap_or_else(|| rest.to_string());
                header = Some((source_key, rest.to_string()));
            }
        } else {
            body.push_str(line);
            body.push('\n');
        }
    }
    flush_record(&header, &body, &mut records);
    records
}

/// Extractor 2 — **to-human**: the RESOLVED/FLAG markdown blocks (the authority substrate).
pub fn extract_to_human(text: &str) -> Vec<MigrationRecord> {
    read_resolved_flag_blocks(text)
}

/// Extractor 4 — **escalation**: the SAME RESOLVED/FLAG reader, path-parameterized — escalation is
/// just the reader over a different file, with NO hardcoded layout of its own.
pub fn extract_escalation(text: &str) -> Vec<MigrationRecord> {
    read_resolved_flag_blocks(text)
}

/// Extractor 3 — **decisions-immutable**: a document split on `## N.` / `## §N` section headers, one
/// decision per numbered section. The section number is the source_key; the header text after the
/// number is the decision; structured rejected-roads in the section body become grounds.
pub fn extract_decisions_immutable(text: &str) -> Vec<MigrationRecord> {
    let mut records = Vec::new();
    let mut header: Option<(String, String)> = None;
    let mut body = String::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("## ") {
            // A numbered section header: `## 3. <decision>` or `## §3 <decision>`.
            let rest = rest.trim();
            let digits: String = rest
                .trim_start_matches('§')
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if !digits.is_empty() {
                flush_record(&header, &body, &mut records);
                body.clear();
                let decision = rest
                    .trim_start_matches('§')
                    .trim_start_matches(|c: char| c.is_ascii_digit())
                    .trim_start_matches(['.', ' ', ':', '—', '-'])
                    .trim()
                    .to_string();
                header = Some((format!("§{digits}"), decision));
                continue;
            }
        }
        body.push_str(line);
        body.push('\n');
    }
    flush_record(&header, &body, &mut records);
    records
}

/// The outcome of one backfill pass (idempotent): how many records were imported, skipped (already
/// present by content-addressed id), re-linked (a back-dated mid-chain insert that re-parented), and
/// how many were source-only gaps that could not be appended (e.g. a source lacking authors with no
/// `--blame` fallback). Rendered by the command layer.
#[derive(Debug, Default, PartialEq)]
pub struct BackfillSummary {
    pub imported: usize,
    pub skipped: usize,
    pub relinked: usize,
    pub source_only_gaps: usize,
}

/// Map the store's existing decisions to their durable source key → (id, parent_id). The key is read
/// from the HASHED payload: `round_id` if present, else the first round/#N token in `observe` — never
/// from the non-hashed events log. This is the idempotency + re-link index for a backfill pass.
fn store_key_index(
    store: &Store,
) -> Result<std::collections::HashMap<String, (String, String)>, String> {
    let files = store
        .read_all()
        .map_err(|e| format!("reading store: {e}"))?;
    let mut idx = std::collections::HashMap::new();
    for (name, raw) in &files {
        let key = store_key(raw);
        let parent = raw
            .get("parent_id")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        if let Some(k) = key {
            idx.insert(k, (name.clone(), parent));
        }
    }
    Ok(idx)
}

/// Run the idempotent backfill of `records` into the store at `repo`. Deterministic order: records
/// are sorted by `source_key` first so a re-run replays the same chain. Idempotency is keyed on the
/// durable `source_key` (carried into the hashed `observe` + the non-hashed `round_id`): a record
/// whose key is already in the store is SKIPPED — chain-position-independent, so a re-run over a
/// now-non-empty store writes nothing. The chain is kept by threading the PROSPECTIVE parent (the
/// id we just wrote/found) instead of re-reading the live HEAD each step, so the lineage stays
/// stable across re-runs. A skipped record whose stored parent differs from where it would now land
/// is a back-dated mid-chain insert and is reported as re-linked. `blame_fallback` supplies the
/// author for a record carrying none; a record with neither is a source-only gap (R5 stays intact —
/// we never invent an author). `--dry-run` reports the would-import count but writes nothing.
pub fn backfill(
    repo: &Path,
    mut records: Vec<MigrationRecord>,
    blame_fallback: Option<&str>,
    dry_run: bool,
) -> Result<BackfillSummary, String> {
    records.sort_by(|a, b| a.source_key.cmp(&b.source_key));
    let store = Store::at(repo);
    if !store.exists() {
        return Err("no .evolving/ store here — run `ev init` first".into());
    }
    let existing = store_key_index(&store)?;
    // The prospective parent threads through the loop so the chain stays coherent across this pass:
    // for a brand-new store it begins at the live HEAD; as records resolve it advances to each id.
    // For relink detection we compare a found record's STORED parent against where this sorted pass
    // would place it (`prospective_parent`) — equal ⇒ the chain is intact (a clean re-run reports
    // 0); different ⇒ the chain was re-linked around it (a back-dated mid-chain insert).
    let head = store
        .read_head()
        .map_err(|e| format!("reading HEAD: {e}"))?;
    // Seed the prospective parent: if the FIRST sorted record is already the genesis (stored
    // parent ""), the pass replays from genesis; otherwise it extends the current HEAD.
    let first_is_stored_genesis = records
        .first()
        .and_then(|r| existing.get(&r.source_key))
        .map(|(_, p)| p.is_empty())
        .unwrap_or(false);
    let mut prospective_parent = if first_is_stored_genesis {
        String::new()
    } else {
        head
    };
    let mut summary = BackfillSummary::default();
    for r in records {
        // Idempotency PRE-CHECK on the durable source_key (chain-position-independent).
        if let Some((existing_id, existing_parent)) = existing.get(&r.source_key) {
            // A back-dated mid-chain insert: present, but its stored parent differs from where this
            // pass would now place it — the chain was re-linked around it. Reported, never rewritten.
            if *existing_parent != prospective_parent {
                summary.relinked += 1;
            }
            // Keep the chain coherent for any later records in this same pass.
            prospective_parent = existing_id.clone();
            summary.skipped += 1;
            continue;
        }
        let blame = match r.blame.as_deref().or(blame_fallback) {
            Some(b) if !b.trim().is_empty() => b.trim().to_string(),
            _ => {
                // R5 stays intact: no author, no fabrication. Surface the gap; never invent a human.
                summary.source_only_gaps += 1;
                continue;
            }
        };
        if dry_run {
            // The id this record WOULD take at the prospective parent (no write). held_since is
            // non-hashed, so this matches the id `append` computes on a real run — only the real
            // path needs a write, so the probe lives here, not on the hot import path.
            let probe = Tick {
                id: String::new(),
                parent_id: prospective_parent.clone(),
                observe: r.observe.clone(),
                decision: r.decision.clone(),
                grounds: r.grounds.clone(),
                status: "live".into(),
                held_since: String::new(),
                blame: blame.clone(),
                authority: None,
                jurisdiction: None,
                round_id: Some(r.source_key.clone()),
            };
            prospective_parent = compute_id(&probe);
            summary.imported += 1;
            continue;
        }
        let written = crate::capture::append(
            repo,
            Decision {
                observe: r.observe,
                decision: r.decision,
                grounds: r.grounds,
                blame,
                authority: None,
                jurisdiction: None,
                round_id: Some(r.source_key),
            },
        )?;
        prospective_parent = written.id;
        summary.imported += 1;
    }
    Ok(summary)
}

/// A reconcile bucket count: how many source rulings are IN BOTH the source and the store, how many
/// are SOURCE-ONLY (the capture gap — a ruling the source has that the ledger never captured), how
/// many are STORE-ONLY (in the ledger, absent from this source), and how many store ticks could not
/// be keyed at all (no round token in their hashed observe). Keys come from the HASHED `observe` /
/// `round_id`, never from events.jsonl, so they are durable.
#[derive(Debug, Default, PartialEq)]
pub struct ReconcileReport {
    pub in_both: usize,
    pub source_only: usize,
    pub store_only: usize,
    pub un_keyable: usize,
}

/// Reconcile a source's extracted records against the store. The store-side key is read from each
/// tick's HASHED payload — its `round_id` if present, else the first round/#N token in `observe` —
/// so the join is durable (NOT dependent on the non-hashed events log). A source key with no store
/// match is a SOURCE-ONLY gap (the capture gap to surface); a store key with no source match is
/// STORE-ONLY; a store tick with no derivable key is counted separately as un-keyable.
pub fn reconcile(
    repo: &Path,
    source_records: &[MigrationRecord],
) -> Result<ReconcileReport, String> {
    let store = Store::at(repo);
    if !store.exists() {
        return Err("no .evolving/ store here — run `ev init` first".into());
    }
    let files = store
        .read_all()
        .map_err(|e| format!("reading store: {e}"))?;
    let mut store_keys: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut un_keyable = 0usize;
    for (_name, raw) in &files {
        let key = store_key(raw);
        match key {
            Some(k) => {
                store_keys.insert(k);
            }
            None => un_keyable += 1,
        }
    }
    let source_keys: std::collections::HashSet<String> = source_records
        .iter()
        .map(|r| r.source_key.clone())
        .collect();
    let mut report = ReconcileReport {
        un_keyable,
        ..Default::default()
    };
    for k in &source_keys {
        if store_keys.contains(k) {
            report.in_both += 1;
        } else {
            report.source_only += 1;
        }
    }
    report.store_only = store_keys
        .iter()
        .filter(|k| !source_keys.contains(*k))
        .count();
    Ok(report)
}

/// The `--bind-check` harvest: build a harvested `Check::Test` (counter_test None, full liveness) for
/// the given selector, reusing the Task-5 migrate-only constructor. This is the SAME constructor the
/// harvested-binding path uses — no second half-harvest gate. The caller attaches it to a ground.
pub fn bind_check(
    selector: String,
    verified_at_sha: String,
    platforms: Vec<String>,
    triggered_by: Vec<String>,
    surfaces: Vec<String>,
) -> Result<crate::tick::Check, String> {
    harvested_test_check(selector, verified_at_sha, platforms, triggered_by, surfaces)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_gitlog_should_yield_one_record_per_round_header_when_given_a_chat_room_log() {
        // given: a chat-room log with two `## R<N>` decision records, one carrying a rejected road
        let text = "\
## R2289 QA — restore-safety counter DB-backed
- rejected: Redis: would add a new infra dependency
## R2290 Dev — ship the cross-pod drain
some prose nobody parses for grounds
";

        // when: the gitlog extractor reads it
        let recs = extract_gitlog(text);

        // then: two records, keyed by their round token, the first carrying the structured road
        assert_eq!(recs.len(), 2);
        assert_eq!(recs[0].source_key, "R2289");
        assert_eq!(recs[0].decision, "QA — restore-safety counter DB-backed");
        assert_eq!(recs[0].grounds.len(), 1);
        assert_eq!(recs[0].grounds[0].supports, "rejected:Redis");
        assert_eq!(recs[1].source_key, "R2290");
        assert!(recs[0].observe.contains("R2289"));
    }

    #[test]
    fn extract_to_human_should_read_a_resolved_block_when_given_the_authority_substrate() {
        // given: a to-human doc with a RESOLVED ruling and a FLAG (open) one
        let text = "\
### RESOLVED R555: restore-safety counter DB-backed; reject Redis
- rejected: Redis: a new infra dependency
### FLAG R600: multi-pod relax policy still open
";

        // when: the to-human extractor reads it
        let recs = extract_to_human(text);

        // then: both blocks are captured; the RESOLVED one carries its structured road
        assert_eq!(recs.len(), 2);
        assert_eq!(recs[0].source_key, "R555");
        assert_eq!(
            recs[0].decision,
            "restore-safety counter DB-backed; reject Redis"
        );
        assert_eq!(recs[0].grounds.len(), 1);
        assert_eq!(recs[1].source_key, "R600");
    }

    #[test]
    fn extract_escalation_should_reuse_the_resolved_flag_reader_when_given_an_escalation_log() {
        // given: an escalation log in the SAME RESOLVED/FLAG shape (path-parameterized reader)
        let text = "### FLAG #1194: re-milestoned without sign-off\n";

        // when: the escalation extractor reads it
        let recs = extract_escalation(text);

        // then: it is read identically to to-human (no hardcoded layout of its own)
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].source_key, "#1194");
        assert_eq!(recs[0].decision, "re-milestoned without sign-off");
    }

    #[test]
    fn extract_decisions_immutable_should_split_on_numbered_sections_when_given_a_doc() {
        // given: a decisions-immutable doc split into numbered sections
        let text = "\
## 1. freeze the retrieval schema for v2
- rejected: pgvector: would lock our schema
## 2. restore-safety counter DB-backed
";

        // when: the decisions-immutable extractor reads it
        let recs = extract_decisions_immutable(text);

        // then: one record per section, keyed by §N, the first carrying its structured road
        assert_eq!(recs.len(), 2);
        assert_eq!(recs[0].source_key, "§1");
        assert_eq!(recs[0].decision, "freeze the retrieval schema for v2");
        assert_eq!(recs[0].grounds.len(), 1);
        assert_eq!(recs[1].source_key, "§2");
    }

    #[test]
    fn grounds_are_never_synthesized_when_a_block_has_no_structured_rejected_road() {
        // given: a record whose body is pure prose mentioning a rejected option WITHOUT the
        // structured `rejected:<opt>: <why>` token — an NLP'able sentence we must NOT mine
        let text = "\
## R2289 we considered Redis but rejected it because it adds infra
this paragraph explains at length why redis was rejected, in prose
";

        // when: the gitlog extractor reads it
        let recs = extract_gitlog(text);

        // then: the record exists but carries ZERO grounds — reasons are never NLP'd into grounds
        assert_eq!(recs.len(), 1);
        assert!(
            recs[0].grounds.is_empty(),
            "a prose reason must NEVER become a ground (no synthesis)"
        );
    }
}
