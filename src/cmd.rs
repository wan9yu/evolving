use crate::ledger::{Actor, ActorKind, Ledger, NewEvent};
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
    if !has_baseline(&ledger.scan()?) {
        write_baseline(&ledger, &root)?;
    }
    println!("initialized .evolving/ at {}", root.display());
    println!("ev refreshes when invoked, not in the background.");
    Ok(())
}

/// Record where this ledger began: the current HEAD, or the honest literal
/// "ROOT" when the repo carries no commits yet.
fn write_baseline(ledger: &Ledger, root: &Path) -> Result<String> {
    let head = crate::git_output(root, &["rev-parse", "HEAD"]).unwrap_or_else(|| "ROOT".into());
    ledger.append_batch(vec![NewEvent {
        etype: "session".into(),
        actor: Actor::engine(),
        body: serde_json::json!({ "marker": "baseline", "head": head }),
    }])?;
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
            ledger.append_batch(vec![NewEvent {
                etype: "session".into(),
                actor: Actor::engine(),
                body: serde_json::json!({ "marker": "baseline", "head": resolved }),
            }])?;
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

/// Whether the ledger already carries a baseline marker: a `session` event
/// whose body is `{"marker":"baseline", ...}`. The one predicate for the two
/// sites that must agree on it: `init` (skip a redundant write) and `exhaust`
/// (refuse `--since ROOT` without one).
fn has_baseline(events: &[crate::ledger::Envelope]) -> bool {
    events.iter().any(|e| {
        e.etype == "session" && e.body.get("marker").and_then(|s| s.as_str()) == Some("baseline")
    })
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

    // an inline --evidence attaches an evidence event referencing the just-minted claim.
    // Because the batch is one atomic write, we mint the claim first, then reference it.
    let minted = ledger.append_batch(batch)?;
    if let Some(eref) = &args.evidence {
        let claim_id = &minted[0].id;
        let verdict =
            crate::verify::verify_and_record(&ledger, &root, claim_id, eref, false, actor)?;
        println!(
            "claim {} · evidence {} → {}",
            short(claim_id),
            eref,
            verdict
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
    let scheme = match r.kind {
        crate::verify::RefKind::Test => "test",
        crate::verify::RefKind::Artifact => "artifact",
        _ => "file",
    };
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
    let full = resolve_id(&ledger, &claim_id)?;
    let actor = evidence_actor();
    let verdict = crate::verify::verify_and_record(&ledger, &root, &full, &eref, false, actor)?;
    println!("evidence attached to {} → {verdict}", short(&full));
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
            let full = resolve_id(&ledger, cid)?;
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
                ledger.append_batch(vec![NewEvent {
                    etype: "verify".into(),
                    actor: Actor::engine(),
                    body: serde_json::json!({
                        "claim": c.id,
                        "ref": ev.eref,
                        "status": status,
                    }),
                }])?;
                // drift: the world's movement under the anchor, in commits touching
                // the cited path — a structural fact, judged by the human.
                let moved = ev
                    .base
                    .as_deref()
                    .and_then(|base| crate::verify::drift(&root, base, &r));
                if json {
                    let mut check = serde_json::json!({
                        "claim": c.id,
                        "ref": ev.eref,
                        "status": status,
                    });
                    if let Some(base) = &ev.base {
                        check["base"] = serde_json::json!(base);
                    }
                    if let Some(k) = moved {
                        check["drift"] = serde_json::json!(k);
                    }
                    checks.push(check);
                } else {
                    match moved {
                        Some(k) if k > 0 => println!(
                            "{} · {} → {status} · {}",
                            short(&c.id),
                            ev.eref,
                            crate::verify::drift_phrase(k)
                        ),
                        _ => println!("{} · {} → {status}", short(&c.id), ev.eref),
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
    let full = resolve_id(&ledger, &args.claim)?;
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
        ledger.append_batch(vec![NewEvent {
            etype: "prune".into(),
            actor: Actor::human(),
            body: serde_json::json!({ "claim": full, "reason": reason }),
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
    ledger.append_batch(vec![NewEvent {
        etype: "close".into(),
        actor: Actor::human(),
        body: serde_json::json!({ "claim": full }),
    }])?;
    println!("closed {} with evidence.", short(&full));
    Ok(())
}

pub fn hold(claim: String, reason: String, i_am_the_human: bool) -> Result<()> {
    assert_human(i_am_the_human)?;
    let root = find_root();
    let ledger = Ledger::open(&root)?;
    let full = resolve_id(&ledger, &claim)?;
    ledger.append_batch(vec![NewEvent {
        etype: "hold".into(),
        actor: Actor::human(),
        body: serde_json::json!({ "claim": full, "reason": reason }),
    }])?;
    println!("held (grey): {} — {reason}", short(&full));
    Ok(())
}

pub fn demand(claim: String, i_am_the_human: bool) -> Result<()> {
    assert_human(i_am_the_human)?;
    let root = find_root();
    let ledger = Ledger::open(&root)?;
    let full = resolve_id(&ledger, &claim)?;
    ledger.append_batch(vec![NewEvent {
        etype: "demand".into(),
        actor: Actor::human(),
        body: serde_json::json!({ "claim": full }),
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
    // `--since ROOT` on a ledger with no baseline is the Run-14 false fact.
    if since == "ROOT" {
        let events = ledger.scan()?;
        if !has_baseline(&events) {
            return Err(EvError::Refusal(
                "no baseline marker in this ledger — filing --since ROOT would record \
                 pre-existing commits as this session's output.\n    \
                 Run `ev baseline [<sha>]` to record where the ledger began."
                    .into(),
            ));
        }
    }
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
    crate::verify::annotate_drift(&mut d, &root);
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

    // Dangling refs: evidence/close/hold/demand/verify/prune pointing at an unknown claim.
    for e in &events {
        if matches!(
            e.etype.as_str(),
            "evidence" | "close" | "hold" | "demand" | "verify" | "prune"
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
        Ok(())
    } else {
        for p in &problems {
            println!("• {p}");
        }
        Err(EvError::Failure(format!(
            "{} problem(s) found",
            problems.len()
        )))
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
