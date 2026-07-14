use crate::ledger::{Actor, ActorKind, Envelope, Ledger, NewEvent};
use crate::{EvError, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Walk up from cwd to find an existing `.evolving/` root; else return cwd.
pub fn find_root() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut cur = cwd.as_path();
    loop {
        if cur.join(".evolving").is_dir() {
            return cur.to_path_buf();
        }
        match cur.parent() {
            Some(p) => cur = p,
            None => return cwd,
        }
    }
}

pub fn init() -> Result<()> {
    let root = std::env::current_dir()?;
    let ev = root.join(".evolving");
    fs::create_dir_all(ev.join("ledger"))?;
    fs::create_dir_all(ev.join("artifacts"))?;
    fs::create_dir_all(ev.join("local"))?;
    write_if_absent(&ev.join("version"), "2\n")?;
    write_if_absent(
        &ev.join("config.toml"),
        "# ev preferences (non-historical)\n",
    )?;
    write_if_absent(&ev.join(".gitignore"), "local/\ncache/\n")?;
    ensure_line(
        &root.join(".gitattributes"),
        ".evolving/ledger/*.jsonl merge=union",
    )?;
    register_repo(&root)?;
    // touch the writer id so the ledger is usable immediately
    let ledger = crate::ledger::Ledger::open(&root)?;
    // The baseline: where this ledger began. Without it, the first sweep would
    // file every pre-existing commit as this session's output — a false fact.
    // init is idempotent (write_if_absent, ensure_line above): a re-run must
    // not append a second baseline, or the watermark jumps forward and the
    // commits between the two runs are never filed by any future sweep.
    if !crate::state::has_baseline(&ledger.scan()?) {
        write_baseline(&ledger, &root)?;
    }
    println!("initialized .evolving/ at {}", root.display());
    println!("ev refreshes when invoked, not in the background.");
    Ok(())
}

/// Write the baseline marker for an already-resolved sha — the one construction
/// of the marker, so every path that records a beginning writes the same shape.
fn write_baseline_at(ledger: &Ledger, head: &str) -> Result<()> {
    ledger.append_batch(vec![NewEvent {
        etype: "session".into(),
        actor: Actor::engine(),
        body: serde_json::json!({ "marker": "baseline", "head": head }),
    }])?;
    Ok(())
}

/// Record where this ledger began: the current HEAD, or the honest literal
/// "ROOT" when the repo carries no commits yet.
fn write_baseline(ledger: &Ledger, root: &Path) -> Result<String> {
    let head = crate::git_output(root, &["rev-parse", "HEAD"]).unwrap_or_else(|| "ROOT".into());
    write_baseline_at(ledger, &head)?;
    Ok(head)
}

/// Record a baseline for a ledger that predates it (0.2.1 and earlier), or
/// re-pin it. Append-only: an earlier baseline is never rewritten.
pub fn baseline(sha: Option<String>) -> Result<()> {
    let root = find_root();
    let ledger = Ledger::open(&root)?;
    // the sha the marker actually carries — the message must state that, not HEAD
    let recorded = match sha {
        None => write_baseline(&ledger, &root)?,
        Some(s) => {
            let resolved =
                crate::git_output(&root, &["rev-parse", "--verify", &s]).ok_or_else(|| {
                    EvError::Refusal(format!("{s} does not resolve to a commit in this repo"))
                })?;
            write_baseline_at(&ledger, &resolved)?;
            resolved
        }
    };
    println!(
        "baseline recorded; exhaust windows start after {}",
        short_sha(&recorded)
    );
    Ok(())
}

fn short_sha(s: &str) -> String {
    s.chars().take(8).collect()
}

fn write_if_absent(path: &Path, contents: &str) -> Result<()> {
    if !path.exists() {
        fs::write(path, contents)?;
    }
    Ok(())
}

fn ensure_line(path: &Path, line: &str) -> Result<()> {
    let existing = fs::read_to_string(path).unwrap_or_default();
    if existing.lines().any(|l| l.trim() == line) {
        return Ok(());
    }
    let mut next = existing;
    if !next.is_empty() && !next.ends_with('\n') {
        next.push('\n');
    }
    next.push_str(line);
    next.push('\n');
    fs::write(path, next).map_err(EvError::from)
}

