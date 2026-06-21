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
use std::collections::HashMap;
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
    // The bookkeeping tags a producer may declare. The four built-in extractors leave them at the
    // legacy defaults (authority None, jurisdiction None — so the `--jurisdiction-map` fills it —,
    // source_ref = the source_key token, provenance None); the canonical reader populates them from the
    // wire record so an imported ruling lands with its true authority / jurisdiction / provenance.
    pub authority: Option<String>,
    pub jurisdiction: Option<String>,
    pub source_ref: Option<serde_json::Value>,
    pub provenance: Option<String>,
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
            // Legacy defaults: no inline authority/provenance, source_ref = the source_key token, and
            // jurisdiction left None so the `--jurisdiction-map` remains the sole tagger on this path.
            authority: None,
            jurisdiction: None,
            source_ref: Some(serde_json::Value::String(key.clone())),
            provenance: None,
        });
    }
}

/// The store-side durable key for a tick: the dedup key derived from its opaque `source_ref` if
/// present (a string verbatim, or an object's deterministic JSON — see `source_ref_key`), else the
/// first round/`#<n>` token in the hashed `observe` — never the non-hashed events log. Shared by the
/// idempotency index + reconcile, so the two never disagree on key precedence.
fn store_key(raw: &serde_json::Value) -> Option<String> {
    raw.get("source_ref")
        .map(crate::tick::source_ref_key)
        .or_else(|| {
            raw.get("observe")
                .and_then(|x| x.as_str())
                .and_then(first_round_or_issue_token)
        })
}

/// The closed key set of a Canonical Decision Intake line. The wire envelope is STRICT — unlike a
/// stored tick (which tolerates an unknown non-hashed key as forward-compat), an external producer's
/// line with an unknown key is a hard failure, so a mis-piped file cannot smuggle a field past ingest.
const CANONICAL_KEYS: &[&str] = &[
    "kind",
    "decision",
    "observe",
    "grounds",
    "blame",
    "authority",
    "jurisdiction",
    "source_ref",
    "provenance",
];

