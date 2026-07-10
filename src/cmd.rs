use crate::ledger::{Actor, ActorKind, Ledger, NewEvent};
use crate::state::fold;
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
    let _ = crate::ledger::Ledger::open(&root)?;
    println!("initialized .evolving/ at {}", root.display());
    println!("ev refreshes when invoked, not in the background.");
    Ok(())
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
        Actor {
            kind: ActorKind::Human,
            id: None,
            via: None,
        }
    };
    let mut body = serde_json::json!({ "label": args.label });
    if let Some(sref) = &args.source_ref {
        body["source_ref"] = serde_json::json!(sref);
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
    let actor = Actor {
        kind: ActorKind::Human,
        id: None,
        via: None,
    };
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
    match id.split_once('_') {
        Some((p, rest)) => format!("{p}_{}", &rest[..rest.len().min(6)]),
        None => id.to_string(),
    }
}

fn agent_id() -> Option<String> {
    if std::env::var("CLAUDECODE").is_ok() {
        Some("claude-code".into())
    } else {
        std::env::var("EV_AGENT").ok()
    }
}

/// Fold the full ledger into derived state; used by several read-path verbs.
pub fn load_derived(ledger: &Ledger) -> Result<crate::state::Derived> {
    Ok(fold(&ledger.scan()?))
}

// ── evidence + verify verbs ───────────────────────────────────────────────────

/// Attach a typed evidence ref to a claim. Agents are permitted.
pub fn evidence(claim_id: String, eref: String) -> Result<()> {
    let root = find_root();
    let ledger = Ledger::open(&root)?;
    let full = resolve_id(&ledger, &claim_id)?;
    let actor = evidence_actor();
    let verdict = crate::verify::verify_and_record(&ledger, &root, &full, &eref, false, actor)?;
    println!("evidence attached to {} → {verdict}", short(&full));
    Ok(())
}

/// Re-verify a single claim's evidence, or all open claims' evidence.
pub fn verify_cmd(claim_id: Option<String>) -> Result<()> {
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
    for c in targets {
        for ev in &c.evidence {
            if let Ok(r) = crate::verify::EvRef::parse(&ev.eref) {
                let status = crate::verify::verify_ref(&r, &root);
                ledger.append_batch(vec![NewEvent {
                    etype: "verify".into(),
                    actor: Actor {
                        kind: ActorKind::Engine,
                        id: None,
                        via: None,
                    },
                    body: serde_json::json!({
                        "claim": c.id,
                        "ref": ev.eref,
                        "status": status,
                    }),
                }])?;
                println!("{} · {} → {status}", short(&c.id), ev.eref);
            }
        }
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
        Actor {
            kind: ActorKind::Agent,
            id: Some("claude-code".into()),
            via: None,
        }
    } else {
        Actor {
            kind: ActorKind::Human,
            id: None,
            via: None,
        }
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
            actor: Actor {
                kind: ActorKind::Human,
                id: None,
                via: None,
            },
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
        actor: Actor {
            kind: ActorKind::Human,
            id: None,
            via: None,
        },
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
        actor: Actor {
            kind: ActorKind::Human,
            id: None,
            via: None,
        },
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
        actor: Actor {
            kind: ActorKind::Human,
            id: None,
            via: None,
        },
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
    let window = crate::exhaust::discover(&root, &since, &session)?;
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
    let d = crate::state::fold(&ledger.scan()?);
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
        actor: Actor {
            kind: ActorKind::Human,
            id: None,
            via: None,
        },
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
        actor: Actor {
            kind: ActorKind::Human,
            id: None,
            via: None,
        },
        body: serde_json::json!({ "indicator": full }),
    }])?;
    println!("retired {}", short(&full));
    Ok(())
}