fn register_repo(root: &Path) -> Result<()> {
    let home = std::env::var("HOME").map_err(|_| EvError::Failure("HOME unset".into()))?;
    let cfg = PathBuf::from(home).join(".config/evolving");
    fs::create_dir_all(&cfg)?;
    let canon = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    ensure_line(&cfg.join("repos"), &canon.to_string_lossy())
}

// ── write-path verbs ──────────────────────────────────────────────────────────

pub struct ClaimArgs {
    pub label: String,
    pub evidence: Option<String>,
    pub by_agent: bool,
    pub source_ref: Option<String>,
    pub kind: Option<String>,
}

pub fn claim(args: ClaimArgs) -> Result<()> {
    let root = find_root();
    let ledger = Ledger::open(&root)?;

    // idempotency: if a claim with this source_ref already exists, do nothing.
    if let Some(sref) = &args.source_ref {
        let events = ledger.scan()?;
        let exists = events.iter().any(|e| {
            e.etype == "claim"
                && e.body.get("source_ref").and_then(|s| s.as_str()) == Some(sref.as_str())
        });
        if exists {
            println!("claim already filed for source_ref {sref} (idempotent).");
            return Ok(());
        }
    }

    let actor = if args.by_agent {
        Actor {
            kind: ActorKind::Agent,
            id: agent_id(),
            via: None,
        }
    } else {
        Actor::human()
    };
    let mut body = serde_json::json!({ "label": args.label });
    if let Some(sref) = &args.source_ref {
        body["source_ref"] = serde_json::json!(sref);
    }
    if let Some(kind) = &args.kind {
        body["kind"] = serde_json::json!(kind);
    }
    let batch = vec![NewEvent {
        etype: "claim".into(),
        actor: actor.clone(),
        body,
    }];

    // The attach guard runs BEFORE the claim is written, and ONCE. The guard can refuse an
    // inline --evidence ref (a line number, an unknown scheme); the claim's batch is a
    // separate atomic write, so refusing after it would leave a bare claim behind on every
    // attempt — a refused ref must cost the ledger nothing. The guarded ref is carried to
    // `record_checked` below rather than re-guarded, which would re-read and re-parse the
    // cited file a second time for one filing.
    let guarded = match &args.evidence {
        Some(eref) => Some(crate::verify::guard_attach(eref, &root)?),
        None => None,
    };

    // an inline --evidence attaches an evidence event referencing the just-minted claim.
    // Because the batch is one atomic write, the claim is minted first, then referenced.
    let minted = ledger.append_batch(batch)?;
    if let (Some(eref), Some(r)) = (&args.evidence, &guarded) {
        let claim_id = &minted[0].id;
        let verdict =
            crate::verify::record_checked(&ledger, &root, claim_id, eref, r, false, actor)?;
        println!(
            "claim {} · evidence {} → {}",
            short(claim_id),
            eref,
            verdict.as_str()
        );
        if let Some(h) = anchor_hint(eref) {
            println!("{h}");
        }
    } else {
        println!(
            "claim {} (bare — needs evidence to close)",
            short(&minted[0].id)
        );
    }
    Ok(())
}

pub fn think(label: String, pinned: bool) -> Result<()> {
    let root = find_root();
    let ledger = Ledger::open(&root)?;
    let actor = Actor::human();
    ledger.append_batch(vec![NewEvent {
        etype: "thought".into(),
        actor,
        body: serde_json::json!({ "label": label, "pinned": pinned }),
    }])?;
    println!("noted.");
    Ok(())
}

/// Return a short display prefix for an event id: `<prefix>_<first-6-of-ulid>`.
pub fn short(id: &str) -> String {
    crate::short_id(id)
}

fn agent_id() -> Option<String> {
    if std::env::var("CLAUDECODE").is_ok() {
        Some("claude-code".into())
    } else {
        std::env::var("EV_AGENT").ok()
    }
}

// ── evidence + verify verbs ───────────────────────────────────────────────────