/// Parse a **Canonical Decision Intake** stream (JSONL) into `MigrationRecord`s — the format-neutral
/// intake both an adopter's legacy adapter and a future live runner emit. This IS the trust boundary:
/// the producer supplies STRUCTURE, and ev RE-VALIDATES it here through the very read-path validators
/// (`ground_from_value`, the vocab checks) that guard an on-disk tick — never a parallel serde decode
/// that could trust an unchecked `Ground`. Per line: skip blank / `#`-comment lines; require the fixed
/// `kind` discriminator and reject any unknown envelope key loudly; require a non-empty `decision` and
/// a `grounds` array (which may be empty — the honest zero-grounds capture); validate every declared
/// tag against its closed vocabulary. The durable dedup/sort key mirrors `store_key`: the opaque
/// `source_ref`'s derived key, else the first round/`#issue` token in `observe`.
pub fn canonical_records(text: &str) -> Result<Vec<MigrationRecord>, String> {
    use crate::capture::validate_authority;
    use crate::tick::{
        ground_from_value, only_keys, req_str, source_ref_key, validate_jurisdiction,
        validate_provenance, validate_source_ref,
    };
    let mut out = Vec::new();
    for (i, raw_line) in text.lines().enumerate() {
        let n = i + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let v: serde_json::Value =
            serde_json::from_str(line).map_err(|e| format!("canonical line {n}: not JSON: {e}"))?;
        let obj = v
            .as_object()
            .ok_or_else(|| format!("canonical line {n}: not a JSON object"))?;
        only_keys(obj, CANONICAL_KEYS, &format!("canonical line {n}"))?;
        match obj.get("kind").and_then(|x| x.as_str()) {
            Some("ev-decision-intake") => {}
            other => {
                return Err(format!(
                    "canonical line {n}: not an ev-decision-intake record (kind={other:?})"
                ))
            }
        }
        let decision = req_str(obj, "decision").map_err(|e| format!("canonical line {n}: {e}"))?;
        if decision.trim().is_empty() {
            return Err(format!("canonical line {n}: decision is empty"));
        }
        let observe = obj
            .get("observe")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let grounds_v = obj
            .get("grounds")
            .and_then(|x| x.as_array())
            .ok_or_else(|| format!("canonical line {n}: grounds missing/not array"))?;
        let mut grounds = Vec::new();
        for gv in grounds_v {
            grounds.push(ground_from_value(gv).map_err(|e| format!("canonical line {n}: {e}"))?);
        }
        let blame = obj
            .get("blame")
            .and_then(|x| x.as_str())
            .map(str::to_string);
        // One validated optional string tag: absent → None; present → vocab-checked, with the line
        // number threaded into the error. (source_ref is a raw Value, so it stays its own arm below.)
        let opt_tag = |key: &str,
                       validate: fn(&str) -> Result<(), String>|
         -> Result<Option<String>, String> {
            match obj.get(key).and_then(|x| x.as_str()) {
                None => Ok(None),
                Some(v) => {
                    validate(v).map_err(|e| format!("canonical line {n}: {e}"))?;
                    Ok(Some(v.to_string()))
                }
            }
        };
        let authority = opt_tag("authority", validate_authority)?;
        let jurisdiction = opt_tag("jurisdiction", validate_jurisdiction)?;
        let provenance = opt_tag("provenance", validate_provenance)?;
        let source_ref = match obj.get("source_ref") {
            None => None,
            Some(rv) => {
                validate_source_ref(rv).map_err(|e| format!("canonical line {n}: {e}"))?;
                Some(rv.clone())
            }
        };
        // The dedup/sort key mirrors store_key's precedence: the source_ref's derived key, else the
        // first round/`#issue` token in observe. A record that yields NEITHER has no durable identity,
        // so re-imports could not be idempotent and distinct records would collide on the empty key —
        // reject it at the door (mirroring the strict envelope), rather than silently keying it "".
        let source_key = source_ref
            .as_ref()
            .map(source_ref_key)
            .or_else(|| first_round_or_issue_token(&observe))
            .filter(|k| !k.is_empty());
        let source_key = match source_key {
            Some(k) => k,
            None => {
                return Err(format!(
                    "canonical line {n}: a record needs a source_ref (or a round/#issue token in observe) for idempotent re-import"
                ))
            }
        };
        out.push(MigrationRecord {
            source_key,
            decision,
            observe,
            blame,
            grounds,
            authority,
            jurisdiction,
            source_ref,
            provenance,
        });
    }
    Ok(out)
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
    /// A re-imported record whose RESOLVED non-hashed tags (authority/jurisdiction/provenance) differ
    /// from the already-stored tick. Ticks are immutable, so the difference is reported, NEVER applied —
    /// surfaced (not silently skipped) so a corrected ruling is never invisibly dropped.
    pub discrepancies: usize,
}

