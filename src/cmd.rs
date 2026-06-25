use crate::canonical::compute_id;
use crate::store::Store;
use crate::tick::{Check, Ground, Liveness, Tick};
use crate::verify::verify;
use serde_json::{json, Value};
use std::path::Path;
use std::process::ExitCode;

/// Append a corrective child that fixes a stale non-hashed tag, then report the new child id.
pub fn correct(repo: &Path, a: crate::correct::CorrectArgs) -> ExitCode {
    match crate::correct::run(repo, a) {
        Ok(t) => {
            // No "(N ground(s))": a correction copies the parent's grounds verbatim, so the count is a
            // zero-entropy echo. A count belongs only at CREATION (decide/propose), not an amendment.
            println!("corrected {}", t.id);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Ratify an agent proposal — mint a human-now, user-ruled child carrying the `ratifies` edge.
pub fn ratify(repo: &Path, id: &str, blame: &str) -> ExitCode {
    match crate::ratify::run(
        repo,
        crate::ratify::RatifyArgs {
            id: id.to_string(),
            blame: blame.to_string(),
        },
    ) {
        Ok(t) => {
            println!("ratified {} → {} (now user-ruled, human-now)", id, t.id);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// The identity of a DECISION (not a tick): its hashed payload minus `parent_id`. The LEGACY collapse
/// signal: ticks sharing this — in practice a pre-0.1.10 `ev correct` child and the tick it re-tags
/// (same decision/observe/grounds, a different chain position) — are treated as one decision and
/// collapsed to the latest. Content equality is a weaker signal than the explicit `corrects` edge
/// (two genuinely-independent decisions with byte-identical decision/observe/grounds also share it);
/// since 0.1.10 a correction carries an explicit `corrects:<id>` edge, and the edge supersedes first —
/// content-equality remains only as the fallback that keeps legacy (edge-less) corrections collapsing.
fn decision_identity(t: &Tick) -> String {
    let mut v = crate::canonical::hashed_value(t);
    if let serde_json::Value::Object(m) = &mut v {
        m.remove("parent_id");
    }
    v.to_string()
}

/// Collapse a corrective lineage to its CURRENT state. Two signals, in order:
/// 1. The explicit `corrects:<id>` relation-overlay edge (0.1.10+): a tick whose id is named by some
///    other tick's `corrects` is SUPERSEDED — dropped precisely, regardless of content. (No cycles:
///    `corrects` only ever points backward to an older tick — append-only + content-addressed.)
/// 2. Legacy content-equality fallback: among the survivors, keep only the latest (by `held_since`,
///    then id) per `decision_identity` — so pre-0.1.10 corrective children (no edge) still collapse.
///
/// A decision that was never corrected is its own sole entry.
fn current_decisions(mut ticks: Vec<(String, Tick)>) -> Vec<(String, Tick)> {
    // every id named by a `corrects` edge — explicitly superseded by a correction
    let corrected: std::collections::HashSet<String> = ticks
        .iter()
        .filter_map(|(_, t)| t.corrects.clone())
        .collect();
    // latest-first, so the FIRST seen per decision identity is the current one
    ticks.sort_by(|a, b| b.1.held_since.cmp(&a.1.held_since).then(b.0.cmp(&a.0)));
    let mut seen = std::collections::HashSet::new();
    ticks
        .into_iter()
        .filter(|(_, t)| !corrected.contains(&t.id)) // explicit-edge supersession (precise)
        .filter(|(_, t)| seen.insert(decision_identity(t))) // legacy content-equality fallback
        .collect()
}

/// Parse the raw store files into the current decisions — every parseable tick, collapsed through
/// `current_decisions` (corrective lineages superseded). The shared front of check / pending / brief;
/// each then applies its own status / provenance / visibility filter. (`list` keeps unparseable files,
/// so it parses the long way separately.)
fn parse_current(files: &[(String, Value)]) -> Vec<(String, Tick)> {
    current_decisions(
        files
            .iter()
            .filter_map(|(name, raw)| crate::tick::from_value(raw).ok().map(|t| (name.clone(), t)))
            .collect(),
    )
}

/// Render an opaque `source_ref` for human display: a bare string verbatim, an object as its
/// deterministic compact JSON. ev only renders it — it never interprets the contents. Kept distinct
/// from `tick::source_ref_key` (which derives the dedup/join key): they coincide today but are
/// different concepts — display may later pretty-print, while the key must stay byte-stable.
fn render_source_ref(v: &serde_json::Value) -> String {
    v.as_str()
        .map(String::from)
        .unwrap_or_else(|| v.to_string())
}

/// Whether a triggering change landed after this ground's most recent run. Uses the latest
/// receipt's commit + the binding's triggered_by paths. False when there is no receipt, no
/// Test binding, or git can't tell (None ⇒ not evaluated).
fn triggered_since(
    repo: &std::path::Path,
    ground: &crate::tick::Ground,
    receipts: &[crate::receipt::Receipt],
) -> bool {
    use crate::tick::Check;
    let triggered_by = match &ground.check {
        Some(Check::Test { liveness, .. }) => &liveness.triggered_by,
        _ => return false,
    };
    let latest = receipts.iter().max_by(|a, b| a.ran_at.cmp(&b.ran_at));
    match latest {
        Some(r) => crate::liveness::changed_since(repo, &r.commit, triggered_by).unwrap_or(false),
        None => false,
    }
}

pub fn init(repo: &Path) -> ExitCode {
    let store = Store::at(repo);
    match store.init() {
        Ok(true) => {
            println!("created .evolving/  (content-addressed chain + results cache)");
            ExitCode::SUCCESS
        }
        Ok(false) => {
            println!(".evolving/ already exists (no-op)");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: could not create .evolving/: {e}");
            ExitCode::FAILURE
        }
    }
}
pub fn show(repo: &Path, id: &str) -> ExitCode {
    let store = Store::at(repo);
    // Reject anything that is not a 12-hex tick id BEFORE joining it to the store dir — a `..`/absolute
    // path would otherwise let `ev show` read an arbitrary file (the id can come from a semi-trusted
    // agent wrapper). A malformed id is simply "no such tick".
    if !crate::tick::is_tick_id(id) {
        eprintln!("error: no tick with id {id}");
        return ExitCode::FAILURE;
    }
    let path = store.ticks_dir().join(id);
    if !path.is_file() {
        eprintln!("error: no tick with id {id}");
        return ExitCode::FAILURE;
    }
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            // The raw on-disk tick, verbatim — PURE JSON, so `ev show <id> | jq` is clean. (The
            // authority/jurisdiction/source_ref/corrects/ratifies fields were once re-printed below as
            // label:value lines; that duplicated the JSON, broke a jq pipe, and surfaced an arbitrary
            // subset. A human-readable single-tick view is a separate concern from this machine dump.)
            println!("{text}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: reading {id}: {e}");
            ExitCode::FAILURE
        }
    }
}
/// Clap fills an optional trailing positional with the first token even when it is a flag (the
/// `allow_hyphen_values` lets a leading `--from-git` reach us), so a hyphen-leading "decision" is
/// really a flag: re-route it into args and leave the positional empty. Shared by `decide`/`propose`.
fn reroute_hyphen_positional(
    decision: Option<&str>,
    args: &[String],
) -> (Option<String>, Vec<String>) {
    match decision {
        Some(d) if d.starts_with('-') => {
            let mut v = vec![d.to_string()];
            v.extend_from_slice(args);
            (None, v)
        }
        other => (other.map(String::from), args.to_vec()),
    }
}

/// Pull a no-value BOOLEAN flag out of a decide-style trailing-arg list (a flat `[flag value]*`
/// stream). Pairing-aware: a token in VALUE position (e.g. `--observe "--dry-run"`) is a legitimate
/// value and survives — only a standalone flag-position token counts. A naive global strip would eat
/// the value and misalign every flag after it. Returns `(present, remaining_args)`. Shared by
/// `decide` (`--dry-run`) and `propose` (`--json`).
fn extract_bool_flag(args: &[String], flag: &str) -> (bool, Vec<String>) {
    let mut present = false;
    let mut rest = Vec::with_capacity(args.len());
    let mut i = 0;
    while i < args.len() {
        if args[i] == flag {
            present = true;
            i += 1; // a no-value flag: consume just this token
            continue;
        }
        rest.push(args[i].clone()); // a value-taking flag
        if let Some(val) = args.get(i + 1) {
            rest.push(val.clone()); // its value, kept verbatim
        }
        i += 2;
    }
    (present, rest)
}

pub fn decide(repo: &Path, decision: Option<&str>, args: &[String]) -> ExitCode {
    let (decision, args) = reroute_hyphen_positional(decision, args);
    let decision = decision.as_deref();
    // --dry-run runs every validation + computes the real id, but writes nothing — a safe preview.
    let (dry_run, args) = extract_bool_flag(&args, "--dry-run");

    let assembled = if dry_run {
        crate::capture::build_preview(repo, decision, &args)
    } else {
        crate::capture::run(repo, decision, &args)
    };
    match assembled {
        Ok(t) if dry_run => {
            println!(
                "would record {} ({} ground(s)) — dry run, nothing written",
                t.id,
                t.grounds.len()
            );
            ExitCode::SUCCESS
        }
        Ok(t) => {
            crate::events::append(&Store::at(repo), "decide", Some(&t), None, None, None);
            println!("recorded {} ({} ground(s))", t.id, t.grounds.len());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// The machine sibling of `ev propose` (`--json`): the citable id + the trust fields, so the runner
/// can record what it proposed and later cite it to `ev ratify`. A frozen, parseable envelope.
fn propose_json(t: &Tick) -> String {
    let payload = json!({
        "kind": "ev-proposed",
        "id": t.id,
        "decision": t.decision,
        "provenance": "agent-proposed",
        "authority": "agent-disposable",
        "blame": t.blame,
    });
    serde_json::to_string(&payload).expect("ev-proposed payload serializes")
}

pub fn propose(repo: &Path, decision: Option<&str>, args: &[String]) -> ExitCode {
    let (decision, args) = reroute_hyphen_positional(decision, args);
    let decision = decision.as_deref();
    // --json is the machine sibling (the agent records the id to cite at ratify); never styled.
    let (json_out, args) = extract_bool_flag(&args, "--json");
    match crate::capture::propose(repo, decision, &args) {
        Ok((t, deduped)) => {
            if deduped {
                // Idempotent no-op: this source_ref already names a pending proposal — do not emit a
                // second event or pile up a duplicate (keeps the pending queue triageable).
                if json_out {
                    println!("{}", propose_json(&t));
                } else {
                    println!(
                        "already proposed {:?} as {} for this source_ref — no-op (awaiting ratification)",
                        t.decision, t.id
                    );
                }
                return ExitCode::SUCCESS;
            }
            crate::events::append(&Store::at(repo), "propose", Some(&t), None, None, None);
            if json_out {
                println!("{}", propose_json(&t));
            } else {
                println!(
                    "proposed {} ({} ground(s)) — agent-proposed, awaiting ratification",
                    t.id,
                    t.grounds.len()
                );
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

pub fn guard(repo: &Path, a: crate::guard::GuardArgs) -> ExitCode {
    match crate::guard::run(repo, a) {
        Ok(t) => {
            crate::events::append(&Store::at(repo), "guard", Some(&t), None, None, None);
            println!("bound; wrote child {}", t.id);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

pub fn verify_cmd(repo: &Path, self_test: bool) -> ExitCode {
    if self_test {
        return self_test_golden();
    }
    let store = Store::at(repo);
    // Forward-compat: tolerated unknown top-level keys are warnings, never violations — they do
    // not affect the verdict, but they keep a typo'd field name visible.
    for w in crate::verify::unknown_key_warnings(&store).unwrap_or_default() {
        eprintln!("{w}");
    }
    // Provenance partition: an op-word in faithfully-transcribed imported history is a warning, not a
    // gating violation — surfaced here so it stays visible while fresh authorship keeps the op-lint hard.
    for w in crate::verify::imported_op_warnings(&store).unwrap_or_default() {
        eprintln!("{w}");
    }
    match verify(&store) {
        Ok(v) if v.is_empty() => {
            println!("✓ chain intact: every id == hash(payload), lineage forward-only");
            println!("✓ every tick validates against the closed schema (R1) and check shape (R2)");
            ExitCode::SUCCESS
        }
        Ok(v) => {
            for line in &v {
                println!("✗ {line}");
            }
            eprintln!("{} violation(s)", v.len());
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("error: reading store: {e}");
            ExitCode::FAILURE
        }
    }
}

/// The latest ran_at across a ref's receipts (for the display line), if any.
/// `receipts` is already scoped to one ref by `read_for`, so no filtering is needed.
fn latest_ran_at(receipts: &[crate::receipt::Receipt]) -> Option<String> {
    receipts.iter().map(|r| r.ran_at.clone()).max()
}

/// Roll-up significance for the one-per-tick check event: the tick's single event carries its WORST
/// test-bound verdict, so the de-quintupled count keeps the catch visible. Every GATING verdict (the
/// ones outside the `any_not_green` exclusion — red, silently-unbound, stale, not-run, unproven)
/// outranks a non-gating outcome (green/exempt/memo/n-a), so a co-occurring green can never erase a
/// gating fact's label. Facts only — this orders for the per-tick roll-up, it is not a score on the
/// decision. (A stale hidden behind a worse verdict is separately surfaced via the masked_stale field.)
fn verdict_rank(v: &crate::verdict::Verdict) -> u8 {
    use crate::verdict::Verdict;
    match v {
        Verdict::Red | Verdict::GrayRed => 6,
        // a gating mask-bypass (a touched trigger that was not re-selected) — must outrank green
        Verdict::SilentlyUnbound => 5,
        Verdict::Stale { .. } => 4,
        Verdict::NotRun { .. } => 3,
        Verdict::Unproven => 2,
        Verdict::Memo => 1,
        Verdict::Green | Verdict::Exempt | Verdict::NotApplicable => 0,
    }
}

/// Roll up a tick's test-bound verdicts to the one per-tick check event: `(worst_event_label,
/// masked_stale)`. The worst verdict (by `verdict_rank`) is the event's verdict — a strict `>` keeps
/// the FIRST top-rank ground. `masked_stale` is the sub-kind of the FIRST stale-reading ground (in
/// ground order — each ground's own sub-kind was already resolved by verdict_for's precedence),
/// surfaced ONLY when a worse verdict (red / silently-unbound, rank > stale's 4) hides it — so a
/// drifted/disabled staleness_ref masking a real red never silently drops. Ground order is stable, so
/// this is deterministic per tick. None when no test-bound ground (the tick emits no check event).
fn roll_up_check(verdicts: &[&crate::verdict::Verdict]) -> Option<(String, Option<String>)> {
    use crate::verdict::Verdict;
    let mut worst: Option<(u8, &Verdict)> = None;
    let mut stale: Option<&Verdict> = None;
    for &v in verdicts {
        let rank = verdict_rank(v);
        if worst.map_or(true, |(r, _)| rank > r) {
            worst = Some((rank, v));
        }
        if stale.is_none() && matches!(v, Verdict::Stale { .. }) {
            stale = Some(v);
        }
    }
    worst.map(|(rank, v)| {
        let masked = if rank > 4 {
            stale.map(|s| s.event_label())
        } else {
            None
        };
        (v.event_label(), masked)
    })
}

/// The evaluation context for one `ev check` / `ev reopen` invocation: the staleness reference
/// (resolved per policy by the caller), the selected-list, the wall clock, and the staleness
/// window. The I/O assembly lives here in the command layer so `verdict::verdict_for` stays pure.
fn live_ctx(
    store: &Store,
    staleness_days: u64,
    live_origin_sha: Option<String>,
    attest: Option<Vec<String>>,
) -> crate::verdict::Ctx {
    crate::verdict::Ctx {
        live_origin_sha,
        selected: crate::selected::read(store).unwrap_or(None),
        now_unix: time::OffsetDateTime::now_utc().unix_timestamp(),
        staleness_secs: staleness_days as i64 * 86_400,
        attest,
    }
}

/// One printed check row, captured structurally so the print stage can render it two ways: today's
/// exact tab-separated bytes (legacy / non-TTY / `--plain`) or the rich grammar (a colour TTY). The
/// `class` carries the colour/glyph + the gating/green/non-gating tally bucket.
struct CheckRow {
    label: String,
    class: crate::render::Class,
    id: String,
    decision: String,
    claim: String,
    detail: String,
    // When a not-green verdict was suppressed to Memo (LOCK 1 detect-only / LOCK 3 agent-proposed), why
    // — surfaced on the rich human row so a suppressed red is never mute. None on a normal row.
    memo_reason: Option<String>,
}

pub fn check(
    repo: &Path,
    exit_on_red: bool,
    run: bool,
    platform: &str,
    offline: bool,
    attest: Vec<String>,
    painter: crate::render::Painter,
) -> ExitCode {
    use crate::render::{class_of, Class};
    use crate::verdict::{verdict_for, Verdict};
    let store = Store::at(repo);
    if !store.exists() {
        eprintln!("error: no .evolving/ store here — run `ev init` first");
        return ExitCode::FAILURE;
    }
    let files = match store.read_all() {
        Ok(f) => f,
        Err(e) => {
            eprintln!("error: reading store: {e}");
            return ExitCode::FAILURE;
        }
    };
    let config = crate::config::read(&store);
    // Time the whole check — the COST side of net-effect (the latency ev adds to the agent loop).
    let started = std::time::Instant::now();

    // Collapse each corrective lineage to its current state, then keep only the live ones — exactly as
    // brief/list do. A correction that supersedes (or demotes) a decision then takes effect at the
    // GATE: a stale corrected tick is not-at-head, so it neither runs, prints, nor gates (it stays
    // reachable via `ev log` / `ev show`). Parsing once here also removes check's old double-parse
    // (the --run pass and the verdict loop each re-parsed every file). Unparsable ticks are skipped —
    // `ev verify` owns schema errors, the same contract the per-tick loop had.
    let current: Vec<(String, Tick)> = parse_current(&files)
        .into_iter()
        .filter(|(_, t)| t.status == "live")
        .collect();

    // --run pass: for every live Test-bound ground that declares this platform, run the
    // bound ref locally and append a receipt for it (one local run = one platform receipt).
    if run {
        for (_filename, t) in &current {
            for g in &t.grounds {
                if let Some(Check::Test {
                    reference,
                    counter_test,
                    liveness,
                    ..
                }) = &g.check
                {
                    if liveness.platforms.iter().any(|p| p == platform) {
                        // run the bound check
                        match crate::runner::run_check(
                            repo,
                            reference,
                            platform,
                            config.green_exit_code,
                        ) {
                            Ok(mut rc) => {
                                // prove falsifiability: the counter-test must produce the OPPOSITE
                                // result. A harvested binding (counter_test None) skips this step,
                                // leaving falsifiable None — the existing default.
                                if let Some(counter_test) = counter_test {
                                    match crate::runner::run_check(
                                        repo,
                                        counter_test,
                                        platform,
                                        config.green_exit_code,
                                    ) {
                                        Ok(ct) => rc.falsifiable = Some(rc.result != ct.result),
                                        Err(e) => {
                                            // The binding DECLARES a counter-test but it could not be
                                            // executed — falsifiability is NOT established, so the
                                            // binding must read unproven (it gates), never let the
                                            // check's own green stand. Leaving falsifiable None (the
                                            // old behavior) was a false-green: an un-provable guard
                                            // reading as proven. Some(false) here means "unproven —
                                            // counter could not run", not the vacuous "counter ran and
                                            // agreed" case; the stderr warning differentiates the two
                                            // (the verdict is the same: unproven, which gates).
                                            eprintln!("warning: counter-test {counter_test:?} could not run ({e}) — recording unproven");
                                            rc.falsifiable = Some(false);
                                        }
                                    }
                                }
                                if let Err(e) = crate::receipt::append(&store, &rc) {
                                    eprintln!(
                                        "warning: could not write receipt for {reference:?}: {e}"
                                    );
                                }
                            }
                            Err(e) => eprintln!("warning: run failed for {reference:?}: {e}"),
                        }
                    }
                }
            }
        }
    }

    let live_origin = crate::staleness::resolve(repo, &store, &config.staleness_ref, offline);
    let attest = if attest.is_empty() {
        None
    } else {
        Some(attest)
    };
    let ctx = live_ctx(&store, config.staleness_days, live_origin, attest);
    let mut rows: Vec<CheckRow> = Vec::new();
    let mut any_not_green = false;
    // Harvested-binding honesty debt: N test bindings carry no counter-test (counter_test None) out
    // of M total test bindings. Surfaced as a trailing line so the missing falsifiability proof is
    // never silent — the verdict itself stays honest (a harvested green/red reads exactly as it ran).
    let mut total_test_bindings = 0usize;
    let mut harvested_unproven = 0usize;

    for (filename, t) in &current {
        let mut verdicts = Vec::with_capacity(t.grounds.len());
        // The pre-remap verdicts LOCK 1/3 suppress to memo (test-bound only — non-Test grounds are
        // NotApplicable, never remapped), rolled up for the check event's suppressed_from field so the
        // harness can tell a suppressed-red catch from a benign memo.
        let mut suppressed: Vec<Verdict> = Vec::new();
        for g in &t.grounds {
            // Receipts are read only for Test-bound grounds; person/unbound need none.
            let receipts = match &g.check {
                Some(Check::Test { reference, .. }) => {
                    crate::receipt::read_for(&store, reference).unwrap_or_default()
                }
                _ => Vec::new(),
            };
            // verdict_for returns NotApplicable for any non-Test ground.
            let ts = triggered_since(repo, g, &receipts);
            let mut v = verdict_for(g, &receipts, &ctx, ts);
            // Why a not-green verdict is suppressed to Memo (LOCK 1 detect-only / LOCK 3 agent-proposed),
            // captured for the rich human row — the plain/scriptable path is byte-stable, only rich uses it.
            let mut memo_reason: Option<String> = None;
            // LOCK 1 (gate-time, LEGACY DEFENSE): a C/D-jurisdiction (detect-only) decision never
            // gates. As of 0.1.19 a C/D decision carrying a Test is refused at creation by
            // capture::build() (so it can no longer be authored) — but ev is immutable, and a C/D+Test
            // tick written before that door persists forever. This maps ANY not-green verdict on such a
            // legacy tick to the non-gating Memo BEFORE the any_not_green writer below, so it can never
            // flip --exit-on-red. (Unlike LOCK 3 below — which guards a legitimately-creatable object —
            // LOCK 1 now only defends pre-0.1.19 legacy data; it becomes deletable once no such tick
            // can exist in any ledger.)
            if matches!(t.jurisdiction.as_deref(), Some("C") | Some("D"))
                && !matches!(v, Verdict::Green | Verdict::NotApplicable | Verdict::Exempt)
            {
                memo_reason = Some(format!(
                    "would read {}; detect-only (C/D), never gates",
                    v.label()
                ));
                suppressed.push(v.clone());
                v = Verdict::Memo;
            }
            // LOCK 3 (gate-time, governance): an agent-PROPOSED tick must never flip --exit-on-red —
            // a record DECLARED agent-proposed can't gate; a named human ratifies it into one (§五).
            // (The human/agent line is the propose-vs-decide convention — declared, not verified; LOCK 3
            // enforces only the gating consequence of the declared tag, not who sat at the keyboard.) Map ANY
            // not-green to the non-gating Memo, the gate analogue of brief_visible excluding
            // agent-proposed from the boot-read (defense-in-depth: even if such a tick reaches the
            // gate, it cannot fire it; and it also protects the new rejected-road tripwire — an
            // agent-authored tripwire cannot gate). LOCK 3 is IRREDUCIBLE — an agent-proposed tick
            // carrying a Test is legitimately creatable, so the gate is its only line of defense
            // (whereas LOCK 1 above is now legacy-only). Both map to Memo, so order is irrelevant to
            // the outcome (a tick caught by LOCK 1 is already Memo and skips here).
            if t.provenance.as_deref() == Some("agent-proposed")
                && !matches!(
                    v,
                    Verdict::Green | Verdict::NotApplicable | Verdict::Exempt | Verdict::Memo
                )
            {
                memo_reason = Some(format!(
                    "would read {}; agent-proposed, never gates",
                    v.label()
                ));
                suppressed.push(v.clone());
                v = Verdict::Memo;
            }
            if !matches!(
                v,
                Verdict::Green | Verdict::NotApplicable | Verdict::Exempt | Verdict::Memo
            ) {
                any_not_green = true;
            }
            // Only Test-bound grounds appear in the printed set and the gate.
            if let Some(Check::Test { counter_test, .. }) = &g.check {
                total_test_bindings += 1;
                let harvested = counter_test.is_none();
                let mut detail = match &v {
                    Verdict::NotRun { missing_platforms } => {
                        format!("missing: {}", missing_platforms.join(", "))
                    }
                    Verdict::Stale { reason, .. } => reason.clone(),
                    _ => latest_ran_at(&receipts)
                        .map(|ts| format!("ran {ts}"))
                        .unwrap_or_else(|| "no receipt".into()),
                };
                // A harvested binding carries no counter-test, so its falsifiability was never
                // proven; annotate the row honestly. The verdict is UNCHANGED — a passing harvested
                // test still reads green (pass-green), a failing one still reads red (a real gate).
                if harvested {
                    harvested_unproven += 1;
                    detail = format!("harvested — falsifiability not proven; {detail}");
                    crate::events::append(
                        &store,
                        "harvested",
                        Some(t),
                        Some(&v.event_label()),
                        None,
                        None,
                    );
                }
                rows.push(CheckRow {
                    label: v.label().to_string(),
                    class: class_of(&v),
                    id: filename.clone(),
                    decision: t.decision.clone(),
                    claim: g.claim.clone(),
                    detail,
                    memo_reason,
                });
            }
            verdicts.push((g, v));
        }
        // ONE check event per tick (the de-quintupled count): the worst test-bound verdict, plus a
        // masked_stale companion when a worse verdict hides a stale ground (see `roll_up_check`).
        let test_verdicts: Vec<&Verdict> = verdicts
            .iter()
            .filter(|(g, _)| matches!(g.check, Some(Check::Test { .. })))
            .map(|(_, v)| v)
            .collect();
        if let Some((label, masked_stale)) = roll_up_check(&test_verdicts) {
            // suppressed_from: the worst pre-remap verdict LOCK 1/3 suppressed to memo — computed only
            // when a suppression happened. Additive — `label` (the verdict field) stays "memo" unchanged.
            let suppressed_from = if suppressed.is_empty() {
                None
            } else {
                roll_up_check(&suppressed.iter().collect::<Vec<_>>()).map(|(l, _)| l)
            };
            crate::events::append(
                &store,
                "check",
                Some(t),
                Some(&label),
                masked_stale.as_deref(),
                suppressed_from.as_deref(),
            );
        }
        // The per-host verdict-cache read contract for this tick (a hook reads it without shelling check).
        let _ = crate::state::write_state(
            &store,
            &t.id,
            &verdicts,
            &config.staleness_ref,
            ctx.live_origin_sha.as_deref(),
        );
    }

    let harvested_note = format!(
        "harvested-unproven: {harvested_unproven} of {total_test_bindings} test bindings have no counter-test"
    );
    // The --run nudge adapts to what --run would actually do for THIS set: with a declared counter-test
    // it proves falsifiability; with all bindings harvested it only re-runs the check + records a fresh
    // receipt (promising "execute each counter-test" when there is none would be an empty promise).
    let run_note = if total_test_bindings > harvested_unproven {
        "note: run `ev check --run` to re-run each bound check and prove each declared counter-test's falsifiability"
    } else {
        "note: run `ev check --run` to re-run each bound check and record a fresh receipt"
    };
    if rows.is_empty() {
        println!("no test-bound grounds to check");
    } else if painter.rich {
        // The unified grammar: verdict glyph + the verdict word as the only coloured token, the
        // decision name bold (the headline), the full 12-hex id weighted, the claim + detail dim.
        for r in &rows {
            // A suppressed-red memo names WHY (detect-only / agent-proposed) instead of a mute receipt
            // time — never a false-green about a fact the gate is ignoring. (Rich only; plain unchanged.)
            let detail = r.memo_reason.as_deref().unwrap_or(&r.detail);
            println!(
                "{} {}  {}  {}  {}",
                painter.verdict_glyph(r.class),
                painter.class(&r.label, r.class),
                painter.name(&format!("{:?}", r.decision)),
                painter.id(&r.id),
                painter.meta(&format!("{:?} · {}", r.claim, detail)),
            );
        }
        // A plain COUNT (never a score): gating / green / non-gating, by meaning-class.
        let n = |c: Class| rows.iter().filter(|r| r.class == c).count();
        println!(
            "{}",
            painter.meta(&format!(
                "{} gating · {} green · {} non-gating",
                n(Class::Attention),
                n(Class::Clear),
                n(Class::Informational)
            ))
        );
        // Honest debt stays at attention weight — never rendered quieter than a green (no false-green).
        if harvested_unproven > 0 {
            println!("{}", painter.class(&harvested_note, Class::Attention));
        }
        // Signpost the resurface zoom when something gates (a red invites a re-decide, not a klaxon).
        if n(Class::Attention) > 0 {
            println!(
                "{}",
                painter.meta("tip: ev why <selector> · ev reopen <id> — resurface a red")
            );
        }
        if !run {
            println!("{}", painter.meta(run_note));
        }
    } else {
        // Legacy: today's exact bytes (a pipe / redirect / NO_COLOR / --plain / CI never drifts).
        for r in &rows {
            println!("{}\t{}\t{:?}\t({})", r.label, r.id, r.claim, r.detail);
        }
        if harvested_unproven > 0 {
            println!("{harvested_note}");
        }
        if !run {
            println!("{run_note}");
        }
    }
    // The check-run COST summary (wall-time + how many decisions were evaluated), distinct from the
    // per-tick `check` catch events above — together the harness gets the gate's net-effect.
    crate::events::append_cost(
        &store,
        "check-run",
        &[
            ("wall_ms", (started.elapsed().as_millis() as u64).into()),
            ("decisions", (current.len() as u64).into()),
        ],
    );
    if exit_on_red && any_not_green {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// The parsed `ev migrate` invocation (built in main.rs from the clap subcommand).
pub struct MigrateArgs {
    pub sources: Vec<String>,
    pub dry_run: bool,
    pub reconcile: bool,
    pub against: Option<String>,
    pub blame: Option<String>,
    pub bind_check: Option<String>,
    pub platforms: Vec<String>,
    pub triggered_by: Vec<String>,
    pub surfaces: Vec<String>,
    pub verified_at_sha: Option<String>,
    pub jurisdiction_map: Option<String>,
}

/// Read a `--jurisdiction-map` file into a `source_key -> bucket` map. Each non-blank, non-`#` line is
/// exactly two whitespace-separated tokens `<source_key> <bucket>`; every bucket is validated against
/// the closed A/B/C/D vocabulary so an out-of-vocab bucket (or a malformed line) is a hard error that
/// names the offending line. jurisdiction is non-hashed, so the map only adds a detect-only tag — it
/// never moves a tick id. An absent path yields an empty map (every record imports untagged).
fn parse_jurisdiction_map(path: &str) -> Result<std::collections::HashMap<String, String>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("reading {path}: {e}"))?;
    let mut map = std::collections::HashMap::new();
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        let mut tokens = l.split_whitespace();
        match (tokens.next(), tokens.next(), tokens.next()) {
            (Some(key), Some(bucket), None) => {
                crate::tick::validate_jurisdiction(bucket)
                    .map_err(|e| format!("jurisdiction-map line {l:?}: {e}"))?;
                map.insert(key.to_string(), bucket.to_string());
            }
            _ => {
                return Err(format!(
                    "jurisdiction-map line {l:?}: expected `<source_key> <bucket>`"
                ))
            }
        }
    }
    Ok(map)
}

/// Read a `<kind>:<path>` source spec, dispatch to the matching pure extractor, and return the
/// extracted records. The kind names the substrate format; the path is read from disk here (the
/// extractors themselves stay pure `&str -> Vec<MigrationRecord>`).
fn extract_source(spec: &str) -> Result<Vec<crate::migrate::MigrationRecord>, String> {
    let (kind, path) = spec
        .split_once(':')
        .ok_or_else(|| format!("--source expects <kind>:<path>, got {spec:?}"))?;
    let text = std::fs::read_to_string(path).map_err(|e| format!("reading {path}: {e}"))?;
    let recs = match kind {
        // The format-neutral primary intake: a producer-owned adapter (or a live runner) emits the
        // Canonical Decision Intake JSONL, re-validated through ev's read-path validators on the way in.
        "canonical" => crate::migrate::canonical_records(&text)?,
        "gitlog" => crate::migrate::extract_gitlog(&text),
        "to-human" => crate::migrate::extract_to_human(&text),
        "decisions-immutable" => crate::migrate::extract_decisions_immutable(&text),
        "escalation" => crate::migrate::extract_escalation(&text),
        other => {
            return Err(format!(
                "unknown source kind {other:?} (expected canonical | gitlog | to-human | decisions-immutable | escalation)"
            ))
        }
    };
    Ok(recs)
}

pub fn migrate(repo: &Path, a: MigrateArgs) -> ExitCode {
    // --bind-check: harvest one existing test as a (counter-test-less) bound check and print it.
    if let Some(selector) = &a.bind_check {
        let sha = match crate::capture::resolve_sha(repo, &a.verified_at_sha) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        };
        match crate::migrate::bind_check(
            selector.clone(),
            sha,
            a.platforms.clone(),
            a.triggered_by.clone(),
            a.surfaces.clone(),
        ) {
            Ok(Check::Test {
                reference,
                liveness,
                ..
            }) => {
                println!(
                    "harvested check (falsifiability not proven; no counter-test): {reference:?} on [{}] triggered-by [{}] surface [{}]",
                    liveness.platforms.join(", "),
                    liveness.triggered_by.join(", "),
                    liveness.surfaces.join(", ")
                );
                return ExitCode::SUCCESS;
            }
            Ok(_) => unreachable!("bind_check yields a Test check"),
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        }
    }

    // --reconcile --against <src>: join the source against the store and report the buckets.
    if a.reconcile {
        let against = match &a.against {
            Some(s) => s,
            None => {
                eprintln!("error: --reconcile requires --against <kind>:<path>");
                return ExitCode::FAILURE;
            }
        };
        let recs = match extract_source(against) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        };
        match crate::migrate::reconcile(repo, &recs) {
            Ok(rep) => {
                println!(
                    "reconcile: in-both {}, source-only {} (the capture gap), store-only {}, un-keyable {}",
                    rep.in_both, rep.source_only, rep.store_only, rep.un_keyable
                );
                return ExitCode::SUCCESS;
            }
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        }
    }

    // The default action: backfill every --source into the ledger (idempotent).
    if a.sources.is_empty() {
        eprintln!("error: ev migrate needs at least one --source <kind>:<path> (or --reconcile / --bind-check)");
        return ExitCode::FAILURE;
    }
    let mut records = Vec::new();
    for spec in &a.sources {
        match extract_source(spec) {
            Ok(mut r) => records.append(&mut r),
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        }
    }
    // An omitted --jurisdiction-map ⇒ an empty map ⇒ every record imports untagged (prior behavior).
    let jurisdiction_map = match &a.jurisdiction_map {
        Some(path) => match parse_jurisdiction_map(path) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        },
        None => std::collections::HashMap::new(),
    };
    match crate::migrate::backfill(
        repo,
        records,
        a.blame.as_deref(),
        &jurisdiction_map,
        a.dry_run,
    ) {
        Ok(s) => {
            if !a.dry_run {
                crate::events::append(&Store::at(repo), "migrate", None, None, None, None);
            }
            println!(
                "{}imported {}, skipped {}, re-linked {}, {} source-only gap(s){}",
                if a.dry_run { "(dry-run) " } else { "" },
                s.imported,
                s.skipped,
                s.relinked,
                s.source_only_gaps,
                if s.discrepancies > 0 {
                    format!(", {} discrepancy(ies) — see above", s.discrepancies)
                } else {
                    String::new()
                }
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

pub fn why(repo: &Path, selector: &str) -> ExitCode {
    let store = Store::at(repo);
    if !store.exists() {
        eprintln!("error: no .evolving/ store here — run `ev init` first");
        return ExitCode::FAILURE;
    }
    let files = match store.read_all() {
        Ok(f) => f,
        Err(e) => {
            eprintln!("error: reading store: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut found = false;
    for (filename, raw) in &files {
        let t = match crate::tick::from_value(raw) {
            Ok(t) => t,
            Err(_) => continue,
        };
        if t.status != "live" {
            continue;
        }
        for g in &t.grounds {
            if let Some(Check::Test { reference, .. }) = &g.check {
                if reference.as_str() == selector {
                    found = true;
                    println!(
                        "{filename}\t{:?}\tguards: {:?} ({})",
                        t.decision, g.claim, g.supports
                    );
                }
            }
        }
    }
    if !found {
        eprintln!("{selector:?} guards nothing");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// List every decision in the ledger: id, status, decision (sorted by id, deterministic).
pub fn list(repo: &Path, painter: crate::render::Painter) -> ExitCode {
    let store = Store::at(repo);
    if !store.exists() {
        eprintln!("error: no .evolving/ store here — run `ev init` first");
        return ExitCode::FAILURE;
    }
    let files = match store.read_all() {
        Ok(f) => f,
        Err(e) => {
            eprintln!("error: reading store: {e}");
            return ExitCode::FAILURE;
        }
    };
    // Parse once. Collapse each corrective lineage to its current state (an `ev correct` child
    // supersedes the stale tick it re-tags); unparseable ticks are always shown (verify flags them)
    // since they have no decision identity to supersede.
    let mut parsed: Vec<(String, Tick)> = Vec::new();
    let mut unparseable: Vec<String> = Vec::new();
    for (name, raw) in &files {
        match crate::tick::from_value(raw) {
            Ok(t) => parsed.push((name.clone(), t)),
            Err(_) => unparseable.push(name.clone()),
        }
    }
    let current = current_decisions(parsed);
    if current.is_empty() && unparseable.is_empty() {
        println!("no decisions yet");
        return ExitCode::SUCCESS;
    }
    if painter.rich {
        // Decision-led: provenance glyph + bold name + weighted id + dim meta (status + tags), by id.
        let mut entries: Vec<(String, String)> = Vec::new();
        for name in &unparseable {
            entries.push((
                name.clone(),
                painter.meta(&format!("? {name} <unparseable>")),
            ));
        }
        for (name, t) in &current {
            let mut meta = t.status.clone();
            if let Some(a) = &t.authority {
                meta.push_str(&format!(" · authority={a}"));
            }
            if let Some(j) = &t.jurisdiction {
                meta.push_str(&format!(" · jurisdiction={j}"));
            }
            if let Some(r) = &t.source_ref {
                meta.push_str(&format!(" · source_ref={}", render_source_ref(r)));
            }
            let agent = t.provenance.as_deref() == Some("agent-proposed");
            entries.push((
                name.clone(),
                format!(
                    "{} {}  {}  {}",
                    painter.prov_glyph(agent),
                    painter.name(&format!("{:?}", t.decision)),
                    painter.id(name),
                    painter.meta(&meta)
                ),
            ));
        }
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        for (_, line) in &entries {
            println!("{line}");
        }
    } else {
        // Legacy: today's exact bytes — one tab-separated line per tick, sorted by the rendered line
        // (which begins with the id). Bookkeeping tags appended inline when present (same shape as show).
        let mut rows: Vec<String> = Vec::new();
        for name in &unparseable {
            rows.push(format!("{name}\t?\t\"<unparseable>\""));
        }
        for (name, t) in &current {
            let mut l = format!("{name}\t{}\t{:?}", t.status, t.decision);
            if let Some(a) = &t.authority {
                l.push_str(&format!("\tauthority={a}"));
            }
            if let Some(j) = &t.jurisdiction {
                l.push_str(&format!("\tjurisdiction={j}"));
            }
            if let Some(r) = &t.source_ref {
                l.push_str(&format!("\tsource_ref={}", render_source_ref(r)));
            }
            rows.push(l);
        }
        rows.sort();
        for line in &rows {
            println!("{line}");
        }
    }
    ExitCode::SUCCESS
}

/// `ev pending` — the agent proposals awaiting ratification. A PULL-only view (a query a human runs),
/// NEVER a notifier (no push, no unread, no badge). A proposal is pending iff it is still
/// `agent-proposed` in the current view: `ev ratify` mints a human-now child with the same hashed
/// payload, which collapses the proposal away (content-equality), so an un-ratified proposal is exactly
/// one that survives `current_decisions` still agent-proposed. (Sunset/aging of long-stale proposals to
/// an `--all` view is a stated future refinement; for now every un-ratified proposal is shown.)
pub fn pending(repo: &Path, source_ref: Option<&str>, painter: crate::render::Painter) -> ExitCode {
    let store = Store::at(repo);
    if !store.exists() {
        eprintln!("error: no .evolving/ store here — run `ev init` first");
        return ExitCode::FAILURE;
    }
    let files = match store.read_all() {
        Ok(f) => f,
        Err(e) => {
            eprintln!("error: reading store: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut pend: Vec<(String, Tick)> = parse_current(&files)
        .into_iter()
        .filter(|(_, t)| t.status == "live" && t.provenance.as_deref() == Some("agent-proposed"))
        .collect();
    // --source-ref narrows a piling queue to one round (matched on the same dedup key propose uses).
    if let Some(sr) = source_ref {
        pend.retain(|(_, t)| crate::tick::source_ref_matches(t, sr));
    }
    pend.sort_by(|a, b| b.1.held_since.cmp(&a.1.held_since).then(b.0.cmp(&a.0))); // newest first
    if pend.is_empty() {
        match source_ref {
            Some(sr) => println!("no proposals awaiting ratification for source_ref {sr}"),
            None => println!("no proposals awaiting ratification"),
        }
        return ExitCode::SUCCESS;
    }
    if painter.rich {
        // All rows are agent-proposed → the hollow ○ provenance glyph; the footer signposts the bridge.
        for (id, t) in &pend {
            // Triage context: blame · source_ref · held_since — so a piling queue is scannable at a
            // glance (the dogfood friction was a pending view that hid these). Rich only; plain unchanged.
            let sr = t
                .source_ref
                .as_ref()
                .map(|s| format!(" · {}", render_source_ref(s)))
                .unwrap_or_default();
            println!(
                "{} {}  {}  {}",
                painter.prov_glyph(true),
                painter.name(&format!("{:?}", t.decision)),
                painter.id(id),
                painter.meta(&format!("{}{} · {}", t.blame, sr, t.held_since))
            );
        }
        println!(
            "{}",
            painter.meta(&format!(
                "{} awaiting ratification — ev ratify <id> --blame <you>",
                pend.len()
            ))
        );
    } else {
        for (id, t) in &pend {
            println!("{id}\t{}\t{:?}\t{}", t.status, t.decision, t.blame);
        }
    }
    ExitCode::SUCCESS
}

/// A decision is "load-bearing" iff any of its grounds closes a road (`supports` starts with
/// `"rejected:"`). Those are the rulings a fresh agent must not re-walk, so they pin above the cap.
/// Detectable straight from the tick — 0-network, no receipts, no git.
fn load_bearing(t: &Tick) -> bool {
    rejected_roads(t).next().is_some()
}

/// The roads a decision closed — its `rejected:` grounds as `(option, why)` pairs. THE single place
/// the `rejected:` ground convention is read: `load_bearing`, `brief_json`, and both brief text forms
/// go through it, so the load-bearing contract lives in exactly one spot.
fn rejected_roads(t: &Tick) -> impl Iterator<Item = (&str, &str)> {
    t.grounds.iter().filter_map(|g| {
        g.supports
            .strip_prefix("rejected:")
            .map(|opt| (opt, g.claim.as_str()))
    })
}

/// Boot-read: the live user-ruled decisions and the roads they rejected. A near-zero-cost,
/// 0-network read (read_all only; no git, no receipts) for a fresh agent to load the
/// decisions it must respect and the options it must not re-propose. Load-bearing rulings
/// (those that closed a road) sort FIRST — pinned above the cap regardless of recency — then
/// by recency (held_since), then id. Capped to the effective limit, with a remainder footer
/// that counts how many hidden rulings closed a road so the elision stays visible.
/// The boot-read visibility gate, shared by the text and `--json` forms: a decision reaches `brief`
/// only when it is live, user-ruled, and NOT agent-proposed. The provenance exclusion is the §五
/// guarantee — an agent-proposed proposal never governs a fresh agent, even before the pending-lane
/// machinery lands; until a named human vouches for it, it stays out of the boot-read entirely.
fn brief_visible(t: &Tick) -> bool {
    t.status == "live"
        && t.authority.as_deref() == Some("user-ruled")
        && t.provenance.as_deref() != Some("agent-proposed")
}

/// The boot-read as one line of the frozen `ev-brief` JSON contract a consumer (e.g. the agent-runner
/// enricher) parses. Every entry is a live, user-ruled, non-agent-proposed ruling carrying its citable
/// id; the counts make any elision visible so the consumer can re-pull with a higher limit rather than
/// silently miss a pinned ruling.
fn brief_json(kept: &[(String, Tick)], total: usize, dropped_lb: usize) -> String {
    let decisions: Vec<Value> = kept
        .iter()
        .map(|(_, t)| {
            let roads: Vec<Value> = rejected_roads(t)
                .map(|(option, claim)| json!({ "option": option, "claim": claim }))
                .collect();
            let mut d = json!({
                "id": t.id,
                "decision": t.decision,
                "load_bearing": load_bearing(t),
                "rejected_roads": roads,
            });
            // source_ref is genuinely optional — present only when the producer supplied one.
            if let (Some(sr), Some(obj)) = (&t.source_ref, d.as_object_mut()) {
                obj.insert(
                    "source_ref".into(),
                    Value::String(crate::tick::source_ref_key(sr)),
                );
            }
            d
        })
        .collect();
    let payload = json!({
        "kind": "ev-brief",
        "decisions": decisions,
        "shown": kept.len(),
        "total": total,
        "elided": total - kept.len(),
        "elided_load_bearing": dropped_lb,
    });
    // A Value built by json! is infallible to serialize; .expect documents that invariant rather
    // than masking a failure into an empty string — which would be a false-green: a consumer parsing
    // the contract would read silence as a clean, empty boot-read. (Unlike the droppable events log,
    // this is stdout a consumer parses, so it must never be silently blank.)
    format!(
        "{}\n",
        serde_json::to_string(&payload).expect("ev-brief payload serializes")
    )
}

pub fn brief(
    repo: &Path,
    limit: Option<usize>,
    json: bool,
    painter: crate::render::Painter,
) -> ExitCode {
    let store = Store::at(repo);
    if !store.exists() {
        eprintln!("error: no .evolving/ store here — run `ev init` first");
        return ExitCode::FAILURE;
    }
    let files = match store.read_all() {
        Ok(f) => f,
        Err(e) => {
            eprintln!("error: reading store: {e}");
            return ExitCode::FAILURE;
        }
    };
    // The flag overrides config; 0 (here or in config) means "show all".
    let limit = limit.unwrap_or(crate::config::read(&store).brief_limit);
    // parse_current collapses each corrective lineage to its current state BEFORE filtering, so an
    // `ev correct` that (de)promotes authority is honored — then keep only the brief-visible ones.
    let mut kept: Vec<(String, Tick)> = parse_current(&files)
        .into_iter()
        .filter(|(_, t)| brief_visible(t))
        .collect();
    let lb = load_bearing;
    // Load-bearing first (true > false, so descending pins them), then most-recent-first by
    // held_since, then id descending — all deterministic.
    kept.sort_by(|a, b| {
        lb(&b.1)
            .cmp(&lb(&a.1))
            .then(b.1.held_since.cmp(&a.1.held_since))
            .then(b.0.cmp(&a.0))
    });
    let total = kept.len();
    // 0 means "show all"; otherwise cap at the limit (never past the end).
    let n = if limit == 0 { total } else { limit.min(total) };
    // Count load-bearing rulings about to be elided, before we truncate the shown set.
    let dropped_lb = kept[n..].iter().filter(|(_, t)| lb(t)).count();
    kept.truncate(n);

    // The agent boot-read injection: record its COST so the validation harness can weigh it against
    // the per-decision catch (the net-effect denominator). Emitted on EVERY brief — not just --json —
    // and DECOMPOSED: `testbindable` is the catch-eligible subset (a non-test-bindable ruling can never
    // produce a catch, so it must not be charged as if it could); `total`/`elided` give injection
    // coverage past --limit. Additive, best-effort, gitignored 埋点 — no hashed payload is touched.
    let out = brief_json(&kept, total, dropped_lb);
    let testbindable = kept
        .iter()
        .filter(|(_, t)| crate::tick::has_test_check(&t.grounds))
        .count();
    crate::events::append_cost(
        &store,
        "brief",
        &[
            ("decisions", (kept.len() as u64).into()),
            ("brief_bytes", (out.len() as u64).into()),
            ("testbindable", (testbindable as u64).into()),
            ("total", (total as u64).into()),
            ("elided", ((total - kept.len()) as u64).into()),
        ],
    );

    // --json always emits one valid object (even when empty) — a parsing consumer never sees prose.
    if json {
        print!("{out}");
        return ExitCode::SUCCESS;
    }
    if kept.is_empty() {
        println!("no user-ruled decisions");
        return ExitCode::SUCCESS;
    }
    let footer = |dropped: usize| {
        let lb_clause = if dropped_lb > 0 {
            format!(", {dropped_lb} with rejected roads")
        } else {
            String::new()
        };
        format!("… {dropped} more user-ruled decision(s){lb_clause} — `ev list` for all")
    };
    if painter.rich {
        // Masthead: the count of settled rulings (the 事前 boot-read — "what's settled, don't re-walk").
        // All brief entries are user-ruled + non-agent-proposed by brief_visible, so all solid ●.
        println!(
            "{} {}",
            painter.prov_glyph(false),
            painter.name(&format!("{total} user-ruled decisions"))
        );
        for (id, t) in &kept {
            println!(
                "{} {}  {}",
                painter.prov_glyph(false),
                painter.name(&format!("{:?}", t.decision)),
                painter.id(id)
            );
            for (option, claim) in rejected_roads(t) {
                println!(
                    "    {}",
                    painter.meta(&format!("rejected {option}: {claim}"))
                );
            }
        }
        if total > n {
            println!("{}", painter.meta(&footer(total - n)));
        }
    } else {
        // Legacy: today's exact bytes.
        for (_id, t) in &kept {
            println!("{}  [user-ruled]", t.decision);
            for (option, claim) in rejected_roads(t) {
                println!("  rejected {option}: {claim}");
            }
        }
        if total > n {
            println!("{}", footer(total - n));
        }
    }
    ExitCode::SUCCESS
}

/// Show the decision lineage from HEAD back to genesis (newest first).
pub fn log(repo: &Path, painter: crate::render::Painter) -> ExitCode {
    let store = Store::at(repo);
    if !store.exists() {
        eprintln!("error: no .evolving/ store here — run `ev init` first");
        return ExitCode::FAILURE;
    }
    let mut id = match store.read_head() {
        Ok(h) => h,
        Err(e) => {
            eprintln!("error: reading HEAD: {e}");
            return ExitCode::FAILURE;
        }
    };
    if id.is_empty() {
        println!("no decisions yet");
        return ExitCode::SUCCESS;
    }
    let mut seen = std::collections::HashSet::new();
    while !id.is_empty() {
        if !seen.insert(id.clone()) {
            break; // cycle guard (a content-addressed chain can't cycle, but never loop)
        }
        match store.read_tick(&id) {
            Ok(Some(t)) => {
                if painter.rich {
                    // Chronological lineage: the edge-verb sits in a fixed dim slot — the ledger's
                    // grammar made visible. (ratifies <id> joins once Cut D lands.)
                    let verb = match (&t.corrects, &t.ratifies, t.provenance.as_deref()) {
                        (Some(c), _, _) => format!("corrects {c}"),
                        (_, Some(r), _) => format!("ratifies {r}"),
                        (None, None, Some("agent-proposed")) => "proposed".to_string(),
                        (None, None, _) => "decided".to_string(),
                    };
                    let agent = t.provenance.as_deref() == Some("agent-proposed");
                    println!(
                        "{} {}  {}  {}  {}",
                        painter.prov_glyph(agent),
                        painter.meta(&verb),
                        painter.name(&format!("{:?}", t.decision)),
                        painter.id(&t.id),
                        painter.meta(&t.blame)
                    );
                } else {
                    println!("{}\t{}\t{:?}", t.id, t.status, t.decision);
                }
                id = t.parent_id;
            }
            Ok(None) => {
                eprintln!("warning: {id} not found (broken lineage)");
                break;
            }
            Err(e) => {
                eprintln!("error: reading {id}: {e}");
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}

pub fn reopen(repo: &Path, id: &str, painter: crate::render::Painter) -> ExitCode {
    use crate::render::{class_of, Class};
    let store = Store::at(repo);
    let tick = match store.read_tick(id) {
        Ok(Some(t)) => t,
        Ok(None) => {
            eprintln!("error: no tick with id {id}");
            return ExitCode::FAILURE;
        }
        Err(e) => {
            eprintln!("error: reading {id}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let config = crate::config::read(&store);
    let live_origin = crate::staleness::resolve(repo, &store, &config.staleness_ref, true);
    let ctx = live_ctx(&store, config.staleness_days, live_origin, None);

    crate::events::append(&store, "reopen", Some(&tick), None, None, None);
    // The decision headline — provenance is a DECISION-level mark, shown ONCE here (per-ground rows
    // below carry only their own verdict, never a provenance glyph — the two axes never blur).
    let agent = tick.provenance.as_deref() == Some("agent-proposed");
    if painter.rich {
        println!(
            "{} {}  {}",
            painter.prov_glyph(agent),
            painter.name(&format!("{:?}", tick.decision)),
            painter.id(&tick.id)
        );
        if !tick.observe.is_empty() {
            println!(
                "  {}",
                painter.meta(&format!("observe: {:?}", tick.observe))
            );
        }
        let mut tags = Vec::new();
        if let Some(a) = &tick.authority {
            tags.push(format!("authority={a}"));
        }
        if let Some(j) = &tick.jurisdiction {
            tags.push(format!("jurisdiction={j}"));
        }
        if let Some(r) = &tick.source_ref {
            tags.push(format!("source_ref={}", render_source_ref(r)));
        }
        if let Some(c) = &tick.corrects {
            tags.push(format!("corrects {c}"));
        }
        if let Some(rt) = &tick.ratifies {
            tags.push(format!("ratifies {rt}"));
        }
        if !tags.is_empty() {
            println!("  {}", painter.meta(&tags.join(" · ")));
        }
    } else {
        println!("decision {}: {:?}", tick.id, tick.decision);
        if !tick.observe.is_empty() {
            println!("observe: {:?}", tick.observe);
        }
        if let Some(a) = &tick.authority {
            println!("authority: {a}");
        }
        if let Some(j) = &tick.jurisdiction {
            println!("jurisdiction: {j}");
        }
        if let Some(r) = &tick.source_ref {
            println!("source_ref: {}", render_source_ref(r));
        }
        if let Some(c) = &tick.corrects {
            println!("corrects: {c}");
        }
        if let Some(rt) = &tick.ratifies {
            println!("ratifies: {rt}");
        }
    }
    for g in &tick.grounds {
        match &g.check {
            Some(Check::Test {
                reference,
                verified_at_sha,
                ..
            }) => {
                let receipts = crate::receipt::read_for(&store, reference).unwrap_or_default();
                let ts = triggered_since(repo, g, &receipts);
                let v = crate::verdict::verdict_for(g, &receipts, &ctx, ts);
                let now = v.label();
                let short = &verified_at_sha[..verified_at_sha.len().min(8)];
                if painter.rich {
                    let class = class_of(&v);
                    println!(
                        "  {} {}  [{}] {}  {}",
                        painter.verdict_glyph(class),
                        painter.class(now, class),
                        g.supports,
                        painter.name(&format!("{:?}", g.claim)),
                        painter.meta(&format!("test {reference:?} frozen@{short}"))
                    );
                } else {
                    println!(
                        "  [{}] {:?} — test {:?} frozen@{short} now: {now}",
                        g.supports, g.claim, reference
                    );
                }
            }
            Some(Check::Person { reference }) => {
                if painter.rich {
                    println!(
                        "  {} [{}] {}  {}",
                        painter.verdict_glyph(Class::Informational),
                        g.supports,
                        painter.name(&format!("{:?}", g.claim)),
                        painter.meta(&format!("person {reference:?}"))
                    );
                } else {
                    println!("  [{}] {:?} — person {:?}", g.supports, g.claim, reference);
                }
            }
            None => {
                if painter.rich {
                    println!(
                        "  {} [{}] {}",
                        painter.verdict_glyph(Class::Informational),
                        g.supports,
                        painter.name(&format!("{:?}", g.claim))
                    );
                } else {
                    println!("  [{}] {:?}", g.supports, g.claim);
                }
            }
        }
    }
    ExitCode::SUCCESS
}

/// Reproduce the two frozen golden vectors; non-zero if either id drifts.
fn self_test_golden() -> ExitCode {
    let genesis = Tick {
        id: String::new(),
        parent_id: "".into(),
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
        status: "live".into(),
        held_since: "".into(),
        blame: "Wang Yu".into(),
        authority: None,
        jurisdiction: None,
        source_ref: None,
        provenance: None,
        corrects: None,
        ratifies: None,
    };
    let case1 = Tick {
        id: String::new(),
        parent_id: "7b21f0a4c8de".into(),
        observe: "multi-pod restore-safety counter — chat-room R2289→R2290".into(),
        decision: "restore-safety counter DB-backed; reject Redis".into(),
        grounds: vec![
            Ground {
                claim: "Argus introduces no Redis; multi-pod coord via existing DB".into(),
                supports: "chosen".into(),
                check: Some(Check::Test {
                    reference: "pytest tests/test_redis_absent.py".into(),
                    verified_at_sha: "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901".into(),
                    counter_test: Some(
                        "pytest tests/test_redis_absent.py::test_redis_injection_flips_red".into(),
                    ),
                    liveness: Liveness {
                        platforms: vec!["linux-ci".into()],
                        triggered_by: vec!["pyproject.toml".into()],
                        surfaces: vec!["pyproject-deps".into()],
                    },
                }),
            },
            Ground {
                claim: "team still wants 0-Redis posture".into(),
                supports: "chosen".into(),
                check: Some(Check::Person {
                    reference: "Q3 infra review".into(),
                }),
            },
            Ground {
                claim: "Redis would add a new infra dependency".into(),
                supports: "rejected:Redis".into(),
                check: None,
            },
        ],
        status: "live".into(),
        held_since: "".into(),
        blame: "Wang Yu".into(),
        authority: None,
        jurisdiction: None,
        source_ref: None,
        provenance: None,
        corrects: None,
        ratifies: None,
    };
    // A harvested binding: case1's first ground with counter_test omitted (None). Pins that
    // omit-on-None keeps every harvested id byte-stable — moving it would mean the payload changed.
    let mut harvested = case1.clone();
    if let Some(Check::Test { counter_test, .. }) = &mut harvested.grounds[0].check {
        *counter_test = None;
    }
    // A rejected-road tripwire (0.1.8): case1's rejected:Redis road now CARRYING a Check::Test — the
    // re-walk guard a user-ruled decision may bind to the road it closed. Pins the byte layout of a
    // rejected: ground WITH a check (a layout no other golden exercises). authority=user-ruled is
    // non-hashed (it never moves the id); the new check on grounds[2] is what makes this a fresh id.
    let mut rejected_tripwire = case1.clone();
    rejected_tripwire.authority = Some("user-ruled".into());
    rejected_tripwire.grounds[2].check = Some(Check::Test {
        reference: "! grep -q redis pyproject.toml".into(),
        verified_at_sha: "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901".into(),
        counter_test: Some("grep -q redis pyproject.toml".into()),
        liveness: Liveness {
            platforms: vec!["linux-ci".into()],
            triggered_by: vec!["pyproject.toml".into()],
            surfaces: vec!["pyproject-deps".into()],
        },
    });
    let mut ok = true;
    for (name, t, want) in [
        ("genesis", &genesis, "e2b337f53a1f"),
        ("case1", &case1, "638c47b0c9dd"),
        ("harvested", &harvested, "0cf784b51331"),
        ("rejected_tripwire", &rejected_tripwire, "9c5feb4582ac"),
    ] {
        let got = compute_id(t);
        let pass = got == want;
        ok &= pass;
        println!(
            "{} {name}: {got} (want {want})",
            if pass { "✓" } else { "✗" }
        );
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

#[cfg(test)]
mod tests {
    use super::roll_up_check;
    use crate::verdict::{StaleKind, Verdict};

    fn stale_sha() -> Verdict {
        Verdict::Stale {
            kind: StaleKind::Sha,
            reason: String::new(),
        }
    }

    #[test]
    fn roll_up_check_should_emit_nothing_when_there_is_no_test_bound_ground() {
        // given: a tick with no test-bound verdicts -> no check event
        assert_eq!(roll_up_check(&[]), None);
    }

    #[test]
    fn roll_up_check_should_carry_the_worst_verdict_red_over_green() {
        // given: a green ground and a red ground -> the event carries red, the catch stays visible
        let (g, r) = (Verdict::Green, Verdict::Red);
        assert_eq!(roll_up_check(&[&g, &r]), Some(("red".to_string(), None)));
    }

    #[test]
    fn roll_up_check_should_let_a_gating_silently_unbound_outrank_a_co_occurring_green() {
        // given: a silently-unbound (gating mask-bypass) + a green ground -> su must win either order,
        // so a co-occurring green never erases the gating fact from the log
        let (su, g) = (Verdict::SilentlyUnbound, Verdict::Green);
        assert_eq!(
            roll_up_check(&[&su, &g]),
            Some(("silently-unbound".to_string(), None))
        );
        assert_eq!(
            roll_up_check(&[&g, &su]),
            Some(("silently-unbound".to_string(), None))
        );
    }

    #[test]
    fn roll_up_check_should_carry_the_stale_sub_kind_when_stale_is_the_worst() {
        // given: a sha-stale ground alongside a not-run -> the verdict IS the stale sub-kind (visible)
        let (s, nr) = (
            stale_sha(),
            Verdict::NotRun {
                missing_platforms: vec!["p".to_string()],
            },
        );
        assert_eq!(
            roll_up_check(&[&s, &nr]),
            Some(("stale:sha".to_string(), None))
        );
    }

    #[test]
    fn roll_up_check_should_surface_a_stale_masked_behind_a_red() {
        // given: a red ground hiding a sha-stale ground -> the event carries red AND the masked stale,
        // so a drifted/disabled staleness_ref masking a real red never silently drops
        let (r, s) = (Verdict::Red, stale_sha());
        assert_eq!(
            roll_up_check(&[&r, &s]),
            Some(("red".to_string(), Some("stale:sha".to_string())))
        );
    }

    #[test]
    fn roll_up_check_should_emit_green_when_every_ground_is_green() {
        let (a, b) = (Verdict::Green, Verdict::Green);
        assert_eq!(roll_up_check(&[&a, &b]), Some(("green".to_string(), None)));
    }
}