/// The advisory line for an anchor that can only fail on deletion. `None` for
/// every other class — a hint that fires on everything teaches nothing.
pub fn anchor_hint(eref: &str) -> Option<String> {
    let r = crate::verify::EvRef::parse(eref).ok()?;
    if crate::verify::Liveness::of(&r) != crate::verify::Liveness::Existence {
        return None;
    }
    let scheme = r.kind.scheme();
    Some(format!(
        "  ⚠ existence anchor: {}.\n    For an anchor that fails when the cited code changes: {scheme}:{}::<text>",
        crate::verify::Liveness::Existence.why(),
        r.payload
    ))
}

/// Attach a typed evidence ref to a claim. Agents are permitted.
pub fn evidence(claim_id: String, eref: String) -> Result<()> {
    let root = find_root();
    let ledger = Ledger::open(&root)?;
    let full = resolve_claim_id(&ledger, &claim_id)?;
    let actor = evidence_actor();
    let verdict = crate::verify::verify_and_record(&ledger, &root, &full, &eref, false, actor)?;
    println!(
        "evidence attached to {} → {}",
        short(&full),
        verdict.as_str()
    );
    if let Some(h) = anchor_hint(&eref) {
        println!("{h}");
    }
    Ok(())
}

/// Re-check anchors for one claim (or all open claims): resolution + drift.
pub fn verify_cmd(claim_id: Option<String>, json: bool, full: bool) -> Result<()> {
    let root = find_root();
    let ledger = Ledger::open(&root)?;
    let events = ledger.scan()?;
    let d = crate::state::fold(&events);
    let targets: Vec<&crate::state::ClaimView> = match &claim_id {
        Some(cid) => {
            let full = resolve_claim_id(&ledger, cid)?;
            d.claims.iter().filter(|c| c.id == full).collect()
        }
        None => d.claims.iter().collect(),
    };
    let mut checks: Vec<serde_json::Value> = Vec::new();
    for c in targets {
        for ev in &c.evidence {
            // Self-evident evidence is not a verification claim — ev says so itself
            // at the pause: "acknowledging records that work happened; it does not
            // verify the assertions." Replaying it every round drowns the real
            // checks in forever-green noise (Run-14: 92.7% of all output).
            if ev.self_evident && !full {
                continue;
            }
            if let Ok(r) = crate::verify::EvRef::parse(&ev.eref) {
                let status = crate::verify::verify_ref(&r, &root);
                let liveness = crate::verify::Liveness::of(&r);
                // drift: the world's movement under the anchor, in commits touching
                // the cited path — a structural fact, judged by the human. Counted
                // from the same reference `annotate` uses (the human's last
                // look, else the filing base), through the same rule: a second rule
                // here would be a second source of truth.
                let moved = crate::verify::drift_since(
                    &root,
                    c.last_ack.as_deref(),
                    ev.base.as_deref(),
                    &r,
                );

                let mut body = serde_json::json!({
                    "claim": c.id,
                    "ref": ev.eref,
                    "status": status,
                    "liveness": liveness.as_str(),
                });
                if let Some(base) = &ev.base {
                    body["base"] = serde_json::json!(base);
                }
                if let Some(k) = moved {
                    body["drift"] = serde_json::json!(k);
                }
                // The cell is derived in exactly one place — `Cell::of`. This body is
                // both the appended `verify` event and the `--json` check: one shape,
                // one source of truth.
                if let Some(cell) = crate::verify::Cell::of(status, moved) {
                    body["cell"] = serde_json::json!(cell);
                }
                ledger.append_batch(vec![NewEvent {
                    etype: "verify".into(),
                    actor: Actor::engine(),
                    body: body.clone(),
                }])?;

                if json {
                    checks.push(body);
                } else {
                    match moved {
                        Some(k) if k > 0 => println!(
                            "{} · {} → {} · {}",
                            short(&c.id),
                            ev.eref,
                            status.as_str(),
                            crate::verify::drift_phrase(k)
                        ),
                        _ => println!("{} · {} → {}", short(&c.id), ev.eref, status.as_str()),
                    }
                }
            }
        }
    }
    if json {
        let v = serde_json::json!({ "checks": checks });
        println!("{}", serde_json::to_string_pretty(&v).unwrap());
    }
    Ok(())
}