/// Map the store's existing decisions to their durable source key → (id, parent_id). The key is the
/// derived dedup key of the non-hashed `source_ref` if present, else the first round/#N token in the
/// hashed `observe` — never the non-hashed events log. The idempotency + re-link index for a backfill.
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
/// durable `source_key` (the non-hashed `source_ref`'s derived key, or a token in the hashed `observe`): a record
/// whose key is already in the store is SKIPPED — chain-position-independent, so a re-run over a
/// now-non-empty store writes nothing. The chain is kept by threading the PROSPECTIVE parent (the
/// id we just wrote/found) instead of re-reading the live HEAD each step, so the lineage stays
/// stable across re-runs. A skipped record whose stored parent differs from where it would now land
/// is a back-dated mid-chain insert and is reported as re-linked. `blame_fallback` supplies the
/// author for a record carrying none; a record with neither is a source-only gap (R5 stays intact —
/// we never invent an author). `jurisdiction_map` (source_key → A/B/C/D bucket) tags each imported
/// decision: a record whose key is in the map carries that jurisdiction, one absent imports untagged
/// (None) — so the map is purely additive (an empty map ⇒ every record None, the prior behavior).
/// jurisdiction is NON-hashed, so tagging never moves a tick id (idempotency holds across re-runs).
/// `--dry-run` reports the would-import count but writes nothing.
pub fn backfill(
    repo: &Path,
    mut records: Vec<MigrationRecord>,
    blame_fallback: Option<&str>,
    jurisdiction_map: &HashMap<String, String>,
    dry_run: bool,
) -> Result<BackfillSummary, String> {
    records.sort_by(|a, b| a.source_key.cmp(&b.source_key));
    let store = Store::at(repo);
    if !store.exists() {
        return Err("no .evolving/ store here — run `ev init` first".into());
    }
    // The running source_key index, seeded from the store and EXTENDED as each record is written, so a
    // WITHIN-pass duplicate key (two records — e.g. a gitlog R555 and a to-human R555 across two
    // --source files — sharing a key but absent from the store) routes into the skip/report arm instead
    // of silently double-importing. `initial_keys` remembers the seed so a within-pass duplicate is not
    // misreported as a back-dated relink.
    let mut existing = store_key_index(&store)?;
    let initial_keys: std::collections::HashSet<String> = existing.keys().cloned().collect();
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
        // Resolve the declared non-hashed tags the SAME way the write path does, BEFORE the skip
        // check — so the idempotency-skip arm can compare them against the stored tick, and a
        // jurisdiction conflict is caught whether or not the record is new. Inline jurisdiction WINS
        // over the `--jurisdiction-map`; the map fills only a record that declares none; a record
        // declaring a DIFFERENT bucket than the map is a hard error (two sources of truth disagree).
        let jurisdiction = match (
            r.jurisdiction.as_deref(),
            jurisdiction_map.get(&r.source_key),
        ) {
            (Some(inline), Some(mapped)) if inline != mapped => {
                return Err(format!(
                    "source {:?}: inline jurisdiction {inline:?} conflicts with the --jurisdiction-map entry {mapped:?}",
                    r.source_key
                ));
            }
            (Some(inline), _) => Some(inline.to_string()),
            (None, mapped) => mapped.cloned(),
        };
        let authority = r.authority.clone();
        let source_ref = r.source_ref.clone();
        // The migrate verb backfills HISTORY: a record with no declared provenance is stamped
        // `imported`. An explicit value (a live runner emitting `agent-proposed` / `human-now`) wins.
        // `ev decide` / `ev guard` never reach here, so fresh authorship is never stamped imported.
        let provenance = r
            .provenance
            .clone()
            .or_else(|| Some("imported".to_string()));

        // Idempotency PRE-CHECK on the durable source_key (chain-position-independent).
        if let Some((existing_id, existing_parent)) = existing.get(&r.source_key) {
            // A back-dated mid-chain insert: present in the INITIAL store, but its stored parent differs
            // from where this pass would now place it — the chain was re-linked around it. Reported,
            // never rewritten. Gated on `initial_keys` so a within-pass duplicate (added to `existing`
            // this pass) is not misreported as a relink.
            if initial_keys.contains(&r.source_key) && *existing_parent != prospective_parent {
                summary.relinked += 1;
            }
            // A re-import NEVER rewrites a tick (immutability). But if the record's RESOLVED non-hashed
            // tags differ from the stored tick, that is a real faithfulness difference — SURFACE it
            // loudly, never drop it silently (a silent skip of a corrected authority is the false-green
            // ev exists to refuse). The human resolves it with `ev correct`. Mirrors the re-linked
            // report: detect a difference on a present record, report it, never rewrite.
            if let Ok(Some(stored)) = store.read_tick(existing_id) {
                let diffs: Vec<String> = [
                    ("authority", &stored.authority, &authority),
                    ("jurisdiction", &stored.jurisdiction, &jurisdiction),
                    ("provenance", &stored.provenance, &provenance),
                ]
                .iter()
                .filter(|(_, s, i)| s != i)
                .map(|(label, s, i)| format!("{label} stored={s:?} incoming={i:?}"))
                .collect();
                if !diffs.is_empty() {
                    summary.discrepancies += 1;
                    eprintln!(
                        "discrepancy: source {:?} (tick {existing_id}): {} — NOT applied (ticks are immutable; resolve with `ev correct {existing_id}`)",
                        r.source_key,
                        diffs.join("; ")
                    );
                }
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
        // Ingest-boundary structural gates — the SAME refusals `ev verify` enforces at rest, applied at
        // the door so a malformed record never lands. A C/D (detect-only) decision may carry no runnable
        // Test check (one shared predicate with verify, so they cannot drift):
        if crate::tick::detect_only_carries_test(jurisdiction.as_deref(), &r.grounds) {
            return Err(format!(
                "source {:?}: a {} jurisdiction (detect-only) decision cannot carry a runnable test check",
                r.source_key,
                jurisdiction.as_deref().unwrap_or("")
            ));
        }
        // And a harvested check (a Test with no counter-test) is allowed ONLY for imported history — a
        // fresh `agent-proposed` binding must prove falsifiability with a counter-test, exactly as decide.
        for g in &r.grounds {
            if let Some(crate::tick::Check::Test {
                counter_test: None, ..
            }) = &g.check
            {
                if provenance.as_deref() != Some("imported") {
                    return Err(format!(
                        "source {:?}: a harvested test check (no counter-test) is allowed only for imported history, not {}",
                        r.source_key,
                        provenance.as_deref().unwrap_or("human-now")
                    ));
                }
            }
        }
        // A rejected-road TRIPWIRE (a Test check on a rejected: road) is a GATING capability, so the
        // canonical door admits it under the SAME authoring rule decide/guard enforce: the decision
        // must be authority=user-ruled (a human's deliberate closed-road ruling) AND the check must
        // carry a counter-test (no harvested/non-falsifiable rejected-road tripwire — stricter than
        // the general harvested gate above, which lets imported history harvest a chosen ground). This
        // makes the user-ruled-only rule STRUCTURAL across every producer, closing the bypass where a
        // hand-crafted canonical line slips a rejected-road check past ground_from_value's permissive
        // parse. (verify stays permissive at rest: authority is mutable via `ev correct`, so a later
        // re-tag must not retroactively invalidate a legitimately-authored tripwire.)
        for g in &r.grounds {
            if g.supports.starts_with("rejected:") {
                if let Some(crate::tick::Check::Test { counter_test, .. }) = &g.check {
                    if authority.as_deref() != Some("user-ruled") {
                        return Err(format!(
                            "source {:?}: a rejected road can carry a tripwire test only when authority=user-ruled",
                            r.source_key
                        ));
                    }
                    if counter_test.is_none() {
                        return Err(format!(
                            "source {:?}: a rejected-road tripwire requires a counter-test (no harvested tripwire)",
                            r.source_key
                        ));
                    }
                }
            }
        }
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
                authority: authority.clone(),
                jurisdiction: jurisdiction.clone(),
                source_ref: source_ref.clone(),
                provenance: provenance.clone(),
            };
            let probe_id = compute_id(&probe);
            // Extend the running index so a later same-key record this pass routes into the skip arm.
            existing.insert(
                r.source_key.clone(),
                (probe_id.clone(), prospective_parent.clone()),
            );
            prospective_parent = probe_id;
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
                authority,
                jurisdiction,
                source_ref,
                provenance,
            },
        )?;
        // Extend the running index so a later same-key record this pass routes into the skip arm
        // (a within-pass duplicate is detected + reported, never silently double-imported). r.source_key
        // (an owned field untouched by the partial move above) and prospective_parent move in directly.
        existing.insert(r.source_key, (written.id.clone(), prospective_parent));
        prospective_parent = written.id;
        summary.imported += 1;
    }
    Ok(summary)
}