/// Resolve a unique id prefix to a full id.
fn resolve_id(ledger: &Ledger, prefix: &str) -> Result<String> {
    let events = ledger.scan()?;
    let matches: Vec<&str> = events
        .iter()
        .map(|e| e.id.as_str())
        .filter(|id| id.starts_with(prefix))
        .collect();
    match matches.len() {
        1 => Ok(matches[0].to_string()),
        0 => Err(EvError::Refusal(format!("no event matches id {prefix}"))),
        _ => Err(EvError::Refusal(format!(
            "ambiguous id {prefix} — {} matches",
            matches.len()
        ))),
    }
}

/// Resolve an id prefix against CLAIM events only. A type prefix (`thk_`, `clm_`)
/// makes an id unambiguous; it never makes it the right kind. Evidence, closure and
/// demands all attach to claims — an id of any other kind is a caller error, and
/// accepting it writes an event the fold can never reach.
fn resolve_claim_id(ledger: &Ledger, prefix: &str) -> Result<String> {
    let events = ledger.scan()?;
    let matches: Vec<&Envelope> = events.iter().filter(|e| e.id.starts_with(prefix)).collect();
    match matches.len() {
        0 => Err(EvError::Refusal(format!("no event matches id {prefix}"))),
        1 if matches[0].etype == "claim" => Ok(matches[0].id.clone()),
        1 => Err(EvError::Refusal(format!(
            "{} is a {} event, not a claim. Evidence, closure and demands attach to claims.",
            matches[0].id, matches[0].etype
        ))),
        n => Err(EvError::Refusal(format!(
            "ambiguous id {prefix} — {n} matches"
        ))),
    }
}

/// The pair, at the instant a human disposed of a claim: what each anchor read, and how
/// far the world had moved under it. Written into every disposition event so a later
/// analysis can ask whether the signal PRECEDED the decision — the first measurable
/// proxy for whether the rail earns its cost. ev emits it and never reads it.
pub fn at_verify_snapshot(root: &Path, ledger: &Ledger, claim_id: &str) -> serde_json::Value {
    let mut d = match ledger.scan() {
        Ok(events) => crate::state::fold(&events),
        Err(_) => return serde_json::json!([]),
    };
    crate::verify::annotate(&mut d, root);
    let found = d
        .claims
        .iter()
        .chain(&d.grey)
        .chain(&d.closed)
        .find(|c| c.id == claim_id);
    match found {
        None => serde_json::json!([]),
        Some(c) => serde_json::Value::Array(
            c.evidence
                .iter()
                .map(|e| {
                    let mut v = serde_json::json!({
                        "ref": e.eref,
                        "status": e.status,
                    });
                    if let Some(k) = e.drift {
                        v["drift"] = serde_json::json!(k);
                    }
                    if let Some(cell) = e.cell {
                        v["cell"] = serde_json::json!(cell);
                    }
                    v
                })
                .collect(),
        ),
    }
}

fn evidence_actor() -> Actor {
    // Evidence is creation-only; agents are permitted. Provenance is recorded.
    if std::env::var("CLAUDECODE").is_ok() {
        Actor::agent("claude-code")
    } else {
        Actor::human()
    }
}

// ── closure verbs ─────────────────────────────────────────────────────────────

/// Refuse closure verbs under CLAUDECODE unless the human override is present.
fn assert_human(i_am_the_human: bool) -> Result<()> {
    if std::env::var("CLAUDECODE").is_ok() && !i_am_the_human {
        return Err(EvError::Refusal(
            "closure is the human's move. Re-run with --i-am-the-human if that's you.".into(),
        ));
    }
    Ok(())
}

pub struct CloseArgs {
    pub claim: String,
    pub dead: bool,
    pub reason: Option<String>,
    pub i_am_the_human: bool,
}

pub fn close(args: CloseArgs) -> Result<()> {
    assert_human(args.i_am_the_human)?;
    let root = find_root();
    let ledger = Ledger::open(&root)?;
    let full = resolve_claim_id(&ledger, &args.claim)?;
    let d = crate::state::fold(&ledger.scan()?);
    let view = d
        .claims
        .iter()
        .find(|c| c.id == full)
        .ok_or_else(|| EvError::Refusal(format!("{} is not an open claim", short(&full))))?;

    if args.dead {
        let reason = args
            .reason
            .ok_or_else(|| EvError::Refusal("--dead needs --reason".into()))?;
        let snap = at_verify_snapshot(&root, &ledger, &full);
        ledger.append_batch(vec![NewEvent {
            etype: "prune".into(),
            actor: Actor::human(),
            body: serde_json::json!({ "claim": full, "reason": reason, "at_verify": snap }),
        }])?;
        println!("declared dead: {} — {reason}", short(&full));
        return Ok(());
    }

    if view.evidence.is_empty() {
        return Err(EvError::Refusal(format!(
            "{} has no evidence. A claim closes with a pointer, or it is declared dead (--dead --reason).\nClosed-anyway does not exist here.",
            short(&full)
        )));
    }
    let snap = at_verify_snapshot(&root, &ledger, &full);
    ledger.append_batch(vec![NewEvent {
        etype: "close".into(),
        actor: Actor::human(),
        body: serde_json::json!({ "claim": full, "at_verify": snap }),
    }])?;
    println!("closed {} with evidence.", short(&full));
    Ok(())
}

pub fn hold(claim: String, reason: String, i_am_the_human: bool) -> Result<()> {
    assert_human(i_am_the_human)?;
    let root = find_root();
    let ledger = Ledger::open(&root)?;
    let full = resolve_claim_id(&ledger, &claim)?;
    let snap = at_verify_snapshot(&root, &ledger, &full);
    ledger.append_batch(vec![NewEvent {
        etype: "hold".into(),
        actor: Actor::human(),
        body: serde_json::json!({ "claim": full, "reason": reason, "at_verify": snap }),
    }])?;
    println!("held (grey): {} — {reason}", short(&full));
    Ok(())
}

/// The disposition the set was missing: the human looked, and the claim still stands.
/// Records the HEAD that was looked at, so ev can report movement since the LAST LOOK
/// as well as movement since the filing. This is not a re-base: the evidence `base`
/// stays pinned forever.
///
/// When git cannot resolve HEAD (a repo with no commits yet) the `head` field is OMITTED
/// rather than filled with a placeholder. A recorded non-sha would be a reference git can
/// never resolve, so `drift_since` would return None for the claim forever and the anchor
/// would read as unmoved no matter how far the world moved. An absent `head` instead folds
/// to `last_ack = None` and drift falls back to the evidence `base`.
pub fn ack(claim: String, i_am_the_human: bool) -> Result<()> {
    assert_human(i_am_the_human)?;
    let root = find_root();
    let ledger = Ledger::open(&root)?;
    let full = resolve_claim_id(&ledger, &claim)?;
    let head = crate::git_output(&root, &["rev-parse", "HEAD"]);
    let snap = at_verify_snapshot(&root, &ledger, &full);
    let mut body = serde_json::json!({ "claim": full, "at_verify": snap });
    if let Some(h) = &head {
        body["head"] = serde_json::json!(h);
    }
    ledger.append_batch(vec![NewEvent {
        etype: "ack".into(),
        actor: Actor::human(),
        body,
    }])?;
    match &head {
        Some(h) => println!("{} acknowledged at {}", short(&full), &h[..h.len().min(8)]),
        None => println!(
            "{} acknowledged. No commit to reference yet, so drift is counted from the filing base.",
            short(&full)
        ),
    }
    Ok(())
}

pub fn demand(claim: String, i_am_the_human: bool) -> Result<()> {
    assert_human(i_am_the_human)?;
    let root = find_root();
    let ledger = Ledger::open(&root)?;
    let full = resolve_claim_id(&ledger, &claim)?;
    let snap = at_verify_snapshot(&root, &ledger, &full);
    ledger.append_batch(vec![NewEvent {
        etype: "demand".into(),
        actor: Actor::human(),
        body: serde_json::json!({ "claim": full, "at_verify": snap }),
    }])?;
    println!(
        "demanded evidence for {}. It leads the next brief.",
        short(&full)
    );
    Ok(())
}