/// A reconcile bucket count: how many source rulings are IN BOTH the source and the store, how many
/// are SOURCE-ONLY (the capture gap — a ruling the source has that the ledger never captured), how
/// many are STORE-ONLY (in the ledger, absent from this source), and how many store ticks could not
/// be keyed at all (no round token in their hashed observe). Keys come from the non-hashed `source_ref`
/// or the hashed `observe`, never from events.jsonl, so they are durable.
#[derive(Debug, Default, PartialEq)]
pub struct ReconcileReport {
    pub in_both: usize,
    pub source_only: usize,
    pub store_only: usize,
    pub un_keyable: usize,
}

/// Reconcile a source's extracted records against the store. The store-side key is read from each
/// the derived key of its non-hashed `source_ref` if present, else the first round/#N token in the
/// hashed `observe` — so the join is durable (NOT dependent on the events log). A source key with no store
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

    // --- canonical intake reader (the trust boundary) ---

    fn canonical_line(extra: &str) -> String {
        // a minimal valid ev-decision-intake line (carrying a source_ref so it has a durable dedup
        // key), with room to splice in extra fields. Tests that OVERRIDE source_ref build inline.
        format!(
            "{{\"kind\":\"ev-decision-intake\",\"decision\":\"no Redis\",\"grounds\":[],\"source_ref\":\"R1\"{extra}}}"
        )
    }

    #[test]
    fn canonical_reader_should_parse_a_full_ruling_record_when_given_a_valid_line() {
        // given: a full ev-decision-intake ruling carrying every declared tag
        let text = "{\"kind\":\"ev-decision-intake\",\"decision\":\"rate-limit at the edge\",\
\"observe\":\"round R1043\",\"grounds\":[{\"claim\":\"edge sees every request\",\"supports\":\"chosen\"},\
{\"claim\":\"app tier double-counts\",\"supports\":\"rejected:app-tier\"}],\"blame\":\"Wang Yu\",\
\"authority\":\"user-ruled\",\"jurisdiction\":\"C\",\"source_ref\":\"R1043\",\"provenance\":\"imported\"}";

        // when: the canonical reader parses it
        let recs = canonical_records(text).expect("valid record");

        // then: every field maps onto the record, grounds re-parsed through the read-path validator
        assert_eq!(recs.len(), 1);
        let r = &recs[0];
        assert_eq!(r.decision, "rate-limit at the edge");
        assert_eq!(r.grounds.len(), 2);
        assert_eq!(r.grounds[1].supports, "rejected:app-tier");
        assert_eq!(r.blame.as_deref(), Some("Wang Yu"));
        assert_eq!(r.authority.as_deref(), Some("user-ruled"));
        assert_eq!(r.jurisdiction.as_deref(), Some("C"));
        assert_eq!(r.source_ref, Some(serde_json::json!("R1043")));
        assert_eq!(r.source_key, "R1043");
        assert_eq!(r.provenance.as_deref(), Some("imported"));
    }

    #[test]
    fn canonical_reader_should_reject_a_line_whose_kind_is_not_ev_decision_intake() {
        // given: a JSON line with the wrong envelope kind (a mis-piped non-intake file)
        let text = "{\"kind\":\"something-else\",\"decision\":\"x\",\"grounds\":[]}";

        // when: the canonical reader parses it
        let result = canonical_records(text);

        // then: it loud-fails (the wire envelope is strict, not forward-compat-tolerant)
        assert!(result.is_err());
    }

    #[test]
    fn canonical_reader_should_reject_an_unknown_envelope_key() {
        // given: an otherwise-valid line carrying a key outside the closed envelope set
        let text = canonical_line(",\"emoji\":\"✅\"");

        // when: the canonical reader parses it
        let result = canonical_records(&text);

        // then: the unknown key is rejected at the door (no format bleeds into core)
        assert!(result.is_err());
    }

    #[test]
    fn canonical_reader_should_reject_a_malformed_ground_via_ground_from_value() {
        // given: a line whose ground has an invalid supports (not chosen / rejected:<opt>)
        let text = "{\"kind\":\"ev-decision-intake\",\"decision\":\"x\",\
\"grounds\":[{\"claim\":\"c\",\"supports\":\"maybe\"}]}";

        // when: the canonical reader parses it
        let result = canonical_records(text);

        // then: it fails through the SAME read-path validator a stored tick uses (the trust boundary)
        assert!(result.is_err());
    }

    #[test]
    fn canonical_reader_should_import_zero_grounds_when_grounds_is_empty() {
        // given: a valid line with an empty grounds array (the honest zero-grounds capture, e.g. a FLAG)
        let text = canonical_line("");

        // when: the canonical reader parses it
        let recs = canonical_records(&text).expect("zero-grounds is first-class");

        // then: the record imports with no grounds (never synthesized)
        assert_eq!(recs.len(), 1);
        assert!(recs[0].grounds.is_empty());
    }

    #[test]
    fn canonical_reader_should_take_source_ref_verbatim_without_resniffing_tokens() {
        // given: a line whose source_ref is an opaque key and whose observe carries a DIFFERENT token
        let text = "{\"kind\":\"ev-decision-intake\",\"decision\":\"no Redis\",\"grounds\":[],\
\"observe\":\"see R2289\",\"source_ref\":\"ticket-42\"}";

        // when: the canonical reader parses it
        let recs = canonical_records(text).expect("valid");

        // then: source_ref and the dedup key are the verbatim source_ref — never re-sniffed from observe
        assert_eq!(recs[0].source_ref, Some(serde_json::json!("ticket-42")));
        assert_eq!(recs[0].source_key, "ticket-42");
    }

    #[test]
    fn canonical_reader_should_key_a_structured_source_ref_by_its_deterministic_json() {
        // given: a line whose source_ref is a STRUCTURED object (richer than a string)
        let text = "{\"kind\":\"ev-decision-intake\",\"decision\":\"no Redis\",\"grounds\":[],\
\"source_ref\":{\"round\":\"R1\",\"sprint\":\"S7\"}}";

        // when: the canonical reader parses it
        let recs = canonical_records(text).expect("valid");

        // then: the object is carried opaquely and the dedup key is its deterministic (sorted) JSON
        assert_eq!(
            recs[0].source_ref,
            Some(serde_json::json!({"round": "R1", "sprint": "S7"}))
        );
        assert_eq!(recs[0].source_key, "{\"round\":\"R1\",\"sprint\":\"S7\"}");
    }

    #[test]
    fn canonical_reader_should_skip_blank_and_comment_lines() {
        // given: a stream padded with a blank line and a #-comment around one record
        let text = format!("\n# a comment\n{}\n\n", canonical_line(""));

        // when: the canonical reader parses it
        let recs = canonical_records(&text).expect("valid");

        // then: only the real record is read (blank/comment lines are skipped, not errors)
        assert_eq!(recs.len(), 1);
    }

    #[test]
    fn canonical_reader_should_reject_a_record_with_no_source_ref_and_no_observe_token() {
        // given: a canonical line with NO source_ref and an observe carrying NO round/#issue token
        let text = "{\"kind\":\"ev-decision-intake\",\"decision\":\"x\",\"grounds\":[],\
\"observe\":\"no token here\"}";

        // when: the canonical reader parses it
        let result = canonical_records(text);

        // then: it is rejected — a record with no durable key cannot be re-imported idempotently
        assert!(
            result.is_err(),
            "an un-keyable record must be refused at the door"
        );
    }

    #[test]
    fn canonical_reader_should_reject_an_out_of_vocab_provenance() {
        // given: a line whose provenance is outside the closed vocabulary
        let text = canonical_line(",\"provenance\":\"self-asserted\"");

        // when: the canonical reader parses it
        let result = canonical_records(&text);

        // then: it fails (provenance is vocab-validated at the boundary, like jurisdiction/authority)
        assert!(result.is_err());
    }
}