pub fn pause(boundary: bool, script: bool, i_am_the_human: bool) -> Result<()> {
    assert_human(i_am_the_human)?;
    let root = find_root();
    crate::pause::run_pause(&root, crate::pause::PauseOpts { boundary, script })
}

pub fn exhaust(since: String, session: String) -> Result<()> {
    let root = find_root();
    let ledger = Ledger::open(&root)?;
    // `--since ROOT` names the ledger's own beginning, not the repo's. Passing the
    // literal through to `discover` would file every pre-existing commit as this
    // session's output — the Run-14 false fact. Resolve it to the baseline marker's
    // head; when the baseline honestly says "ROOT" (an empty repo at init time) the
    // repo's first commit IS the ledger's beginning and the whole history is the
    // truthful window. A ledger with no baseline cannot answer the question at all.
    let since = if since == "ROOT" {
        crate::state::baseline_head(&ledger.scan()?)
            .ok_or_else(crate::state::no_baseline_refusal)?
    } else {
        since
    };
    let window = crate::exhaust::discover(&root, &since, "HEAD", &session)?;
    match crate::exhaust::file_window(&ledger, &root, &window, None)? {
        Some(id) => println!(
            "filed exhaust claim {} ({} commits).",
            short(&id),
            window.shas.len()
        ),
        None => println!("nothing to file for session {session}."),
    }
    Ok(())
}

pub fn brief(json: bool) -> Result<()> {
    let root = find_root();
    let ledger = Ledger::open(&root)?;
    let mut d = crate::state::fold(&ledger.scan()?);
    crate::verify::annotate(&mut d, &root);
    print!("{}", crate::render::brief(&d, json));
    Ok(())
}

pub fn line(json: bool, stable: bool) -> Result<()> {
    let root = find_root();
    let ledger = Ledger::open(&root)?;
    let d = crate::state::fold(&ledger.scan()?);
    print!("{}", crate::render::line(&d, json, stable));
    Ok(())
}

pub fn indicator_declare(name: String, i_am_the_human: bool) -> Result<()> {
    assert_human(i_am_the_human)?;
    let root = find_root();
    let ledger = Ledger::open(&root)?;
    let d = crate::state::fold(&ledger.scan()?);
    if d.indicators.len() >= 4 {
        return Err(EvError::Refusal(
            "indicator ceiling is 4. Retire one first.".into(),
        ));
    }
    ledger.append_batch(vec![NewEvent {
        etype: "indicator".into(),
        actor: Actor::human(),
        body: serde_json::json!({ "name": name }),
    }])?;
    println!("indicator declared: {name}");
    Ok(())
}

pub fn indicator_retire(id: String, i_am_the_human: bool) -> Result<()> {
    assert_human(i_am_the_human)?;
    let root = find_root();
    let ledger = Ledger::open(&root)?;
    let full = resolve_id(&ledger, &id)?;
    ledger.append_batch(vec![NewEvent {
        etype: "retire".into(),
        actor: Actor::human(),
        body: serde_json::json!({ "indicator": full }),
    }])?;
    println!("retired {}", short(&full));
    Ok(())
}

pub fn doctor() -> Result<()> {
    let root = find_root();
    let ledger = Ledger::open(&root)?;
    let events = ledger.scan()?;
    let mut problems: Vec<String> = Vec::new();

    let claim_ids: std::collections::HashSet<&str> = events
        .iter()
        .filter(|e| e.etype == "claim")
        .map(|e| e.id.as_str())
        .collect();

    // Dangling refs: evidence/close/hold/demand/verify/prune/ack pointing at an unknown claim.
    for e in &events {
        if matches!(
            e.etype.as_str(),
            "evidence" | "close" | "hold" | "demand" | "verify" | "prune" | "ack"
        ) {
            if let Some(cid) = e.body.get("claim").and_then(|s| s.as_str()) {
                if !claim_ids.contains(cid) {
                    problems.push(format!(
                        "dangling {} → unknown claim {cid} (event {})",
                        e.etype, e.id
                    ));
                }
            }
        }
    }

    // Duplicate close transitions on the same claim.
    let mut closed_once: std::collections::HashSet<String> = std::collections::HashSet::new();
    for e in events.iter().filter(|e| e.etype == "close") {
        if let Some(cid) = e.body.get("claim").and_then(|s| s.as_str()) {
            if !closed_once.insert(cid.to_string()) {
                problems.push(format!("duplicate close on {cid}"));
            }
        }
    }

    // Clock drift: non-monotonic ts within a single writer's event stream.
    let mut last_ts: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
    for e in &events {
        if let Some(prev) = last_ts.get(e.writer.as_str()) {
            if e.ts.as_str() < *prev {
                problems.push(format!(
                    "clock drift on writer {}: {} < {}",
                    e.writer, e.ts, prev
                ));
            }
        }
        last_ts.insert(e.writer.as_str(), e.ts.as_str());
    }

    if problems.is_empty() {
        println!(
            "ledger clean: {} events, {} claims.",
            events.len(),
            claim_ids.len()
        );
    } else {
        for p in &problems {
            println!("• {p}");
        }
    }

    // Facts, not verdicts: the census lines below never gate. They report what
    // the ledger's shape already implies but no command has been saying out loud.
    // The cells the movement census counts do not exist until annotated here —
    // the same step `ev pause` takes before it reads them.
    let mut d = crate::state::fold(&events);
    crate::verify::annotate(&mut d, &root);
    print_liveness_census(&d);
    print_movement_census(&d);
    if !crate::state::has_baseline(&events) {
        // The two paths that need the baseline are the session-end sweep and
        // `ev exhaust --since ROOT`; `ev exhaust --since <sha>` carries its own
        // start and files without one. State only what is checked here.
        println!(
            "no baseline marker: the session-end sweep will not file a window. \
             Run `ev baseline [<sha>]` to record where the ledger began."
        );
    }

    if problems.is_empty() {
        Ok(())
    } else {
        Err(EvError::Failure(format!(
            "{} problem(s) found",
            problems.len()
        )))
    }
}

/// The liveness census: what it would take for each recorded anchor to go red.
/// Facts only — a count and one plain sentence. Never a score, never a gate.
///
/// Scope is every claim the fold knows: `claims` (open), `grey` (held) and
/// `closed` (closed or dead). A census over the open bucket alone would undercount
/// in silence — the exact failure this command exists to surface.
fn print_liveness_census(d: &crate::state::Derived) {
    use crate::verify::{EvRef, Liveness, RefKind};
    let mut content = 0usize;
    let mut existence = 0usize;
    let mut immutable = 0usize;
    let mut asserted = 0usize;
    // a ref no current grammar accepts. Counted out loud: a census that dropped it
    // would be the silent undercount this command exists to expose.
    let mut unparseable = 0usize;
    // the ref schemes actually in use — the count 0.2.3 judges `artifact:`/`url:`/
    // `metric:` with. Liveness is not the same question: an `artifact:…::text` and a
    // `file:…::text` are both `content`, and `metric:` and `url:` are both `asserted`.
    let mut commit = 0usize;
    let mut file = 0usize;
    let mut test = 0usize;
    let mut artifact = 0usize;
    let mut url = 0usize;
    let mut metric = 0usize;
    // claims whose every anchor is incapable of failing when the cited code changes
    let mut claims_total = 0usize;
    let mut claims_no_content = 0usize;

    for c in d.claims.iter().chain(&d.grey).chain(&d.closed) {
        if c.evidence.is_empty() {
            continue;
        }
        claims_total += 1;
        let mut has_content = false;
        for ev in &c.evidence {
            // the fold already carries the class; re-deriving it here is a second
            // source of truth that can drift from the one `brief --json` renders.
            // Exhaustive: a `_` arm in the one command that exists to prevent
            // silent undercounts would file a new class as `unparseable` unsaid.
            match ev.liveness {
                Liveness::Content => {
                    content += 1;
                    has_content = true;
                }
                Liveness::Existence => existence += 1,
                Liveness::Immutable => immutable += 1,
                Liveness::Asserted => asserted += 1,
                Liveness::Unparseable => unparseable += 1,
            }
            if let Ok(r) = EvRef::parse(&ev.eref) {
                match r.kind {
                    RefKind::Commit => commit += 1,
                    RefKind::File => file += 1,
                    RefKind::Test => test += 1,
                    RefKind::Artifact => artifact += 1,
                    RefKind::Url => url += 1,
                    RefKind::Metric => metric += 1,
                }
            }
        }
        if !has_content {
            claims_no_content += 1;
        }
    }
    if claims_total == 0 {
        return;
    }
    println!(
        "anchor liveness (every claim, open and closed): content {content} · existence {existence} · immutable {immutable} · asserted {asserted} · unparseable {unparseable}"
    );
    println!(
        "ref types in use: commit {commit} · file {file} · test {test} · artifact {artifact} · url {url} · metric {metric}"
    );
    if claims_no_content > 0 {
        println!(
            "  ⚠ {claims_no_content} of {claims_total} claims rest only on anchors that cannot fail when the cited code changes."
        );
    }
    if asserted > 0 {
        println!(
            "  ⚠ {asserted} anchor(s) are metric:/url: — {}.",
            Liveness::Asserted.why()
        );
    }
}

/// How far the world moved while the ledger was not looking. Facts only: counts, and
/// one sentence that says RE-READ. ev cannot know whether a claim was resolved — a fix
/// that adds code beside the anchored line leaves the anchor green — and never says so.
/// Never changes the exit code.
///
/// Scope matches the liveness census exactly: every claim carrying evidence, across
/// `claims` (open), `grey` (held) and `closed` (closed or dead) — the same full set, so
/// this census cannot undercount where the other does not. A claim ev could place on no
/// cell at all is counted as `unmeasured` and stays in the denominator: dropping it would
/// shrink the census's own total in silence and call the remainder "claims" — the exact
/// undercount this command exists to expose.
fn print_movement_census(d: &crate::state::Derived) {
    use crate::verify::Cell;
    let mut still = 0usize;
    let mut moved = 0usize;
    let mut changed = 0usize;
    let mut gone = 0usize;
    let mut legacy = 0usize;
    // no cell at all: a `commit:`/`metric:`/`url:` anchor (no path to move under), an
    // anchor with no reference point to count from, or a path git could not answer for.
    let mut unmeasured = 0usize;
    let mut total = 0usize;

    for c in d.claims.iter().chain(&d.grey).chain(&d.closed) {
        if c.evidence.is_empty() {
            continue;
        }
        let worst = c
            .evidence
            .iter()
            .filter_map(|e| e.cell)
            .max_by_key(|cell| cell.severity());
        match worst {
            None => unmeasured += 1,
            Some(Cell::FileGone) => gone += 1,
            Some(Cell::AnchorChanged) => changed += 1,
            Some(Cell::NeighborhoodMoved) => moved += 1,
            Some(Cell::Legacy) => legacy += 1,
            Some(Cell::Still) => still += 1,
        }
        total += 1;
    }
    if total == 0 {
        return;
    }
    println!(
        "{total} claims · still {still} · neighborhood-moved {moved} · anchor-changed {changed} · file-gone {gone} · legacy {legacy} · unmeasured {unmeasured}"
    );
    if unmeasured > 0 {
        println!(
            "  ⚠ {unmeasured} claim(s) carry no anchor ev can place on the movement map — ev reports nothing about them."
        );
    }
    if moved > 0 {
        println!(
            "  ⚠ {moved} claims sit on code that moved beside the anchored line — the anchor cannot see it. Re-read."
        );
    }
    if changed + gone > 0 {
        println!(
            "  ⚠ {} claims rest on an anchor whose cited code is gone. Re-read.",
            changed + gone
        );
    }
}

pub fn hook(action: String) -> Result<()> {
    let root = find_root();
    match action.as_str() {
        "install" => crate::hooks::install(&root),
        "uninstall" => crate::hooks::uninstall(&root),
        "session-start" => crate::hooks::session_start(&root),
        "session-end" => crate::hooks::session_end(&root),
        other => Err(EvError::Failure(format!("unknown hook action: {other}"))),
    }
}
