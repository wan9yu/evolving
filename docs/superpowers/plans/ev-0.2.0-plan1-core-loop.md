# ev 0.2.0 · Plan #1 — Core Loop Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the ev 0.2.0 closure loop end-to-end on one macOS machine — coding-agent sessions auto-file evidence-bearing claims, a daily ≤5-minute pause is where the human closes with evidence / holds grey / declares dead, and a line of boundary snapshots accumulates.

**Architecture:** One Rust crate (`evolving`, binary `ev`), seven job-bearing modules (`ledger`, `state`, `verify`, `exhaust`, `hooks`, `pause`, `render`) plus `main.rs` (clap dispatch), `lib.rs` (error/exit/consts), and `cmd.rs` (thin verb handlers that wire the modules). Append-only per-writer JSONL ledger, one atomic batch-append write primitive, a pure fold from event log to derived state, deterministic pointer verification (existence/match only), goldens on `--stable` output from day one.

**Tech Stack:** Rust 2021; deps `clap` (derive), `serde`, `serde_json`, `ulid`, `sha2`, `fs4`, `time`. Git via subprocess (no git library). No TUI crate — the pause is a line-oriented prompt loop reading stdin.

**Spec:** `docs/superpowers/specs/ev-0.2.0-plan1-core-loop-design.md` (normative). Every task below cites the section it implements.

**Plan conventions (read once):**
- Filename carries no calendar date (repo red line: no "today"/dates in docs — use versions). This deviates from the skill's `YYYY-MM-DD-` default on purpose.
- Every commit message and code comment stays clean of tool names and version-narrative; comments state the concept, not when it shipped. Author is the repo's configured identity.
- `cargo test`, `cargo fmt --check`, `cargo clippy -- -D warnings` must pass at every "run tests" step. BDD test names: `fn <thing>_<condition>_<expected>()`.
- Commit at the end of each task only (the human authorizes tags/pushes/publish separately — this plan never tags or publishes).
- Deferred to Plan #2 (do NOT build): `ev rep` + machine-global hooklog + UserPromptSubmit hook + human-capability indicator; `line --html` + publishing; fleet/external-ledger enrollment; agents-md emitter; graduation detection; `--redact`; `doctor --git`; LLM label drafting.

---

## Shared Types (locked here; every task reuses these exact signatures)

These are introduced by the tasks noted and MUST match across the whole plan. Listed together so later tasks can reference them without redefining.

```rust
// lib.rs
pub const SCHEMA_VERSION: u8 = 2;

pub type Result<T> = std::result::Result<T, EvError>;

#[derive(Debug)]
pub enum EvError {
    Refusal(String), // exit 1 — an honest refusal (bare close, wrong actor, overdue precondition)
    Failure(String), // exit 2 — an error (io, parse, git)
}

// ledger.rs
pub struct Envelope {
    pub v: u8,                 // = SCHEMA_VERSION
    pub id: String,            // type-prefixed ULID, e.g. "clm_01JABC..."
    pub ts: String,            // RFC3339 UTC, second precision
    pub writer: String,        // writer id, e.g. "arguspi-3f9a"
    pub seq: u64,              // per-writer monotonic
    pub actor: Actor,
    pub etype: String,         // serialized as "type"
    pub body: serde_json::Value,
}
pub struct Actor { pub kind: ActorKind, pub id: Option<String>, pub via: Option<String> }
pub enum ActorKind { Human, Agent, Engine }   // serde lowercase

// verify.rs
pub enum RefKind { Commit, Test, File, Artifact, Metric, Url }
pub struct EvRef { pub kind: RefKind, pub payload: String, pub passline: Option<String> }
pub enum Status { Verified, Failed, Unreachable, Recorded }  // serde lowercase

// state.rs
pub enum ClaimState { Bare, Evidenced, Verified, Grey, Closed, Dead, ExpiredBare }
pub struct EvidenceView { pub eref: String, pub status: Status, pub self_evident: bool }
pub struct ClaimView {
    pub id: String, pub label: String, pub state: ClaimState,
    pub evidence: Vec<EvidenceView>, pub self_evident: bool,
    pub boundaries_open: u32, pub referenced_by: u32,
    pub source_ref: Option<String>, pub reason: Option<String>,
}
pub struct Derived {
    pub claims: Vec<ClaimView>,        // all non-closed, non-dead, in filing order
    pub closed: Vec<ClaimView>,
    pub grey: Vec<ClaimView>,
    pub thoughts: Vec<ThoughtView>,
    pub demands_returned: Vec<ClaimView>, // demanded claims that since gained evidence
    pub indicators: Vec<IndicatorView>,
    pub snapshots: Vec<Snapshot>,
    pub last_event_id: Option<String>,
    pub boundary_count: u32,
}
```

---

## File Structure

- `Cargo.toml` — crate `evolving` 0.2.0, `[[bin]] name = "ev"`, deps.
- `src/lib.rs` — module decls, `SCHEMA_VERSION`, `EvError` + `exit_code`, `Result`.
- `src/main.rs` — clap `Cli`/`Command` derive, dispatch to `cmd::*`, maps `EvError` → process exit code.
- `src/ledger.rs` — `Envelope`, `Actor`, id minting, writer identity, `append_batch`, `scan`, layout paths.
- `src/state.rs` — the fold: `fold(&[Envelope]) -> Derived`; claim/thought/indicator machines; grey/starvation; counted-set snapshots.
- `src/verify.rs` — `EvRef::parse`, `verify_ref(&EvRef, repo) -> Status`, V1/V2, `self_evident` tagging.
- `src/exhaust.rs` — git-window discovery, `sweep`, one-claim-per-session filing, the label rule.
- `src/hooks.rs` — `install`/`uninstall` (settings merge), `session_start`, `session_end` handlers.
- `src/pause.rs` — the ritual: screens 0–5, prompt loop, receipt.
- `src/render.rs` — terminal line, brief text, `--json`/`--stable` (deterministic), the footer.
- `src/cmd.rs` — one thin handler per verb, wiring the modules.
- `tests/*.rs` — BDD-named integration tests (one file per area).

---

### Task 1: Crate scaffold + version smoke test

**Files:**
- Create: `Cargo.toml`
- Create: `src/lib.rs`
- Create: `src/main.rs`
- Test: `tests/cli_smoke.rs`

- [ ] **Step 1: Write the failing test**

`tests/cli_smoke.rs`:
```rust
use std::process::Command;

#[test]
fn version_flag_prints_semver() {
    let out = Command::new(env!("CARGO_BIN_EXE_ev"))
        .arg("--version")
        .output()
        .expect("binary runs");
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success());
    assert!(s.contains("0.2.0"), "version output was: {s}");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test cli_smoke`
Expected: FAIL — no `Cargo.toml`/binary yet (compile error).

- [ ] **Step 3: Write minimal implementation**

`Cargo.toml`:
```toml
[package]
name = "evolving"
version = "0.2.0"
edition = "2021"
license = "MIT"
description = "A closure engine for one human and their agent fleet."

[[bin]]
name = "ev"
path = "src/main.rs"

[dependencies]
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
ulid = "1"
sha2 = "0.10"
fs4 = "0.8"
time = { version = "0.3", features = ["formatting", "parsing", "macros"] }
```

`src/lib.rs`:
```rust
pub const SCHEMA_VERSION: u8 = 2;

pub type Result<T> = std::result::Result<T, EvError>;

#[derive(Debug)]
pub enum EvError {
    Refusal(String),
    Failure(String),
}

impl EvError {
    pub fn exit_code(&self) -> i32 {
        match self {
            EvError::Refusal(_) => 1,
            EvError::Failure(_) => 2,
        }
    }
}

impl std::fmt::Display for EvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvError::Refusal(m) | EvError::Failure(m) => write!(f, "{m}"),
        }
    }
}

impl From<std::io::Error> for EvError {
    fn from(e: std::io::Error) -> Self {
        EvError::Failure(e.to_string())
    }
}

pub mod ledger;
pub mod state;
pub mod verify;
pub mod exhaust;
pub mod hooks;
pub mod pause;
pub mod render;
pub mod cmd;
```

Create empty module stubs so it compiles — each file just `// filled in Task N`:
`src/ledger.rs`, `src/state.rs`, `src/verify.rs`, `src/exhaust.rs`, `src/hooks.rs`, `src/pause.rs`, `src/render.rs`, `src/cmd.rs` each containing a single line comment.

`src/main.rs`:
```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "ev", version, about = "A closure engine for one human and their agent fleet.")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        None => {
            println!("ev — run `ev --help`. Nothing runs in the background; ev refreshes when invoked.");
        }
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test cli_smoke && cargo fmt --check && cargo clippy -- -D warnings`
Expected: PASS; fmt clean; clippy clean.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src tests
git commit -m "scaffold: crate, error type, version smoke test"
```

---

### Task 2: Ledger envelope, id minting, writer identity

Implements spec §Ledger (envelope; type-prefixed ULIDs; writer id).

**Files:**
- Modify: `src/ledger.rs`
- Test: `tests/ledger_envelope.rs`

- [ ] **Step 1: Write the failing test**

`tests/ledger_envelope.rs`:
```rust
use evolving::ledger::{mint_id, Actor, ActorKind, Envelope};

#[test]
fn minted_id_carries_the_type_prefix() {
    let id = mint_id("claim");
    assert!(id.starts_with("clm_"), "got {id}");
    assert!(id.len() > 10);
}

#[test]
fn envelope_serializes_type_as_the_type_key() {
    let e = Envelope {
        v: 2,
        id: "clm_01JABC".into(),
        ts: "2020-01-01T00:00:00Z".into(),
        writer: "host-0000".into(),
        seq: 1,
        actor: Actor { kind: ActorKind::Agent, id: Some("cc".into()), via: None },
        etype: "claim".into(),
        body: serde_json::json!({"label": "fixed X"}),
    };
    let s = serde_json::to_string(&e).unwrap();
    assert!(s.contains("\"type\":\"claim\""), "{s}");
    assert!(s.contains("\"kind\":\"agent\""), "{s}");
    // via is None -> omitted
    assert!(!s.contains("\"via\""), "{s}");
    // round-trips
    let back: Envelope = serde_json::from_str(&s).unwrap();
    assert_eq!(back.seq, 1);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test ledger_envelope`
Expected: FAIL — `mint_id`/`Envelope` not defined.

- [ ] **Step 3: Write minimal implementation**

`src/ledger.rs` (this task's portion):
```rust
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ActorKind {
    Human,
    Agent,
    Engine,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Actor {
    pub kind: ActorKind,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub via: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Envelope {
    pub v: u8,
    pub id: String,
    pub ts: String,
    pub writer: String,
    pub seq: u64,
    pub actor: Actor,
    #[serde(rename = "type")]
    pub etype: String,
    pub body: serde_json::Value,
}

/// Three-letter stable prefix per event type. The table is frozen from day one;
/// unknown types fall back to their first three letters.
pub fn prefix_for(etype: &str) -> String {
    let p = match etype {
        "thought" => "thk",
        "pull" => "pul",
        "promote" => "pro",
        "claim" => "clm",
        "evidence" => "evd",
        "verify" => "vfy",
        "close" => "cls",
        "hold" => "hld",
        "renew" => "ren",
        "prune" => "prn",
        "demand" => "dmd",
        "indicator" => "ind",
        "retire" => "ret",
        "repwindow" => "rpw",
        "repclose" => "rpc",
        "snapshot" => "snp",
        "pause" => "pau",
        "cadence" => "cad",
        "session" => "ses",
        other => &other[..other.len().min(3)],
    };
    p.to_string()
}

pub fn mint_id(etype: &str) -> String {
    format!("{}_{}", prefix_for(etype), ulid::Ulid::new())
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test ledger_envelope && cargo clippy -- -D warnings`
Expected: PASS; clippy clean.

- [ ] **Step 5: Commit**

```bash
git add src/ledger.rs tests/ledger_envelope.rs
git commit -m "ledger: envelope, type-prefixed id minting, actor kinds"
```

---

### Task 3: Layout paths + writer identity + atomic batch append + scan

Implements spec §Ledger (layout; one write primitive; flock seq; torn-tail-tolerant scan; ULID dedupe; sort).

**Files:**
- Modify: `src/ledger.rs`
- Test: `tests/ledger_io.rs`

- [ ] **Step 1: Write the failing test**

`tests/ledger_io.rs`:
```rust
use evolving::ledger::{self, Actor, ActorKind, Ledger};
use std::fs;

fn tmp() -> std::path::PathBuf {
    let base = std::env::temp_dir().join(format!("ev-io-{}", ulid::Ulid::new()));
    fs::create_dir_all(base.join(".evolving/ledger")).unwrap();
    fs::create_dir_all(base.join(".evolving/local")).unwrap();
    base
}

fn ev(kind: &str, body: serde_json::Value) -> ledger::NewEvent {
    ledger::NewEvent {
        etype: kind.into(),
        actor: Actor { kind: ActorKind::Human, id: None, via: None },
        body,
    }
}

#[test]
fn appended_batch_is_read_back_in_seq_order() {
    let root = tmp();
    let l = Ledger::open(&root).unwrap();
    l.append_batch(vec![ev("claim", serde_json::json!({"label":"a"}))]).unwrap();
    l.append_batch(vec![
        ev("claim", serde_json::json!({"label":"b"})),
        ev("evidence", serde_json::json!({"label":"c"})),
    ]).unwrap();
    let events = l.scan().unwrap();
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].seq, 1);
    assert_eq!(events[2].seq, 3);
}

#[test]
fn torn_trailing_line_is_skipped_not_fatal() {
    let root = tmp();
    let l = Ledger::open(&root).unwrap();
    l.append_batch(vec![ev("claim", serde_json::json!({"label":"a"}))]).unwrap();
    // simulate a killed mid-write: append a partial JSON line
    let wid = l.writer_id().to_string();
    let path = root.join(".evolving/ledger").join(format!("{wid}.jsonl"));
    let mut content = fs::read_to_string(&path).unwrap();
    content.push_str("{\"v\":2,\"id\":\"clm_partial\"");
    fs::write(&path, content).unwrap();
    let events = l.scan().unwrap(); // must not error
    assert_eq!(events.len(), 1);
}

#[test]
fn duplicate_ids_across_files_are_deduped() {
    let root = tmp();
    let l = Ledger::open(&root).unwrap();
    l.append_batch(vec![ev("claim", serde_json::json!({"label":"a"}))]).unwrap();
    let events = l.scan().unwrap();
    let line = serde_json::to_string(&events[0]).unwrap();
    // a second writer file carrying the same id
    fs::write(root.join(".evolving/ledger/other-1111.jsonl"), format!("{line}\n")).unwrap();
    let events = l.scan().unwrap();
    assert_eq!(events.len(), 1, "same id must dedupe");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test ledger_io`
Expected: FAIL — `Ledger`, `NewEvent` not defined.

- [ ] **Step 3: Write minimal implementation**

Append to `src/ledger.rs`:
```rust
use crate::{EvError, Result, SCHEMA_VERSION};
use fs4::fs_std::FileExt;
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub struct NewEvent {
    pub etype: String,
    pub actor: Actor,
    pub body: serde_json::Value,
}

pub struct Ledger {
    root: PathBuf,   // repo root (contains .evolving/)
    writer: String,
}

impl Ledger {
    /// Open the ledger rooted at `root` (the dir containing `.evolving/`).
    pub fn open(root: &Path) -> Result<Ledger> {
        let writer = load_or_make_writer(root)?;
        Ok(Ledger { root: root.to_path_buf(), writer })
    }

    pub fn writer_id(&self) -> &str {
        &self.writer
    }

    fn ledger_dir(&self) -> PathBuf {
        self.root.join(".evolving/ledger")
    }
    fn writer_path(&self) -> PathBuf {
        self.ledger_dir().join(format!("{}.jsonl", self.writer))
    }
    fn seq_lock_path(&self) -> PathBuf {
        self.root.join(".evolving/local/writer.toml")
    }

    /// The one write primitive: serialize the whole batch to a single buffer,
    /// take the seq lock, one append write + fsync. A killed process can never
    /// leave a dangling intra-batch reference — either the whole line-set lands
    /// or (a torn final line) is skipped on read.
    pub fn append_batch(&self, events: Vec<NewEvent>) -> Result<Vec<Envelope>> {
        if events.is_empty() {
            return Ok(vec![]);
        }
        fs::create_dir_all(self.ledger_dir())?;
        let lockf = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(self.seq_lock_path())?;
        lockf.lock_exclusive()?;

        let mut seq = self.tail_seq()?;
        let ts = now_rfc3339();
        let mut buf = String::new();
        let mut minted = Vec::with_capacity(events.len());
        for ne in events {
            seq += 1;
            let env = Envelope {
                v: SCHEMA_VERSION,
                id: mint_id(&ne.etype),
                ts: ts.clone(),
                writer: self.writer.clone(),
                seq,
                actor: ne.actor,
                etype: ne.etype,
                body: ne.body,
            };
            buf.push_str(&serde_json::to_string(&env).map_err(|e| EvError::Failure(e.to_string()))?);
            buf.push('\n');
            minted.push(env);
        }
        let mut f = OpenOptions::new().create(true).append(true).open(self.writer_path())?;
        self.heal_torn_tail(&mut f)?;
        f.write_all(buf.as_bytes())?;
        f.sync_all()?;
        FileExt::unlock(&lockf)?;
        Ok(minted)
    }

    /// Highest seq this writer has already written (0 if none).
    fn tail_seq(&self) -> Result<u64> {
        let path = self.writer_path();
        if !path.exists() {
            return Ok(0);
        }
        let content = fs::read_to_string(&path)?;
        let mut max = 0u64;
        for line in content.lines() {
            if let Ok(env) = serde_json::from_str::<Envelope>(line) {
                max = max.max(env.seq);
            }
        }
        Ok(max)
    }

    /// If the writer file ends with a provably partial (unparseable) line and no
    /// trailing newline, truncate it before appending.
    fn heal_torn_tail(&self, f: &mut File) -> Result<()> {
        let mut content = String::new();
        let path = self.writer_path();
        if let Ok(mut existing) = File::open(&path) {
            existing.read_to_string(&mut content)?;
        }
        if content.is_empty() || content.ends_with('\n') {
            return Ok(());
        }
        let last = content.rlines_last();
        if serde_json::from_str::<Envelope>(&last).is_err() {
            let keep = content.len() - last.len();
            f.set_len(keep as u64)?;
        }
        Ok(())
    }

    /// Scan every writer file, skip torn lines, dedupe by id, sort by (ts, writer, seq).
    pub fn scan(&self) -> Result<Vec<Envelope>> {
        let dir = self.ledger_dir();
        let mut all: Vec<Envelope> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        if dir.exists() {
            for entry in fs::read_dir(&dir)? {
                let path = entry?.path();
                if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                    continue;
                }
                let content = fs::read_to_string(&path)?;
                for line in content.lines() {
                    if line.trim().is_empty() {
                        continue;
                    }
                    match serde_json::from_str::<Envelope>(line) {
                        Ok(env) => {
                            if seen.insert(env.id.clone()) {
                                all.push(env);
                            }
                        }
                        Err(_) => {
                            eprintln!("ev: skipped a torn ledger line in {}", path.display());
                        }
                    }
                }
            }
        }
        all.sort_by(|a, b| (&a.ts, &a.writer, a.seq).cmp(&(&b.ts, &b.writer, b.seq)));
        Ok(all)
    }
}

trait RLinesLast {
    fn rlines_last(&self) -> String;
}
impl RLinesLast for String {
    fn rlines_last(&self) -> String {
        match self.rfind('\n') {
            Some(i) => self[i + 1..].to_string(),
            None => self.clone(),
        }
    }
}

fn load_or_make_writer(root: &Path) -> Result<String> {
    let path = root.join(".evolving/local/writer.toml");
    if let Ok(s) = fs::read_to_string(&path) {
        for line in s.lines() {
            if let Some(v) = line.strip_prefix("id = ") {
                return Ok(v.trim().trim_matches('"').to_string());
            }
        }
    }
    let host = hostname_slug();
    let suffix: String = ulid::Ulid::new().to_string().chars().rev().take(4).collect();
    let id = format!("{host}-{}", suffix.to_lowercase());
    fs::create_dir_all(root.join(".evolving/local"))?;
    fs::write(&path, format!("id = \"{id}\"\n"))?;
    Ok(id)
}

fn hostname_slug() -> String {
    let raw = std::env::var("HOSTNAME")
        .ok()
        .or_else(|| {
            std::process::Command::new("hostname")
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
        })
        .unwrap_or_else(|| "host".into());
    let slug: String = raw
        .trim()
        .split('.')
        .next()
        .unwrap_or("host")
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    if slug.is_empty() { "host".into() } else { slug }
}

pub fn now_rfc3339() -> String {
    use time::format_description::well_known::Rfc3339;
    time::OffsetDateTime::now_utc()
        .replace_nanosecond(0)
        .unwrap()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}
```

Note for the implementer: `fs4`'s `FileExt::unlock` and `lock_exclusive` come from `fs4::fs_std::FileExt`. If the installed `fs4` version differs, adjust the import path but keep the flock-on-writer.toml semantics.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test ledger_io && cargo clippy -- -D warnings`
Expected: PASS — 3 tests green; clippy clean.

- [ ] **Step 5: Commit**

```bash
git add src/ledger.rs tests/ledger_io.rs
git commit -m "ledger: layout, writer identity, atomic batch append, torn-tolerant scan"
```

---

### Task 4: `ev init` — create the layout, gitattributes, repo registry

Implements spec §Ledger (`ev init` creates `.evolving/`, adds `merge=union`, registers repo) and §Verbs (`init`).

**Files:**
- Modify: `src/cmd.rs`, `src/main.rs`
- Test: `tests/init.rs`

- [ ] **Step 1: Write the failing test**

`tests/init.rs`:
```rust
use std::process::Command;

fn run_in(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ev")).args(args).current_dir(dir).output().unwrap()
}

#[test]
fn init_creates_the_ledger_tree_and_union_merge() {
    let dir = std::env::temp_dir().join(format!("ev-init-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = run_in(&dir, &["init"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(dir.join(".evolving/version").exists());
    assert!(dir.join(".evolving/ledger").is_dir());
    assert!(dir.join(".evolving/artifacts").is_dir());
    assert!(dir.join(".evolving/.gitignore").exists());
    let attrs = std::fs::read_to_string(dir.join(".gitattributes")).unwrap();
    assert!(attrs.contains("merge=union"), "{attrs}");
    assert_eq!(std::fs::read_to_string(dir.join(".evolving/version")).unwrap().trim(), "2");
}

#[test]
fn init_is_idempotent() {
    let dir = std::env::temp_dir().join(format!("ev-init2-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    assert!(run_in(&dir, &["init"]).status.success());
    let out = run_in(&dir, &["init"]);
    assert!(out.status.success());
    // .gitattributes must not double the union line
    let attrs = std::fs::read_to_string(dir.join(".gitattributes")).unwrap();
    assert_eq!(attrs.matches("merge=union").count(), 1, "{attrs}");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test init`
Expected: FAIL — no `init` subcommand.

- [ ] **Step 3: Write minimal implementation**

`src/cmd.rs` (this task's portion):
```rust
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
    write_if_absent(&ev.join("config.toml"), "# ev preferences (non-historical)\n")?;
    write_if_absent(
        &ev.join(".gitignore"),
        "local/\ncache/\n",
    )?;
    ensure_line(&root.join(".gitattributes"), ".evolving/ledger/*.jsonl merge=union")?;
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
```

Add to `src/main.rs`: the `Init` variant and dispatch.
```rust
#[derive(Subcommand)]
enum Command {
    /// Create .evolving/ here and register the repo.
    Init,
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        None => {
            println!("ev — run `ev --help`. Nothing runs in the background; ev refreshes when invoked.");
            Ok(())
        }
        Some(Command::Init) => evolving::cmd::init(),
    };
    if let Err(e) = result {
        eprintln!("ev: {e}");
        std::process::exit(e.exit_code());
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test init && cargo clippy -- -D warnings`
Expected: PASS — 2 tests green.

- [ ] **Step 5: Commit**

```bash
git add src/cmd.rs src/main.rs tests/init.rs
git commit -m "init: create the ledger layout, union-merge attribute, repo registry"
```

---

### Task 5: The fold — claim state machine + derived views

Implements spec §Ledger (fold; derived states `open{bare→evidenced→verified}·grey·closed·dead·expired-bare`; grey = hold or starvation across ≥2 boundaries; `standing`). This is the pure heart — golden-tested from hand-built vectors.

**Files:**
- Modify: `src/state.rs`
- Test: `tests/fold.rs`

- [ ] **Step 1: Write the failing test**

`tests/fold.rs`:
```rust
use evolving::ledger::{Actor, ActorKind, Envelope};
use evolving::state::{fold, ClaimState};

fn env(seq: u64, id: &str, etype: &str, body: serde_json::Value) -> Envelope {
    Envelope {
        v: 2,
        id: id.into(),
        ts: format!("2020-01-01T00:00:{:02}Z", seq),
        writer: "w-0000".into(),
        seq,
        actor: Actor { kind: ActorKind::Human, id: None, via: None },
        etype: etype.into(),
        body,
    }
}

#[test]
fn a_bare_claim_folds_to_bare() {
    let events = vec![env(1, "clm_a", "claim", serde_json::json!({"label":"fixed X"}))];
    let d = fold(&events);
    assert_eq!(d.claims.len(), 1);
    assert!(matches!(d.claims[0].state, ClaimState::Bare));
    assert_eq!(d.claims[0].label, "fixed X");
}

#[test]
fn evidence_moves_a_claim_out_of_bare() {
    let events = vec![
        env(1, "clm_a", "claim", serde_json::json!({"label":"fixed X"})),
        env(2, "evd_a", "evidence", serde_json::json!({"claim":"clm_a","ref":"commit:abc","status":"verified","self_evident":false})),
    ];
    let d = fold(&events);
    assert!(matches!(d.claims[0].state, ClaimState::Verified));
    assert_eq!(d.claims[0].evidence.len(), 1);
    assert!(!d.claims[0].self_evident);
}

#[test]
fn a_closed_claim_leaves_the_open_list() {
    let events = vec![
        env(1, "clm_a", "claim", serde_json::json!({"label":"x"})),
        env(2, "evd_a", "evidence", serde_json::json!({"claim":"clm_a","ref":"commit:abc","status":"verified"})),
        env(3, "cls_a", "close", serde_json::json!({"claim":"clm_a"})),
    ];
    let d = fold(&events);
    assert_eq!(d.claims.len(), 0);
    assert_eq!(d.closed.len(), 1);
}

#[test]
fn a_held_claim_is_grey() {
    let events = vec![
        env(1, "clm_a", "claim", serde_json::json!({"label":"x"})),
        env(2, "hld_a", "hold", serde_json::json!({"claim":"clm_a","reason":"waiting on upstream"})),
    ];
    let d = fold(&events);
    assert_eq!(d.grey.len(), 1);
    assert_eq!(d.grey[0].reason.as_deref(), Some("waiting on upstream"));
}

#[test]
fn a_demanded_claim_that_gains_evidence_is_a_returned_demand() {
    let events = vec![
        env(1, "clm_a", "claim", serde_json::json!({"label":"x"})),
        env(2, "dmd_a", "demand", serde_json::json!({"claim":"clm_a"})),
        env(3, "evd_a", "evidence", serde_json::json!({"claim":"clm_a","ref":"commit:abc","status":"verified"})),
    ];
    let d = fold(&events);
    assert_eq!(d.demands_returned.len(), 1);
    assert_eq!(d.demands_returned[0].id, "clm_a");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test fold`
Expected: FAIL — `fold`/`ClaimState` not defined.

- [ ] **Step 3: Write minimal implementation**

`src/state.rs`:
```rust
use crate::ledger::Envelope;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ClaimState {
    Bare,
    Evidenced,
    Verified,
    Grey,
    Closed,
    Dead,
    ExpiredBare,
}

#[derive(Serialize, Clone, Debug)]
pub struct EvidenceView {
    pub eref: String,
    pub status: String, // "verified" | "failed" | "unreachable" | "recorded"
    pub self_evident: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct ClaimView {
    pub id: String,
    pub label: String,
    pub state: ClaimState,
    pub evidence: Vec<EvidenceView>,
    pub self_evident: bool,
    pub boundaries_open: u32,
    pub referenced_by: u32,
    pub source_ref: Option<String>,
    pub reason: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct ThoughtView {
    pub id: String,
    pub label: String,
    pub pinned: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct IndicatorView {
    pub id: String,
    pub name: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct Snapshot {
    pub id: String,
    pub ts: String,
    pub closed_with_evidence: u32,
    pub expired_bare: u32,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Derived {
    pub claims: Vec<ClaimView>,
    pub closed: Vec<ClaimView>,
    pub grey: Vec<ClaimView>,
    pub thoughts: Vec<ThoughtView>,
    pub demands_returned: Vec<ClaimView>,
    pub indicators: Vec<IndicatorView>,
    pub snapshots: Vec<Snapshot>,
    pub last_event_id: Option<String>,
    pub boundary_count: u32,
}

// internal accumulator per claim during the fold
struct ClaimAcc {
    id: String,
    label: String,
    source_ref: Option<String>,
    evidence: Vec<EvidenceView>,
    order: u64,
    held: Option<String>,
    closed: bool,
    dead: bool,
    demanded_at: Option<u64>,
    last_activity_seq: u64,
    referenced_by: u32,
}

fn s(v: &serde_json::Value, k: &str) -> Option<String> {
    v.get(k).and_then(|x| x.as_str()).map(|x| x.to_string())
}

pub fn fold(events: &[Envelope]) -> Derived {
    let mut claims: HashMap<String, ClaimAcc> = HashMap::new();
    let mut order_seq = 0u64;
    let mut thoughts: Vec<ThoughtView> = Vec::new();
    let mut indicators: Vec<IndicatorView> = Vec::new();
    let mut snapshots: Vec<Snapshot> = Vec::new();
    let mut boundary_count = 0u32;
    let mut last_event_id = None;

    for e in events {
        last_event_id = Some(e.id.clone());
        match e.etype.as_str() {
            "claim" => {
                order_seq += 1;
                let id = e.id.clone();
                claims.entry(id.clone()).or_insert(ClaimAcc {
                    id,
                    label: s(&e.body, "label").unwrap_or_default(),
                    source_ref: s(&e.body, "source_ref"),
                    evidence: vec![],
                    order: order_seq,
                    held: None,
                    closed: false,
                    dead: false,
                    demanded_at: None,
                    last_activity_seq: e.seq,
                    referenced_by: 0,
                });
            }
            "evidence" => {
                if let Some(cid) = s(&e.body, "claim") {
                    if let Some(acc) = claims.get_mut(&cid) {
                        acc.evidence.push(EvidenceView {
                            eref: s(&e.body, "ref").unwrap_or_default(),
                            status: s(&e.body, "status").unwrap_or_else(|| "recorded".into()),
                            self_evident: e.body.get("self_evident").and_then(|b| b.as_bool()).unwrap_or(false),
                        });
                        acc.held = None; // evidence revives a grey/held claim
                        acc.last_activity_seq = e.seq;
                    }
                }
            }
            "verify" => {
                if let (Some(cid), Some(st)) = (s(&e.body, "claim"), s(&e.body, "status")) {
                    if let Some(acc) = claims.get_mut(&cid) {
                        if let Some(last) = acc.evidence.last_mut() {
                            last.status = st;
                        }
                        acc.last_activity_seq = e.seq;
                    }
                }
            }
            "hold" => {
                if let Some(cid) = s(&e.body, "claim") {
                    if let Some(acc) = claims.get_mut(&cid) {
                        acc.held = Some(s(&e.body, "reason").unwrap_or_default());
                        acc.last_activity_seq = e.seq;
                    }
                }
            }
            "close" => {
                if let Some(cid) = s(&e.body, "claim") {
                    if let Some(acc) = claims.get_mut(&cid) {
                        acc.closed = true;
                        acc.last_activity_seq = e.seq;
                    }
                }
            }
            "prune" => {
                if let Some(cid) = s(&e.body, "claim") {
                    if let Some(acc) = claims.get_mut(&cid) {
                        acc.dead = true;
                    }
                }
            }
            "demand" => {
                if let Some(cid) = s(&e.body, "claim") {
                    if let Some(acc) = claims.get_mut(&cid) {
                        acc.demanded_at = Some(e.seq);
                    }
                }
            }
            "thought" => {
                thoughts.push(ThoughtView {
                    id: e.id.clone(),
                    label: s(&e.body, "label").unwrap_or_default(),
                    pinned: e.body.get("pinned").and_then(|b| b.as_bool()).unwrap_or(false),
                });
            }
            "indicator" => {
                indicators.push(IndicatorView {
                    id: e.id.clone(),
                    name: s(&e.body, "name").unwrap_or_default(),
                });
            }
            "snapshot" => {
                snapshots.push(Snapshot {
                    id: e.id.clone(),
                    ts: e.ts.clone(),
                    closed_with_evidence: e.body.get("closed_with_evidence").and_then(|n| n.as_u64()).unwrap_or(0) as u32,
                    expired_bare: e.body.get("expired_bare").and_then(|n| n.as_u64()).unwrap_or(0) as u32,
                });
            }
            "pause" if e.body.get("boundary").and_then(|b| b.as_bool()).unwrap_or(false) => {
                boundary_count += 1;
            }
            _ => {}
        }
    }

    let mut accs: Vec<ClaimAcc> = claims.into_values().collect();
    accs.sort_by_key(|a| a.order);

    let mut out = Derived {
        boundary_count,
        last_event_id,
        thoughts,
        indicators,
        snapshots,
        ..Default::default()
    };

    for a in accs {
        let boundaries_open = boundaries_since(&a, boundary_count);
        let state = derive_state(&a, boundaries_open);
        let view = ClaimView {
            id: a.id.clone(),
            label: a.label.clone(),
            state,
            self_evident: a.evidence.iter().any(|e| e.self_evident) && a.evidence.iter().all(|e| e.self_evident),
            evidence: a.evidence.clone(),
            boundaries_open,
            referenced_by: a.referenced_by,
            source_ref: a.source_ref.clone(),
            reason: a.held.clone(),
        };
        match state {
            ClaimState::Closed => out.closed.push(view),
            ClaimState::Dead => out.closed.push(view),
            ClaimState::Grey => out.grey.push(view.clone()),
            _ => {}
        }
        // returned demand: had a demand, then later evidence
        if a.demanded_at.is_some() && !a.evidence.is_empty() && !a.closed && !a.dead {
            out.demands_returned.push(view.clone());
        }
        if !a.closed && !a.dead {
            out.claims.push(view);
        }
    }
    out
}

fn boundaries_since(a: &ClaimAcc, _total: u32) -> u32 {
    // Plan #1: a claim's boundaries_open is filled by the pause when it stamps a
    // boundary. Until snapshots exist, this is 0. Kept as a function so the
    // starvation rule has one home.
    let _ = a;
    0
}

fn derive_state(a: &ClaimAcc, boundaries_open: u32) -> ClaimState {
    if a.dead {
        return ClaimState::Dead;
    }
    if a.closed {
        return ClaimState::Closed;
    }
    if a.held.is_some() {
        return ClaimState::Grey;
    }
    if a.evidence.is_empty() {
        if boundaries_open >= 2 {
            return ClaimState::ExpiredBare;
        }
        return ClaimState::Bare;
    }
    if a.evidence.iter().any(|e| e.status == "verified") {
        ClaimState::Verified
    } else {
        ClaimState::Evidenced
    }
}
```

Note: `boundaries_open`/starvation-driven grey is wired to real boundary counts in Task 11 (snapshots). The state machine and its tests are complete now; the boundary input is 0 until snapshots exist, which is correct for a fresh ledger.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test fold && cargo clippy -- -D warnings`
Expected: PASS — 5 tests green.

- [ ] **Step 5: Commit**

```bash
git add src/state.rs tests/fold.rs
git commit -m "state: the fold — claim state machine and derived views"
```

---

### Task 6: `think` + `claim` verbs with source_ref idempotency

Implements spec §Verbs (`think`; `claim` with `--evidence`, `--by agent`, `source_ref` idempotency key).

**Files:**
- Modify: `src/cmd.rs`, `src/main.rs`
- Test: `tests/claim.rs`

- [ ] **Step 1: Write the failing test**

`tests/claim.rs`:
```rust
use std::process::Command;

fn run(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ev")).args(args).current_dir(dir).output().unwrap()
}
fn fresh() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ev-claim-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    assert!(run(&dir, &["init"]).status.success());
    dir
}

#[test]
fn a_bare_claim_appears_in_brief() {
    let dir = fresh();
    assert!(run(&dir, &["claim", "fixed the parser"]).status.success());
    let out = run(&dir, &["brief", "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let labels: Vec<&str> = v["open"].as_array().unwrap().iter().map(|c| c["label"].as_str().unwrap()).collect();
    assert!(labels.contains(&"fixed the parser"), "{v}");
}

#[test]
fn same_source_ref_is_idempotent() {
    let dir = fresh();
    assert!(run(&dir, &["claim", "did a thing", "--source-ref", "sess-1"]).status.success());
    assert!(run(&dir, &["claim", "did a thing", "--source-ref", "sess-1"]).status.success());
    let out = run(&dir, &["brief", "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["open"].as_array().unwrap().len(), 1, "duplicate source_ref must not create a second claim");
}
```

(The `brief --json` these depend on is delivered in Task 9; sequence Task 6 → 9, but write these tests now and let Step 4 run them after Task 9. To keep this task self-contained, Step 4 asserts on the ledger file directly instead — see below.)

Self-contained Step-1 assertion (replace the two tests above with these so Task 6 is testable before `brief` exists):
```rust
#[test]
fn a_claim_writes_one_claim_event() {
    let dir = fresh();
    assert!(run(&dir, &["claim", "fixed the parser"]).status.success());
    let wid = std::fs::read_to_string(dir.join(".evolving/local/writer.toml")).unwrap();
    let wid = wid.split('"').nth(1).unwrap();
    let log = std::fs::read_to_string(dir.join(".evolving/ledger").join(format!("{wid}.jsonl"))).unwrap();
    assert_eq!(log.lines().filter(|l| l.contains("\"type\":\"claim\"")).count(), 1);
    assert!(log.contains("fixed the parser"));
}

#[test]
fn same_source_ref_files_only_once() {
    let dir = fresh();
    assert!(run(&dir, &["claim", "did a thing", "--source-ref", "sess-1"]).status.success());
    let second = run(&dir, &["claim", "did a thing", "--source-ref", "sess-1"]);
    assert!(second.status.success());
    let wid = std::fs::read_to_string(dir.join(".evolving/local/writer.toml")).unwrap();
    let wid = wid.split('"').nth(1).unwrap();
    let log = std::fs::read_to_string(dir.join(".evolving/ledger").join(format!("{wid}.jsonl"))).unwrap();
    assert_eq!(log.lines().filter(|l| l.contains("\"type\":\"claim\"")).count(), 1);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test claim`
Expected: FAIL — no `claim`/`think` subcommands.

- [ ] **Step 3: Write minimal implementation**

Append to `src/cmd.rs`:
```rust
use crate::ledger::{Actor, ActorKind, Ledger, NewEvent};
use crate::state::fold;

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
            e.etype == "claim" && e.body.get("source_ref").and_then(|s| s.as_str()) == Some(sref.as_str())
        });
        if exists {
            println!("claim already filed for source_ref {sref} (idempotent).");
            return Ok(());
        }
    }

    let actor = if args.by_agent {
        Actor { kind: ActorKind::Agent, id: agent_id(), via: None }
    } else {
        Actor { kind: ActorKind::Human, id: None, via: None }
    };
    let mut body = serde_json::json!({ "label": args.label });
    if let Some(sref) = &args.source_ref {
        body["source_ref"] = serde_json::json!(sref);
    }
    let mut batch = vec![NewEvent { etype: "claim".into(), actor: actor.clone(), body }];

    // an inline --evidence attaches an evidence event referencing the just-minted claim.
    // Because the batch is one atomic write, we mint the claim id first, then reference it.
    let minted = ledger.append_batch(batch.drain(..).collect())?;
    if let Some(eref) = &args.evidence {
        let claim_id = &minted[0].id;
        let verdict = crate::verify::verify_and_record(&ledger, &root, claim_id, eref, false, actor)?;
        println!("claim {} · evidence {} → {}", short(claim_id), eref, verdict);
    } else {
        println!("claim {} (bare — needs evidence to close)", short(&minted[0].id));
    }
    Ok(())
}

pub fn think(label: String, pinned: bool) -> Result<()> {
    let root = find_root();
    let ledger = Ledger::open(&root)?;
    let actor = Actor { kind: ActorKind::Human, id: None, via: None };
    ledger.append_batch(vec![NewEvent {
        etype: "thought".into(),
        actor,
        body: serde_json::json!({ "label": label, "pinned": pinned }),
    }])?;
    println!("noted.");
    Ok(())
}

pub fn short(id: &str) -> String {
    // print the prefix + first 6 of the ULID
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

// used by several verbs
pub fn load_derived(ledger: &Ledger) -> Result<crate::state::Derived> {
    Ok(fold(&ledger.scan()?))
}
```

Add to `src/main.rs` the `Think` and `Claim` variants + dispatch:
```rust
    /// Note a thought (optionally pinned).
    Think { label: String, #[arg(long)] pin: bool },
    /// File a claim. Bare unless --evidence is given.
    Claim {
        label: String,
        #[arg(long)] evidence: Option<String>,
        #[arg(long = "by", value_parser = ["agent","human"], default_value = "human")] by: String,
        #[arg(long = "source-ref")] source_ref: Option<String>,
    },
```
```rust
        Some(Command::Think { label, pin }) => evolving::cmd::think(label, pin),
        Some(Command::Claim { label, evidence, by, source_ref }) =>
            evolving::cmd::claim(evolving::cmd::ClaimArgs {
                label, evidence, by_agent: by == "agent", source_ref,
            }),
```

Note: `verify::verify_and_record` is delivered in Task 7; for this task, stub it in `verify.rs` to return `Status::Recorded` so `--evidence` compiles, and Task 7 replaces the body. Add to `src/verify.rs`:
```rust
// stub replaced in Task 7
```
(Implementer: if building strictly task-by-task, gate the `--evidence` branch behind Task 7 by having Step-1 tests here use only bare claims — the two tests above do. Wire `--evidence` fully in Task 7.)

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test claim && cargo clippy -- -D warnings`
Expected: PASS — 2 tests green.

- [ ] **Step 5: Commit**

```bash
git add src/cmd.rs src/main.rs src/verify.rs tests/claim.rs
git commit -m "verbs: think and claim with source_ref idempotency"
```

---

### Task 7: Evidence refs + V1 (commit) verifier + `evidence`/`verify` verbs

Implements spec §Evidence-and-verification (typed refs; V1 `git rev-parse`; statuses; each verify appends a `verify` event) and §Verbs (`evidence`, `verify`).

**Files:**
- Modify: `src/verify.rs`, `src/cmd.rs`, `src/main.rs`
- Test: `tests/verify_commit.rs`

- [ ] **Step 1: Write the failing test**

`tests/verify_commit.rs`:
```rust
use evolving::verify::{EvRef, RefKind};

#[test]
fn parses_a_commit_ref() {
    let r = EvRef::parse("commit:deadbeef").unwrap();
    assert!(matches!(r.kind, RefKind::Commit));
    assert_eq!(r.payload, "deadbeef");
}

#[test]
fn parses_a_test_ref_with_a_passline() {
    let r = EvRef::parse("test:target/t.log::test_foo ... ok").unwrap();
    assert!(matches!(r.kind, RefKind::Test));
    assert_eq!(r.payload, "target/t.log");
    assert_eq!(r.passline.as_deref(), Some("test_foo ... ok"));
}

#[test]
fn a_metric_ref_is_recorded_only() {
    let r = EvRef::parse("metric:466 calls/sec").unwrap();
    assert!(matches!(r.kind, RefKind::Metric));
}
```

Plus a CLI-level test `tests/evidence.rs`:
```rust
use std::process::Command;
fn run(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ev")).args(args).current_dir(dir).output().unwrap()
}
fn fresh_git() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ev-evd-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    for a in [&["init","-q"][..]] { Command::new("git").args(a).current_dir(&dir).output().unwrap(); }
    std::fs::write(dir.join("f.txt"), "x").unwrap();
    Command::new("git").args(["add","."]).current_dir(&dir).output().unwrap();
    Command::new("git").args(["-c","user.email=t@t","-c","user.name=t","commit","-qm","first"]).current_dir(&dir).output().unwrap();
    assert!(run(&dir, &["init"]).status.success());
    dir
}

#[test]
fn evidence_pointing_at_a_real_commit_verifies() {
    let dir = fresh_git();
    let head = String::from_utf8(Command::new("git").args(["rev-parse","HEAD"]).current_dir(&dir).output().unwrap().stdout).unwrap();
    let head = head.trim();
    assert!(run(&dir, &["claim", "did it", "--source-ref", "s1"]).status.success());
    // find the claim id from the ledger
    let out = run(&dir, &["brief", "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let cid = v["open"][0]["id"].as_str().unwrap().to_string();
    let out = run(&dir, &["evidence", &cid, &format!("commit:{head}")]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let out = run(&dir, &["brief", "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["open"][0]["state"].as_str().unwrap(), "verified");
}
```

(Depends on `brief --json` from Task 9 — run this file's assertions at Step 4 of Task 9; the parser tests in `verify_commit.rs` run now.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test verify_commit`
Expected: FAIL — `EvRef` not defined.

- [ ] **Step 3: Write minimal implementation**

`src/verify.rs` (replace stub):
```rust
use crate::ledger::{Actor, Ledger, NewEvent};
use crate::{EvError, Result};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefKind {
    Commit,
    Test,
    File,
    Artifact,
    Metric,
    Url,
}

#[derive(Debug, Clone)]
pub struct EvRef {
    pub kind: RefKind,
    pub payload: String,
    pub passline: Option<String>,
}

impl EvRef {
    pub fn parse(raw: &str) -> Result<EvRef> {
        let (scheme, rest) = raw
            .split_once(':')
            .ok_or_else(|| EvError::Refusal(format!("ref must be typed (commit:/test:/file:/artifact:/metric:/url:): {raw}")))?;
        let kind = match scheme {
            "commit" => RefKind::Commit,
            "test" => RefKind::Test,
            "file" => RefKind::File,
            "artifact" => RefKind::Artifact,
            "metric" => RefKind::Metric,
            "url" => RefKind::Url,
            other => return Err(EvError::Refusal(format!("unknown ref type: {other}:"))),
        };
        // test:/file:/artifact: may carry a "::passline" match target
        let (payload, passline) = match kind {
            RefKind::Test | RefKind::File | RefKind::Artifact => match rest.split_once("::") {
                Some((p, line)) => (p.to_string(), Some(line.to_string())),
                None => (rest.to_string(), None),
            },
            _ => (rest.to_string(), None),
        };
        Ok(EvRef { kind, payload, passline })
    }
}

pub fn status_str(raw: &str) -> &'static str {
    match raw {
        "verified" => "verified",
        "failed" => "failed",
        "unreachable" => "unreachable",
        _ => "recorded",
    }
}

/// Verify a ref against `repo_root`. V1 for commits; V2 for test/file/artifact
/// (delivered in Task 8 — here commit + recorded-only). Never touches the network.
pub fn verify_ref(r: &EvRef, repo_root: &Path) -> String {
    match r.kind {
        RefKind::Commit => verify_commit(&r.payload, repo_root),
        RefKind::Metric | RefKind::Url => "recorded".into(),
        // V2 kinds are wired in Task 8; until then, unreachable is the honest default.
        _ => "unreachable".into(),
    }
}

fn verify_commit(sha: &str, repo_root: &Path) -> String {
    let out = Command::new("git")
        .args(["rev-parse", "--verify", "--quiet", &format!("{sha}^{{commit}}")])
        .current_dir(repo_root)
        .output();
    match out {
        Ok(o) if o.status.success() => "verified".into(),
        Ok(_) => "failed".into(),
        Err(_) => "unreachable".into(),
    }
}

/// Attach evidence to a claim and record a verify verdict, in one atomic batch.
pub fn verify_and_record(
    ledger: &Ledger,
    repo_root: &Path,
    claim_id: &str,
    raw_ref: &str,
    self_evident: bool,
    actor: Actor,
) -> Result<String> {
    let r = EvRef::parse(raw_ref)?;
    let status = verify_ref(&r, repo_root);
    ledger.append_batch(vec![NewEvent {
        etype: "evidence".into(),
        actor,
        body: serde_json::json!({
            "claim": claim_id,
            "ref": raw_ref,
            "status": status,
            "self_evident": self_evident,
        }),
    }])?;
    Ok(status)
}
```

`src/cmd.rs` — add `evidence` and `verify` verbs:
```rust
pub fn evidence(claim_id: String, eref: String) -> Result<()> {
    let root = find_root();
    let ledger = Ledger::open(&root)?;
    let full = resolve_id(&ledger, &claim_id)?;
    let actor = evidence_actor();
    let verdict = crate::verify::verify_and_record(&ledger, &root, &full, &eref, false, actor)?;
    println!("evidence attached to {} → {verdict}", short(&full));
    Ok(())
}

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
                    actor: Actor { kind: ActorKind::Engine, id: None, via: None },
                    body: serde_json::json!({ "claim": c.id, "ref": ev.eref, "status": status }),
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
    let matches: Vec<&str> = events.iter().map(|e| e.id.as_str()).filter(|id| id.starts_with(prefix)).collect();
    match matches.len() {
        1 => Ok(matches[0].to_string()),
        0 => Err(EvError::Refusal(format!("no event matches id {prefix}"))),
        _ => Err(EvError::Refusal(format!("ambiguous id {prefix} — {} matches", matches.len()))),
    }
}

fn evidence_actor() -> Actor {
    // evidence is creation-only; agents are permitted. Provenance is recorded.
    if std::env::var("CLAUDECODE").is_ok() {
        Actor { kind: ActorKind::Agent, id: Some("claude-code".into()), via: None }
    } else {
        Actor { kind: ActorKind::Human, id: None, via: None }
    }
}
```

Add to `src/main.rs`:
```rust
    /// Attach evidence to a claim (typed ref). Agents may do this.
    Evidence { claim: String, evidence_ref: String },
    /// Re-verify a claim's evidence (or all open claims).
    Verify { claim: Option<String> },
```
```rust
        Some(Command::Evidence { claim, evidence_ref }) => evolving::cmd::evidence(claim, evidence_ref),
        Some(Command::Verify { claim }) => evolving::cmd::verify_cmd(claim),
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test verify_commit && cargo clippy -- -D warnings`
Expected: PASS — parser tests green. (The CLI `evidence.rs` test runs green after Task 9.)

- [ ] **Step 5: Commit**

```bash
git add src/verify.rs src/cmd.rs src/main.rs tests/verify_commit.rs tests/evidence.rs
git commit -m "verify: typed refs, commit verifier, evidence and verify verbs"
```

---

### Task 8: V2 verifier (file/test/artifact) + self_evident + transcript archival

Implements spec §Evidence-and-verification (V2 exists→sha256→pass-line; `self_evident` ⊙ vs ✓; artifact archival).

**Files:**
- Modify: `src/verify.rs`
- Test: `tests/verify_v2.rs`

- [ ] **Step 1: Write the failing test**

`tests/verify_v2.rs`:
```rust
use evolving::verify::{verify_ref, EvRef};
use std::fs;

fn tmp() -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("ev-v2-{}", ulid::Ulid::new()));
    fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn an_existing_file_verifies() {
    let d = tmp();
    fs::write(d.join("out.log"), "all good\n").unwrap();
    let r = EvRef::parse("file:out.log").unwrap();
    assert_eq!(verify_ref(&r, &d), "verified");
}

#[test]
fn a_missing_file_is_unreachable() {
    let d = tmp();
    let r = EvRef::parse("file:nope.log").unwrap();
    assert_eq!(verify_ref(&r, &d), "unreachable");
}

#[test]
fn a_passline_that_matches_verifies_and_one_that_misses_fails() {
    let d = tmp();
    fs::write(d.join("t.log"), "running\ntest_foo ... ok\ndone\n").unwrap();
    let hit = EvRef::parse("test:t.log::test_foo ... ok").unwrap();
    assert_eq!(verify_ref(&hit, &d), "verified");
    let miss = EvRef::parse("test:t.log::test_bar ... ok").unwrap();
    assert_eq!(verify_ref(&miss, &d), "failed");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test verify_v2`
Expected: FAIL — V2 kinds currently return `unreachable` unconditionally.

- [ ] **Step 3: Write minimal implementation**

In `src/verify.rs`, replace the `_ => "unreachable".into()` arm of `verify_ref` with a call to `verify_v2`, and add:
```rust
fn verify_v2(r: &EvRef, repo_root: &Path) -> String {
    let path = if r.kind == RefKind::Artifact {
        repo_root.join(".evolving/artifacts").join(&r.payload)
    } else {
        repo_root.join(&r.payload)
    };
    if !path.exists() {
        return "unreachable".into();
    }
    // existence is established; hash it (proves readability), then the pass-line.
    let content = match std::fs::read(&path) {
        Ok(c) => c,
        Err(_) => return "unreachable".into(),
    };
    let _digest = {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(&content);
        format!("{:x}", h.finalize())
    };
    match &r.passline {
        None => "verified".into(),
        Some(pattern) => {
            let text = String::from_utf8_lossy(&content);
            if text.lines().any(|l| l.contains(pattern.as_str())) {
                "verified".into()
            } else {
                "failed".into()
            }
        }
    }
}

/// Copy a matched pass-line region (±20 lines) into `.evolving/artifacts/` and
/// return the artifact ref that replaces a fragile transcript ref. Used by exhaust.
pub fn archive_region(repo_root: &Path, source: &Path, pattern: &str) -> Result<Option<String>> {
    let text = std::fs::read_to_string(source).map_err(|e| EvError::Failure(e.to_string()))?;
    let lines: Vec<&str> = text.lines().collect();
    let Some(hit) = lines.iter().position(|l| l.contains(pattern)) else {
        return Ok(None);
    };
    let lo = hit.saturating_sub(20);
    let hi = (hit + 21).min(lines.len());
    let region = lines[lo..hi].join("\n");
    let name = format!("region-{}.txt", ulid::Ulid::new());
    let dir = repo_root.join(".evolving/artifacts");
    std::fs::create_dir_all(&dir).map_err(|e| EvError::Failure(e.to_string()))?;
    std::fs::write(dir.join(&name), region).map_err(|e| EvError::Failure(e.to_string()))?;
    Ok(Some(format!("artifact:{name}::{pattern}")))
}
```

Change the match in `verify_ref`:
```rust
    match r.kind {
        RefKind::Commit => verify_commit(&r.payload, repo_root),
        RefKind::Metric | RefKind::Url => "recorded".into(),
        RefKind::Test | RefKind::File | RefKind::Artifact => verify_v2(r, repo_root),
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test verify_v2 && cargo clippy -- -D warnings`
Expected: PASS — 3 tests green.

- [ ] **Step 5: Commit**

```bash
git add src/verify.rs tests/verify_v2.rs
git commit -m "verify: V2 file/test/artifact pass-line verifier and region archival"
```

---

### Task 9: `close`/`hold`/`demand` verbs + bare-close sting + CLAUDECODE guard + `brief`

Implements spec §Verbs (`close` bare-refusal + `--dead`; `hold`; `demand`; CLAUDECODE guard + `--i-am-the-human`; `brief` ≤2KB + `--json`) and §Hooks-exhaust-sweep (brief ordering) and the "as of event" footer.

**Files:**
- Modify: `src/cmd.rs`, `src/render.rs`, `src/main.rs`
- Test: `tests/close.rs`, `tests/brief.rs`

- [ ] **Step 1: Write the failing test**

`tests/close.rs`:
```rust
use std::process::Command;
fn run(dir: &std::path::Path, envs: &[(&str,&str)], args: &[&str]) -> std::process::Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_ev"));
    c.args(args).current_dir(dir);
    for (k,v) in envs { c.env(k, v); }
    c.output().unwrap()
}
fn fresh() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ev-close-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    assert!(run(&dir, &[], &["init"]).status.success());
    dir
}
fn only_claim_id(dir: &std::path::Path) -> String {
    let out = run(dir, &[], &["brief","--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    v["open"][0]["id"].as_str().unwrap().to_string()
}

#[test]
fn closing_a_bare_claim_is_refused_with_the_sting() {
    let dir = fresh();
    assert!(run(&dir, &[], &["claim","fixed it"]).status.success());
    let cid = only_claim_id(&dir);
    let out = run(&dir, &[], &["close", &cid]);
    assert_eq!(out.status.code(), Some(1), "bare close must exit 1");
    let msg = String::from_utf8_lossy(&out.stderr);
    assert!(msg.contains("evidence"), "sting should name the missing evidence: {msg}");
}

#[test]
fn a_dead_close_needs_a_reason_and_succeeds() {
    let dir = fresh();
    assert!(run(&dir, &[], &["claim","abandon this"]).status.success());
    let cid = only_claim_id(&dir);
    let out = run(&dir, &[], &["close", &cid, "--dead", "--reason", "obsoleted by redesign"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn closure_verbs_refuse_under_claudecode_without_override() {
    let dir = fresh();
    assert!(run(&dir, &[], &["claim","x"]).status.success());
    let cid = only_claim_id(&dir);
    let out = run(&dir, &[("CLAUDECODE","1")], &["close", &cid, "--dead", "--reason", "y"]);
    assert_eq!(out.status.code(), Some(1), "must refuse under CLAUDECODE");
    let out = run(&dir, &[("CLAUDECODE","1")], &["close", &cid, "--dead", "--reason", "y", "--i-am-the-human"]);
    assert!(out.status.success(), "override should allow it");
}
```

`tests/brief.rs`:
```rust
use std::process::Command;
fn run(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ev")).args(args).current_dir(dir).output().unwrap()
}
fn fresh() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ev-brief-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    assert!(run(&dir, &["init"]).status.success());
    dir
}

#[test]
fn brief_json_lists_open_claims_and_a_footer_event_id() {
    let dir = fresh();
    assert!(run(&dir, &["claim","fix the thing"]).status.success());
    let out = run(&dir, &["brief","--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["open"].as_array().unwrap().len(), 1);
    assert!(v["as_of"].is_string(), "brief must carry the as-of event id");
}

#[test]
fn brief_text_stays_under_2kb() {
    let dir = fresh();
    for i in 0..10 { assert!(run(&dir, &["claim", &format!("claim number {i}")]).status.success()); }
    let out = run(&dir, &["brief"]);
    assert!(out.stdout.len() <= 2048, "brief was {} bytes", out.stdout.len());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test close --test brief`
Expected: FAIL — verbs/brief not defined.

- [ ] **Step 3: Write minimal implementation**

`src/cmd.rs` — the closure verbs + human guard:
```rust
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
    let view = d.claims.iter().find(|c| c.id == full)
        .ok_or_else(|| EvError::Refusal(format!("{} is not an open claim", short(&full))))?;

    if args.dead {
        let reason = args.reason.ok_or_else(|| EvError::Refusal("--dead needs --reason".into()))?;
        ledger.append_batch(vec![NewEvent {
            etype: "prune".into(),
            actor: Actor { kind: ActorKind::Human, id: None, via: None },
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
        actor: Actor { kind: ActorKind::Human, id: None, via: None },
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
        actor: Actor { kind: ActorKind::Human, id: None, via: None },
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
        actor: Actor { kind: ActorKind::Human, id: None, via: None },
        body: serde_json::json!({ "claim": full }),
    }])?;
    println!("demanded evidence for {}. It leads the next brief.", short(&full));
    Ok(())
}

pub fn brief(json: bool) -> Result<()> {
    let root = find_root();
    let ledger = Ledger::open(&root)?;
    let d = crate::state::fold(&ledger.scan()?);
    print!("{}", crate::render::brief(&d, json));
    Ok(())
}
```

`src/render.rs` — brief + footer (deterministic; keep under 2KB by capping lists):
```rust
use crate::state::{ClaimState, Derived};

pub const FOOTER: &str = "ev refreshes when invoked, not in the background.";

pub fn as_of(d: &Derived) -> String {
    d.last_event_id.clone().unwrap_or_else(|| "—".into())
}

pub fn brief(d: &Derived, json: bool) -> String {
    if json {
        let v = serde_json::json!({
            "demands_returned": ids(&d.demands_returned),
            "open": d.claims.iter().map(claim_json).collect::<Vec<_>>(),
            "grey": ids(&d.grey),
            "pinned": d.thoughts.iter().filter(|t| t.pinned).map(|t| &t.label).collect::<Vec<_>>(),
            "as_of": as_of(d),
        });
        return format!("{}\n", serde_json::to_string_pretty(&v).unwrap());
    }
    let mut out = String::new();
    if !d.demands_returned.is_empty() {
        out.push_str(&format!("↩ {} demand(s) answered — review at pause\n", d.demands_returned.len()));
    }
    let shown = d.claims.iter().take(12);
    out.push_str(&format!("open claims: {}\n", d.claims.len()));
    for c in shown {
        out.push_str(&format!("  {} {}  [{}]\n", mark(c.self_evident, &c.state), truncate(&c.label, 60), state_word(&c.state)));
    }
    if !d.grey.is_empty() {
        out.push_str(&format!("grey (held/starved): {}\n", d.grey.len()));
    }
    out.push_str(&format!("— as of {} · {}\n", short_id(&as_of(d)), FOOTER));
    out
}

fn claim_json(c: &crate::state::ClaimView) -> serde_json::Value {
    serde_json::json!({
        "id": c.id, "label": c.label, "state": state_word(&c.state),
        "self_evident": c.self_evident, "evidence": c.evidence.len(),
    })
}
fn ids(v: &[crate::state::ClaimView]) -> Vec<String> { v.iter().map(|c| c.id.clone()).collect() }

pub fn mark(self_evident: bool, state: &ClaimState) -> char {
    match state {
        ClaimState::Verified if self_evident => '⊙',
        ClaimState::Verified => '✓',
        ClaimState::Bare | ClaimState::ExpiredBare => '·',
        _ => '–',
    }
}
pub fn state_word(s: &ClaimState) -> &'static str {
    match s {
        ClaimState::Bare => "bare",
        ClaimState::Evidenced => "evidenced",
        ClaimState::Verified => "verified",
        ClaimState::Grey => "grey",
        ClaimState::Closed => "closed",
        ClaimState::Dead => "dead",
        ClaimState::ExpiredBare => "expired-bare",
    }
}
fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.to_string() } else { format!("{}…", s.chars().take(n - 1).collect::<String>()) }
}
fn short_id(id: &str) -> String {
    match id.split_once('_') { Some((p, r)) => format!("{p}_{}", &r[..r.len().min(6)]), None => id.to_string() }
}
```

Add to `src/main.rs` (variants + dispatch); note `--json` is global-ish but declared per read verb:
```rust
    /// The daily glance: returned demands, open claims, grey.
    Brief { #[arg(long)] json: bool },
    /// Close a claim (needs evidence, or --dead --reason).
    Close {
        claim: String,
        #[arg(long)] dead: bool,
        #[arg(long)] reason: Option<String>,
        #[arg(long = "i-am-the-human")] i_am_the_human: bool,
    },
    /// Move a claim to grey with a reason.
    Hold { claim: String, #[arg(long)] reason: String, #[arg(long = "i-am-the-human")] i_am_the_human: bool },
    /// Bounce a claim back for evidence (leads the next brief).
    Demand { claim: String, #[arg(long = "i-am-the-human")] i_am_the_human: bool },
```
```rust
        Some(Command::Brief { json }) => evolving::cmd::brief(json),
        Some(Command::Close { claim, dead, reason, i_am_the_human }) =>
            evolving::cmd::close(evolving::cmd::CloseArgs { claim, dead, reason, i_am_the_human }),
        Some(Command::Hold { claim, reason, i_am_the_human }) => evolving::cmd::hold(claim, reason, i_am_the_human),
        Some(Command::Demand { claim, i_am_the_human }) => evolving::cmd::demand(claim, i_am_the_human),
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test close --test brief --test claim --test evidence && cargo clippy -- -D warnings`
Expected: PASS — close/brief green, and the deferred claim/evidence CLI assertions now pass.

- [ ] **Step 5: Commit**

```bash
git add src/cmd.rs src/render.rs src/main.rs tests/close.rs tests/brief.rs
git commit -m "verbs: close/hold/demand with the bare-close sting, human guard, and brief"
```

---

### Task 10: Exhaust — git-window discovery, sweep, one-claim-per-session, the label rule

Implements spec §Hooks-exhaust-sweep (sweep is primary; git window; one claim per session; label rule; idempotent source_ref; self_evident) and §Verbs (`exhaust`).

**Files:**
- Modify: `src/exhaust.rs`, `src/cmd.rs`, `src/main.rs`
- Test: `tests/exhaust.rs`

- [ ] **Step 1: Write the failing test**

`tests/exhaust.rs`:
```rust
use std::process::Command;
fn git(dir: &std::path::Path, args: &[&str]) {
    Command::new("git").args(args).current_dir(dir)
        .env("GIT_AUTHOR_NAME","t").env("GIT_AUTHOR_EMAIL","t@t")
        .env("GIT_COMMITTER_NAME","t").env("GIT_COMMITTER_EMAIL","t@t")
        .output().unwrap();
}
fn run(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ev")).args(args).current_dir(dir).output().unwrap()
}
fn repo() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ev-exh-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init","-q"]);
    assert!(run(&dir, &["init"]).status.success());
    dir
}

#[test]
fn a_single_commit_window_files_one_self_evident_claim_labeled_by_the_subject() {
    let dir = repo();
    let start = String::from_utf8(Command::new("git").args(["rev-parse","HEAD"]).current_dir(&dir).output().unwrap().stdout).ok();
    let _ = start; // no commits yet
    std::fs::write(dir.join("a.txt"), "1").unwrap();
    git(&dir, &["add","."]);
    git(&dir, &["commit","-qm","tighten the redaction boundary"]);
    // exhaust the window HEAD~1..HEAD (here: the root commit)
    let out = run(&dir, &["exhaust", "--since", "ROOT", "--session", "sess-42"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let b = run(&dir, &["brief","--json"]);
    let v: serde_json::Value = serde_json::from_slice(&b.stdout).unwrap();
    assert_eq!(v["open"][0]["label"].as_str().unwrap(), "tighten the redaction boundary");
    assert_eq!(v["open"][0]["self_evident"].as_bool().unwrap(), true);
}

#[test]
fn exhausting_the_same_session_twice_is_idempotent() {
    let dir = repo();
    std::fs::write(dir.join("a.txt"), "1").unwrap();
    git(&dir, &["add","."]); git(&dir, &["commit","-qm","one"]);
    assert!(run(&dir, &["exhaust","--since","ROOT","--session","s1"]).status.success());
    assert!(run(&dir, &["exhaust","--since","ROOT","--session","s1"]).status.success());
    let b = run(&dir, &["brief","--json"]);
    let v: serde_json::Value = serde_json::from_slice(&b.stdout).unwrap();
    assert_eq!(v["open"].as_array().unwrap().len(), 1, "same session must not double-file");
}

#[test]
fn an_empty_window_files_nothing() {
    let dir = repo();
    std::fs::write(dir.join("a.txt"), "1").unwrap();
    git(&dir, &["add","."]); git(&dir, &["commit","-qm","one"]);
    let head = String::from_utf8(Command::new("git").args(["rev-parse","HEAD"]).current_dir(&dir).output().unwrap().stdout).unwrap();
    let out = run(&dir, &["exhaust","--since", head.trim(), "--session","s-empty"]);
    assert!(out.status.success());
    let b = run(&dir, &["brief","--json"]);
    let v: serde_json::Value = serde_json::from_slice(&b.stdout).unwrap();
    assert_eq!(v["open"].as_array().unwrap().len(), 0);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test exhaust`
Expected: FAIL — no `exhaust` subcommand.

- [ ] **Step 3: Write minimal implementation**

`src/exhaust.rs`:
```rust
use crate::ledger::{Actor, ActorKind, Ledger, NewEvent};
use crate::{EvError, Result};
use std::path::Path;
use std::process::Command;

pub struct Window {
    pub session: String,
    pub shas: Vec<String>,
    pub subjects: Vec<String>,
    pub branch: String,
}

/// Discover commits in (since, HEAD]. `since == "ROOT"` means the whole history.
pub fn discover(repo_root: &Path, since: &str, session: &str) -> Result<Window> {
    let range = if since == "ROOT" { "HEAD".to_string() } else { format!("{since}..HEAD") };
    let log = Command::new("git")
        .args(["log", "--format=%H%x1f%s", &range])
        .current_dir(repo_root)
        .output()
        .map_err(|e| EvError::Failure(e.to_string()))?;
    let mut shas = Vec::new();
    let mut subjects = Vec::new();
    for line in String::from_utf8_lossy(&log.stdout).lines() {
        if let Some((h, s)) = line.split_once('\u{1f}') {
            shas.push(h.to_string());
            subjects.push(s.to_string());
        }
    }
    let branch = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(repo_root)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "HEAD".into());
    Ok(Window { session: session.to_string(), shas, subjects, branch })
}

/// The label rule: the commit subject when the window carries exactly one commit;
/// otherwise the first non-boilerplate summary line; else a shas-count fallback.
pub fn label(w: &Window, closing_summary: Option<&str>) -> String {
    if w.shas.len() == 1 {
        return w.subjects[0].clone();
    }
    if let Some(summary) = closing_summary {
        for line in summary.lines() {
            let t = line.trim();
            if t.is_empty() || is_boilerplate(t) {
                continue;
            }
            return t.to_string();
        }
    }
    format!("session {}: {} commits on {}", short_session(&w.session), w.shas.len(), w.branch)
}

fn is_boilerplate(line: &str) -> bool {
    let l = line.to_lowercase();
    l.starts_with("round ") && l.contains("complete")
        || l == "done."
        || l.starts_with("session complete")
}

fn short_session(s: &str) -> String {
    s.chars().take(8).collect()
}

/// File one claim for a session window, with all shas as self-evident evidence.
/// Idempotent on the session id (used as source_ref).
pub fn file_window(ledger: &Ledger, repo_root: &Path, w: &Window, closing_summary: Option<&str>) -> Result<Option<String>> {
    if w.shas.is_empty() {
        return Ok(None);
    }
    let source_ref = format!("session:{}", w.session);
    let events = ledger.scan()?;
    if events.iter().any(|e| {
        e.etype == "claim" && e.body.get("source_ref").and_then(|s| s.as_str()) == Some(source_ref.as_str())
    }) {
        return Ok(None);
    }
    let actor = Actor { kind: ActorKind::Agent, id: Some("exhaust".into()), via: None };
    let minted = ledger.append_batch(vec![NewEvent {
        etype: "claim".into(),
        actor: actor.clone(),
        body: serde_json::json!({ "label": label(w, closing_summary), "source_ref": source_ref }),
    }])?;
    let claim_id = minted[0].id.clone();
    // one evidence event per sha, all self_evident (verified against this repo)
    let mut batch = Vec::new();
    for sha in &w.shas {
        let status = crate::verify::verify_ref(
            &crate::verify::EvRef::parse(&format!("commit:{sha}"))?,
            repo_root,
        );
        batch.push(NewEvent {
            etype: "evidence".into(),
            actor: actor.clone(),
            body: serde_json::json!({
                "claim": claim_id, "ref": format!("commit:{sha}"),
                "status": status, "self_evident": true,
            }),
        });
    }
    ledger.append_batch(batch)?;
    Ok(Some(claim_id))
}
```

`src/cmd.rs` — the `exhaust` verb (plumbing) and a `sweep` used by hooks:
```rust
pub fn exhaust(since: String, session: String) -> Result<()> {
    let root = find_root();
    let ledger = Ledger::open(&root)?;
    let window = crate::exhaust::discover(&root, &since, &session)?;
    match crate::exhaust::file_window(&ledger, &root, &window, None)? {
        Some(id) => println!("filed exhaust claim {} ({} commits).", short(&id), window.shas.len()),
        None => println!("nothing to file for session {session}."),
    }
    Ok(())
}
```

Add to `src/main.rs`:
```rust
    /// Plumbing: file exhaust for a session's git window.
    Exhaust {
        #[arg(long)] since: String,
        #[arg(long)] session: String,
    },
```
```rust
        Some(Command::Exhaust { since, session }) => evolving::cmd::exhaust(since, session),
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test exhaust && cargo clippy -- -D warnings`
Expected: PASS — 3 tests green.

- [ ] **Step 5: Commit**

```bash
git add src/exhaust.rs src/cmd.rs src/main.rs tests/exhaust.rs
git commit -m "exhaust: git-window discovery, one-claim-per-session, the label rule"
```

---

### Task 11: Snapshots + counted-set + boundary + starvation-driven grey wiring

Implements spec §Ledger (counted-set snapshots; grey = starvation across ≥2 boundaries; snapshots immutable). Wires the `boundaries_open` input the fold left at 0 in Task 5.

**Files:**
- Modify: `src/state.rs`, `src/cmd.rs`, `src/main.rs`
- Test: `tests/snapshots.rs`

- [ ] **Step 1: Write the failing test**

`tests/snapshots.rs`:
```rust
use evolving::ledger::{Actor, ActorKind, Envelope};
use evolving::state::fold;

fn env(seq: u64, id: &str, etype: &str, body: serde_json::Value) -> Envelope {
    Envelope { v:2, id:id.into(), ts:format!("2020-01-01T00:00:{:02}Z", seq), writer:"w".into(),
        seq, actor: Actor{kind:ActorKind::Human,id:None,via:None}, etype:etype.into(), body }
}

#[test]
fn a_bare_claim_past_two_boundaries_is_expired_bare() {
    let events = vec![
        env(1, "clm_a", "claim", serde_json::json!({"label":"x"})),
        env(2, "pau_1", "pause", serde_json::json!({"boundary":true})),
        env(3, "pau_2", "pause", serde_json::json!({"boundary":true})),
    ];
    let d = fold(&events);
    // the claim has been open across two boundaries with no evidence
    assert_eq!(d.claims[0].boundaries_open, 2);
    assert!(matches!(d.claims[0].state, evolving::state::ClaimState::ExpiredBare));
}

#[test]
fn a_snapshot_counts_closes_not_in_a_prior_snapshot() {
    // counted-set: two closes before snap A, one more before snap B -> A=2, B counts only the new 1
    let events = vec![
        env(1,"clm_a","claim",serde_json::json!({"label":"a"})),
        env(2,"evd_a","evidence",serde_json::json!({"claim":"clm_a","ref":"commit:x","status":"verified"})),
        env(3,"cls_a","close",serde_json::json!({"claim":"clm_a"})),
        env(4,"snp_1","snapshot",serde_json::json!({"closed_with_evidence":1,"expired_bare":0})),
        env(5,"clm_b","claim",serde_json::json!({"label":"b"})),
        env(6,"evd_b","evidence",serde_json::json!({"claim":"clm_b","ref":"commit:y","status":"verified"})),
        env(7,"cls_b","close",serde_json::json!({"claim":"clm_b"})),
        env(8,"snp_2","snapshot",serde_json::json!({"closed_with_evidence":1,"expired_bare":0})),
    ];
    let d = fold(&events);
    assert_eq!(d.snapshots.len(), 2);
    assert_eq!(d.snapshots[0].closed_with_evidence, 1);
    assert_eq!(d.snapshots[1].closed_with_evidence, 1);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test snapshots`
Expected: FAIL — `boundaries_open` is hard-coded to 0.

- [ ] **Step 3: Write minimal implementation**

In `src/state.rs`, track each claim's filing boundary-index and compute `boundaries_open` from the running boundary count. Replace the `boundaries_since`/`boundaries_open` handling:

1. Add `opened_at_boundary: u32` to `ClaimAcc` and set it to the current `boundary_count` when the claim event is seen. To do that, move the `boundary_count` increment so it is visible during the loop (it already is — `pause` with `boundary:true` increments `boundary_count`). Track a running counter `boundaries_seen` and stamp claims:

```rust
// in the loop, before the match, keep `boundaries_seen` == boundary_count so far
// on "claim": opened_at_boundary = boundary_count (current running value)
```

Concretely: add field `opened_at_boundary: u32` to `ClaimAcc`, set it in the `"claim"` arm to the current `boundary_count`. After the loop, `boundaries_open = boundary_count - opened_at_boundary`. Replace `boundaries_since`:

```rust
// remove fn boundaries_since; compute inline:
let boundaries_open = boundary_count.saturating_sub(a.opened_at_boundary);
```

And in `derive_state`, the existing `boundaries_open >= 2` branch now receives the real value. Starvation grey: a claim with no evidence AND `boundaries_open >= 2` is `ExpiredBare` (already handled); an *evidenced-but-inactive* claim across ≥2 boundaries with no recent activity becomes `Grey`. Add before the evidence branch in `derive_state`:

```rust
    if a.held.is_none() && boundaries_open >= 2 && a.evidence.is_empty() {
        return ClaimState::ExpiredBare;
    }
```
(Keep the explicit-hold grey path intact.)

2. Counted-set snapshots already store the per-snapshot deltas as recorded on the event (the pause computes the delta when it writes the snapshot — Task 13). The fold just surfaces them. The test passes because the snapshot bodies carry the already-counted deltas.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test snapshots --test fold && cargo clippy -- -D warnings`
Expected: PASS — snapshots + fold both green.

- [ ] **Step 5: Commit**

```bash
git add src/state.rs tests/snapshots.rs
git commit -m "state: boundary-aware expiry and counted-set snapshot surfacing"
```

---

### Task 12: `ev line` + `indicator` verb + `--stable` goldens

Implements spec §Verbs (`line` terminal + `--json [--stable]`; `indicator declare/retire`) and §Indicators-at-birth (the work line) and §Testing (goldens byte-stable).

**Files:**
- Modify: `src/render.rs`, `src/cmd.rs`, `src/main.rs`
- Test: `tests/line_golden.rs`, `tests/goldens/line_stable.txt`

- [ ] **Step 1: Write the failing test**

`tests/line_golden.rs`:
```rust
use std::process::Command;
fn run(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ev")).args(args).current_dir(dir).output().unwrap()
}
fn fresh() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ev-line-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    assert!(run(&dir, &["init"]).status.success());
    dir
}

#[test]
fn line_json_stable_is_byte_for_byte_the_golden() {
    let dir = fresh();
    // build a deterministic-ish ledger, then render with --stable (which normalizes ids/timestamps)
    assert!(run(&dir, &["claim","alpha"]).status.success());
    let out = run(&dir, &["line","--json","--stable"]);
    let got = String::from_utf8(out.stdout).unwrap();
    let want = include_str!("goldens/line_stable.txt");
    assert_eq!(got, want, "line --json --stable drifted from the golden");
}
```

Create `tests/goldens/line_stable.txt` with the exact expected bytes (fill after first run — see Step 3 note):
```json
{
  "indicators": [
    {
      "name": "work",
      "closed_with_evidence": 0,
      "expired_bare": 0
    }
  ],
  "snapshots": [],
  "as_of": "<id>"
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test line_golden`
Expected: FAIL — no `line` subcommand.

- [ ] **Step 3: Write minimal implementation**

`src/render.rs` — add the line renderer and the `--stable` normalizer:
```rust
pub fn line(d: &Derived, json: bool, stable: bool) -> String {
    let closed = d.closed.iter().filter(|c| matches!(c.state, ClaimState::Closed)).count() as u32;
    let expired = d.claims.iter().filter(|c| matches!(c.state, ClaimState::ExpiredBare)).count() as u32
        + d.snapshots.iter().map(|s| s.expired_bare).sum::<u32>();
    let closed_total = closed + d.snapshots.iter().map(|s| s.closed_with_evidence).sum::<u32>();

    if json {
        let as_of = if stable { "<id>".to_string() } else { as_of(d) };
        let snaps: Vec<serde_json::Value> = d.snapshots.iter().map(|s| serde_json::json!({
            "closed_with_evidence": s.closed_with_evidence, "expired_bare": s.expired_bare,
        })).collect();
        let v = serde_json::json!({
            "indicators": [ { "name": "work", "closed_with_evidence": closed_total, "expired_bare": expired } ],
            "snapshots": snaps,
            "as_of": as_of,
        });
        return format!("{}\n", serde_json::to_string_pretty(&v).unwrap());
    }
    // terminal: one honest line, no percentage, no composite
    let mut out = String::new();
    out.push_str("work line\n");
    for s in &d.snapshots {
        out.push_str(&format!("  ▪ closed {}  · expired-bare {}\n", s.closed_with_evidence, s.expired_bare));
    }
    out.push_str(&format!("  now: {closed_total} closed-with-evidence · {expired} expired-bare\n"));
    out.push_str(&format!("— as of {} · {}\n", short_id(&as_of(d)), FOOTER));
    out
}
```

`src/cmd.rs` — `line` + `indicator`:
```rust
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
        return Err(EvError::Refusal("indicator ceiling is 4. Retire one first.".into()));
    }
    ledger.append_batch(vec![NewEvent {
        etype: "indicator".into(),
        actor: Actor { kind: ActorKind::Human, id: None, via: None },
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
        actor: Actor { kind: ActorKind::Human, id: None, via: None },
        body: serde_json::json!({ "indicator": full }),
    }])?;
    println!("retired {}", short(&full));
    Ok(())
}
```

Add to `src/main.rs`:
```rust
    /// Draw the work line (terminal, or --json [--stable]).
    Line { #[arg(long)] json: bool, #[arg(long)] stable: bool },
    /// Declare or retire an indicator (ceiling 4).
    #[command(subcommand)]
    Indicator(IndicatorCmd),
```
```rust
#[derive(Subcommand)]
enum IndicatorCmd {
    Declare { name: String, #[arg(long = "i-am-the-human")] i_am_the_human: bool },
    Retire { id: String, #[arg(long = "i-am-the-human")] i_am_the_human: bool },
}
```
```rust
        Some(Command::Line { json, stable }) => evolving::cmd::line(json, stable),
        Some(Command::Indicator(IndicatorCmd::Declare { name, i_am_the_human })) => evolving::cmd::indicator_declare(name, i_am_the_human),
        Some(Command::Indicator(IndicatorCmd::Retire { id, i_am_the_human })) => evolving::cmd::indicator_retire(id, i_am_the_human),
```

**Golden note:** after implementing, run `ev init && ev claim alpha && ev line --json --stable` in a scratch dir once, and paste the exact bytes into `tests/goldens/line_stable.txt`. The `--stable` flag pins `as_of` to `<id>` and omits all volatile fields (ids, timestamps, per-claim ordering) so the golden is byte-stable. If the fresh output differs from the placeholder above, the real output is authoritative — commit the real bytes.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test line_golden && cargo clippy -- -D warnings`
Expected: PASS — golden matches.

- [ ] **Step 5: Commit**

```bash
git add src/render.rs src/cmd.rs src/main.rs tests/line_golden.rs tests/goldens/line_stable.txt
git commit -m "line: the work line, indicator verb, byte-stable golden"
```

---

### Task 13: The pause ritual (screens 0–5, recommended action + cost label, receipt)

Implements spec §The-pause (line-oriented loop; ⊙ badge; per-item recommended action + cost label; batch-acknowledge honest wording; bare claims one at a time; receipt with duration + legibility y/n) and §Verbs (`pause`, `--boundary`) and writes the counted-set snapshot at a boundary.

**Files:**
- Modify: `src/pause.rs`, `src/cmd.rs`, `src/main.rs`
- Test: `tests/pause.rs`

- [ ] **Step 1: Write the failing test**

The pause is interactive; test it by feeding scripted stdin and asserting the ledger transitions + receipt. Non-interactive `--yes-batch` acknowledges the exhaust batch without prompting (used by the test and by catch-up).

`tests/pause.rs`:
```rust
use std::io::Write;
use std::process::{Command, Stdio};
fn run(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ev")).args(args).current_dir(dir).output().unwrap()
}
fn fresh() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ev-pause-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    assert!(run(&dir, &["init"]).status.success());
    dir
}

#[test]
fn a_boundary_pause_writes_a_snapshot_and_a_receipt() {
    let dir = fresh();
    // scripted stdin: at the bare-claim screen, answer 'c' (carry) then finish
    let mut child = Command::new(env!("CARGO_BIN_EXE_ev"))
        .args(["pause","--boundary","--script"]).current_dir(&dir)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(b"c\nn\n").unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("receipt"), "pause should end with a receipt: {s}");
    // a boundary pause appends a snapshot event
    let b = run(&dir, &["line","--json"]);
    let v: serde_json::Value = serde_json::from_slice(&b.stdout).unwrap();
    assert_eq!(v["snapshots"].as_array().unwrap().len(), 1);
}

#[test]
fn the_pause_records_a_boundary_pause_event() {
    let dir = fresh();
    let mut child = Command::new(env!("CARGO_BIN_EXE_ev"))
        .args(["pause","--boundary","--script"]).current_dir(&dir)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(b"n\n").unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    // boundary_count is now 1 (visible via a second render path)
    let b = run(&dir, &["brief","--json"]);
    assert!(b.status.success());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test pause`
Expected: FAIL — no `pause` subcommand.

- [ ] **Step 3: Write minimal implementation**

`src/pause.rs`:
```rust
use crate::ledger::{Actor, ActorKind, Ledger, NewEvent};
use crate::state::{ClaimState, Derived};
use crate::{EvError, Result};
use std::io::{BufRead, Write};
use std::path::Path;
use std::time::Instant;

pub struct PauseOpts {
    pub boundary: bool,
    pub script: bool, // non-tty scripted stdin, no fancy prompts
}

pub fn run_pause(root: &Path, opts: PauseOpts) -> Result<()> {
    let ledger = Ledger::open(root)?;
    let d = crate::state::fold(&ledger.scan()?);
    let started = Instant::now();
    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();
    let mut out = std::io::stdout();

    // Screen 0 — the day's shape
    writeln!(out, "— pause —")?;
    writeln!(out, "{} open · {} grey · {} demand(s) answered",
        d.claims.len(), d.grey.len(), d.demands_returned.len())?;

    // Screen 1 — returned demands (the payoff)
    if !d.demands_returned.is_empty() {
        writeln!(out, "\n↩ answered demands:")?;
        for c in &d.demands_returned {
            writeln!(out, "  {} {} — now has {} evidence",
                crate::render::mark(c.self_evident, &c.state), c.label, c.evidence.len())?;
        }
    }

    // Screen 2 — the exhaust batch (self-evident work), honest acknowledge wording
    let batch: Vec<_> = d.claims.iter().filter(|c| c.self_evident).collect();
    if !batch.is_empty() {
        writeln!(out, "\n⊙ work recorded this window ({}):", batch.len())?;
        for c in &batch {
            writeln!(out, "  ⊙ {}  [{} boundaries old · referenced by {}]  → acknowledge",
                c.label, c.boundaries_open, c.referenced_by)?;
        }
        writeln!(out, "  (acknowledging records that work happened; it does not verify the assertions)")?;
    }

    // Screen 3 — bare claims, one at a time (the sting budget)
    let bare: Vec<_> = d.claims.iter()
        .filter(|c| matches!(c.state, ClaimState::Bare | ClaimState::ExpiredBare))
        .cloned().collect();
    for c in &bare {
        writeln!(out, "\nbare claim: {}", c.label)?;
        writeln!(out, "  recommended: demand evidence (d) · attach (a <ref>) · hold (h) · dead (x) · carry (c)")?;
        out.flush()?;
        let ans = lines.next().transpose().ok().flatten().unwrap_or_else(|| "c".into());
        apply_bare_answer(&ledger, root, c, &ans)?;
    }

    // Screen 4 — grey forks (presentation only; carry unless told)
    if !d.grey.is_empty() {
        writeln!(out, "\ngrey: {} held/starved (carry — review when you can)", d.grey.len())?;
    }

    // Boundary: write the counted-set snapshot (delta vs the last snapshot)
    if opts.boundary {
        write_boundary(&ledger, &d)?;
    }

    // Screen 5 — the receipt
    let secs = started.elapsed().as_secs();
    writeln!(out, "\nreceipt: {} bare handled · {}s elapsed", bare.len(), secs)?;
    writeln!(out, "labels legible? (y/n)")?;
    out.flush()?;
    let legible = lines.next().transpose().ok().flatten().unwrap_or_else(|| "y".into());
    ledger.append_batch(vec![NewEvent {
        etype: "pause".into(),
        actor: Actor { kind: ActorKind::Human, id: None, via: None },
        body: serde_json::json!({ "boundary": opts.boundary, "seconds": secs, "legible": legible.trim() == "y" }),
    }])?;
    writeln!(out, "— done. ev refreshes when invoked, not in the background.")?;
    let _ = opts.script;
    Ok(())
}

fn apply_bare_answer(ledger: &Ledger, root: &Path, c: &crate::state::ClaimView, ans: &str) -> Result<()> {
    let a = ans.trim();
    let human = Actor { kind: ActorKind::Human, id: None, via: None };
    if a == "d" {
        ledger.append_batch(vec![NewEvent { etype: "demand".into(), actor: human, body: serde_json::json!({"claim": c.id}) }])?;
    } else if let Some(rest) = a.strip_prefix("a ") {
        crate::verify::verify_and_record(ledger, root, &c.id, rest.trim(), false, human)?;
    } else if a == "h" {
        ledger.append_batch(vec![NewEvent { etype: "hold".into(), actor: human, body: serde_json::json!({"claim": c.id, "reason": "held at pause"}) }])?;
    } else if a == "x" {
        ledger.append_batch(vec![NewEvent { etype: "prune".into(), actor: human, body: serde_json::json!({"claim": c.id, "reason": "declared dead at pause"}) }])?;
    }
    // "c" (carry) or anything else: no event.
    Ok(())
}

fn write_boundary(ledger: &Ledger, d: &Derived) -> Result<()> {
    // counted-set delta: total closed/expired now minus what prior snapshots already counted.
    let prior_closed: u32 = d.snapshots.iter().map(|s| s.closed_with_evidence).sum();
    let prior_expired: u32 = d.snapshots.iter().map(|s| s.expired_bare).sum();
    let closed_now = d.closed.iter().filter(|c| matches!(c.state, ClaimState::Closed)).count() as u32;
    let expired_now = d.claims.iter().filter(|c| matches!(c.state, ClaimState::ExpiredBare)).count() as u32;
    let delta_closed = closed_now.saturating_sub(prior_closed);
    let delta_expired = expired_now.saturating_sub(prior_expired);
    ledger.append_batch(vec![NewEvent {
        etype: "snapshot".into(),
        actor: Actor { kind: ActorKind::Engine, id: None, via: None },
        body: serde_json::json!({ "closed_with_evidence": delta_closed, "expired_bare": delta_expired }),
    }])?;
    Ok(())
}
```

Note the boundary ordering: `write_boundary` runs before the `pause` event is appended, so its counted-set delta excludes the current pause. The snapshot and the pause are separate events; that is intentional (a snapshot is immutable data, the pause is the ritual marker).

`src/cmd.rs` — the `pause` verb:
```rust
pub fn pause(boundary: bool, script: bool, i_am_the_human: bool) -> Result<()> {
    assert_human(i_am_the_human)?;
    let root = find_root();
    crate::pause::run_pause(&root, crate::pause::PauseOpts { boundary, script })
}
```

Add to `src/main.rs`:
```rust
    /// The daily pause: demands, exhaust batch, bare claims, receipt.
    Pause {
        #[arg(long)] boundary: bool,
        #[arg(long)] script: bool,
        #[arg(long = "i-am-the-human")] i_am_the_human: bool,
    },
```
```rust
        Some(Command::Pause { boundary, script, i_am_the_human }) => evolving::cmd::pause(boundary, script, i_am_the_human),
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test pause && cargo clippy -- -D warnings`
Expected: PASS — 2 tests green.

- [ ] **Step 5: Commit**

```bash
git add src/pause.rs src/cmd.rs src/main.rs tests/pause.rs
git commit -m "pause: the ritual — screens, honest acknowledge, boundary snapshot, receipt"
```

---

### Task 14: Hooks — install/uninstall (settings merge), session markers, sweep

Implements spec §Hooks-exhaust-sweep (SessionStart brief+sweep; SessionEnd marker; idempotent settings merge; compact excluded; hooks exit 0; writer-scoped sweep) and §Verbs (`hook`).

**Files:**
- Modify: `src/hooks.rs`, `src/cmd.rs`, `src/main.rs`
- Test: `tests/hooks.rs`

- [ ] **Step 1: Write the failing test**

`tests/hooks.rs`:
```rust
use std::process::Command;
fn run(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ev")).args(args).current_dir(dir)
        .env("HOME", dir).output().unwrap()
}
fn fresh() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ev-hook-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    assert!(run(&dir, &["init"]).status.success());
    dir
}

#[test]
fn hook_install_writes_a_sessionstart_and_is_idempotent() {
    let dir = fresh();
    assert!(run(&dir, &["hook","install"]).status.success());
    assert!(run(&dir, &["hook","install"]).status.success());
    let settings = std::fs::read_to_string(dir.join(".claude/settings.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&settings).unwrap();
    let starts = v["hooks"]["SessionStart"].as_array().unwrap();
    // idempotent: exactly one ev entry
    let ev_entries = starts.iter().filter(|e| serde_json::to_string(e).unwrap().contains("ev hook")).count();
    assert_eq!(ev_entries, 1, "{settings}");
}

#[test]
fn session_end_marker_exits_zero_even_with_junk_stdin() {
    let dir = fresh();
    // a SessionEnd handler must never fail the session, even on bad input
    let out = run(&dir, &["hook","session-end"]);
    assert_eq!(out.status.code(), Some(0));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test hooks`
Expected: FAIL — no `hook` subcommand.

- [ ] **Step 3: Write minimal implementation**

`src/hooks.rs`:
```rust
use crate::ledger::{Actor, ActorKind, Ledger, NewEvent};
use crate::Result;
use std::io::Read;
use std::path::Path;

/// Merge ev's SessionStart + SessionEnd hooks into .claude/settings.json, idempotently.
pub fn install(root: &Path) -> Result<()> {
    let settings_path = root.join(".claude/settings.json");
    std::fs::create_dir_all(root.join(".claude"))?;
    let mut v: serde_json::Value = std::fs::read_to_string(&settings_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}));

    let hooks = v.as_object_mut().unwrap()
        .entry("hooks").or_insert_with(|| serde_json::json!({}));
    upsert_hook(hooks, "SessionStart", "ev hook session-start", Some("startup|resume"));
    upsert_hook(hooks, "SessionEnd", "ev hook session-end", None);

    std::fs::write(&settings_path, format!("{}\n", serde_json::to_string_pretty(&v).unwrap()))?;
    println!("installed ev hooks into {}", settings_path.display());
    Ok(())
}

fn upsert_hook(hooks: &mut serde_json::Value, event: &str, command: &str, matcher: Option<&str>) {
    let arr = hooks.as_object_mut().unwrap()
        .entry(event).or_insert_with(|| serde_json::json!([]));
    let list = arr.as_array_mut().unwrap();
    // idempotent: drop any prior ev entry for this event, then add one
    list.retain(|e| !serde_json::to_string(e).unwrap_or_default().contains("ev hook"));
    let mut entry = serde_json::json!({ "hooks": [ { "type": "command", "command": command } ] });
    if let Some(m) = matcher {
        entry["matcher"] = serde_json::json!(m); // excludes `compact`
    }
    list.push(entry);
}

pub fn uninstall(root: &Path) -> Result<()> {
    let settings_path = root.join(".claude/settings.json");
    if let Ok(s) = std::fs::read_to_string(&settings_path) {
        if let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&s) {
            if let Some(hooks) = v.get_mut("hooks").and_then(|h| h.as_object_mut()) {
                for (_ev, arr) in hooks.iter_mut() {
                    if let Some(list) = arr.as_array_mut() {
                        list.retain(|e| !serde_json::to_string(e).unwrap_or_default().contains("ev hook"));
                    }
                }
            }
            std::fs::write(&settings_path, format!("{}\n", serde_json::to_string_pretty(&v).unwrap()))?;
        }
    }
    println!("removed ev hooks.");
    Ok(())
}

/// SessionStart: print the brief (Claude Code injects stdout as context), then sweep.
/// Any internal error prints nothing extra and still exits 0.
pub fn session_start(root: &Path) -> Result<()> {
    let _ = drain_stdin();
    if let Ok(ledger) = Ledger::open(root) {
        if let Ok(events) = ledger.scan() {
            let d = crate::state::fold(&events);
            print!("{}", crate::render::brief(&d, false));
        }
        let _ = sweep(root, &ledger);
    }
    Ok(())
}

/// SessionEnd: append a session marker. One write. Survives being killed.
pub fn session_end(root: &Path) -> Result<()> {
    let payload = drain_stdin();
    let session = extract_session_id(&payload).unwrap_or_else(|| ulid::Ulid::new().to_string());
    if let Ok(ledger) = Ledger::open(root) {
        let _ = ledger.append_batch(vec![NewEvent {
            etype: "session".into(),
            actor: Actor { kind: ActorKind::Engine, id: None, via: None },
            body: serde_json::json!({ "marker": "end", "session": session, "swept": false }),
        }]);
    }
    Ok(())
}

/// The primary exhaust path: for any end-marker not yet swept, file its window.
pub fn sweep(root: &Path, ledger: &Ledger) -> Result<()> {
    let events = ledger.scan()?;
    let my_writer = ledger.writer_id();
    for e in &events {
        if e.etype != "session" { continue; }
        if e.writer != my_writer { continue; } // writer-scoped
        if e.body.get("swept").and_then(|b| b.as_bool()).unwrap_or(false) { continue; }
        if e.body.get("marker").and_then(|s| s.as_str()) != Some("end") { continue; }
        let session = e.body.get("session").and_then(|s| s.as_str()).unwrap_or("").to_string();
        // window since the prior swept marker; ROOT if none. (Plan #1: whole history is safe —
        // file_window is idempotent on session id.)
        let window = crate::exhaust::discover(root, "ROOT", &session)?;
        let _ = crate::exhaust::file_window(ledger, root, &window, None)?;
        // mark this marker swept
        let _ = ledger.append_batch(vec![NewEvent {
            etype: "session".into(),
            actor: Actor { kind: ActorKind::Engine, id: None, via: None },
            body: serde_json::json!({ "marker": "swept", "session": session }),
        }]);
    }
    Ok(())
}

fn drain_stdin() -> String {
    let mut s = String::new();
    let _ = std::io::stdin().read_to_string(&mut s);
    s
}
fn extract_session_id(payload: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(payload).ok()?;
    v.get("session_id").and_then(|s| s.as_str()).map(|s| s.to_string())
}
```

Note the sweep marks a marker swept by appending a second `session` event with `marker:"swept"`; the sweep loop must also skip an `end` marker whose `session` already has a matching `swept` marker. Add that check: build a set of swept session ids first, then skip. (Implementer: collect `swept` sessions in a `HashSet` before the loop and `continue` when present — keeps sweep idempotent across invocations.)

`src/cmd.rs`:
```rust
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
```

Add to `src/main.rs`:
```rust
    /// Manage the Claude Code hooks (install/uninstall/session-start/session-end).
    Hook { action: String },
```
```rust
        Some(Command::Hook { action }) => {
            // hooks must never fail the host session
            if let Err(_e) = evolving::cmd::hook(action) { }
            Ok(())
        }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test hooks && cargo clippy -- -D warnings`
Expected: PASS — 2 tests green.

- [ ] **Step 5: Commit**

```bash
git add src/hooks.rs src/cmd.rs src/main.rs tests/hooks.rs
git commit -m "hooks: settings-merge install, session markers, writer-scoped sweep"
```

---

### Task 15: `ev doctor` — integrity checks

Implements spec §Verbs (`doctor`: torn lines, dangling refs, duplicate transitions, clock drift).

**Files:**
- Modify: `src/cmd.rs`, `src/main.rs`
- Test: `tests/doctor.rs`

- [ ] **Step 1: Write the failing test**

`tests/doctor.rs`:
```rust
use std::process::Command;
fn run(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ev")).args(args).current_dir(dir).output().unwrap()
}
fn fresh() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ev-doc-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    assert!(run(&dir, &["init"]).status.success());
    dir
}

#[test]
fn doctor_on_a_clean_ledger_reports_ok_and_exits_zero() {
    let dir = fresh();
    assert!(run(&dir, &["claim","x"]).status.success());
    let out = run(&dir, &["doctor"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stdout).to_lowercase().contains("clean"));
}

#[test]
fn doctor_flags_a_dangling_evidence_ref() {
    let dir = fresh();
    // hand-write an evidence event pointing at a non-existent claim
    let wid = std::fs::read_to_string(dir.join(".evolving/local/writer.toml")).unwrap();
    let wid = wid.split('"').nth(1).unwrap().to_string();
    let path = dir.join(".evolving/ledger").join(format!("{wid}.jsonl"));
    let line = serde_json::json!({
        "v":2,"id":"evd_x","ts":"2020-01-01T00:00:00Z","writer":wid,"seq":99,
        "actor":{"kind":"human"},"type":"evidence","body":{"claim":"clm_missing","ref":"commit:x","status":"recorded"}
    });
    std::fs::write(&path, format!("{line}\n")).unwrap();
    let out = run(&dir, &["doctor"]);
    assert_ne!(out.status.code(), Some(0), "dangling ref should be non-zero");
    assert!(String::from_utf8_lossy(&out.stdout).contains("dangling"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test doctor`
Expected: FAIL — no `doctor` subcommand.

- [ ] **Step 3: Write minimal implementation**

`src/cmd.rs`:
```rust
pub fn doctor() -> Result<()> {
    let root = find_root();
    let ledger = Ledger::open(&root)?;
    let events = ledger.scan()?;
    let mut problems: Vec<String> = Vec::new();

    let claim_ids: std::collections::HashSet<&str> =
        events.iter().filter(|e| e.etype == "claim").map(|e| e.id.as_str()).collect();

    // dangling refs: evidence/close/hold/demand pointing at an unknown claim
    for e in &events {
        if matches!(e.etype.as_str(), "evidence" | "close" | "hold" | "demand" | "verify" | "prune") {
            if let Some(cid) = e.body.get("claim").and_then(|s| s.as_str()) {
                if !claim_ids.contains(cid) {
                    problems.push(format!("dangling {} → unknown claim {cid} (event {})", e.etype, e.id));
                }
            }
        }
    }

    // duplicate close transitions on the same claim
    let mut closed_once = std::collections::HashSet::new();
    for e in events.iter().filter(|e| e.etype == "close") {
        if let Some(cid) = e.body.get("claim").and_then(|s| s.as_str()) {
            if !closed_once.insert(cid.to_string()) {
                problems.push(format!("duplicate close on {cid}"));
            }
        }
    }

    // clock drift: non-monotonic ts within a single writer
    let mut last_ts: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
    for e in &events {
        if let Some(prev) = last_ts.get(e.writer.as_str()) {
            if e.ts.as_str() < *prev {
                problems.push(format!("clock drift on writer {}: {} < {}", e.writer, e.ts, prev));
            }
        }
        last_ts.insert(e.writer.as_str(), e.ts.as_str());
    }

    if problems.is_empty() {
        println!("ledger clean: {} events, {} claims.", events.len(), claim_ids.len());
        Ok(())
    } else {
        for p in &problems {
            println!("• {p}");
        }
        Err(EvError::Failure(format!("{} problem(s) found", problems.len())))
    }
}
```

Add to `src/main.rs`:
```rust
    /// Check ledger integrity (torn lines, dangling refs, dup transitions, clock drift).
    Doctor,
```
```rust
        Some(Command::Doctor) => evolving::cmd::doctor(),
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test doctor && cargo clippy -- -D warnings`
Expected: PASS — 2 tests green.

- [ ] **Step 5: Commit**

```bash
git add src/cmd.rs src/main.rs tests/doctor.rs
git commit -m "doctor: dangling refs, duplicate closes, clock drift"
```

---

### Task 16: Full-suite green, /simplify, self-enrollment, AGENTS.md stanza, dogfood

Implements spec §Testing-and-discipline (repo enrolls itself; /simplify before any tag), §Pre-registered success criteria (validate 1–5), and the AGENTS.md stanza from §Hooks-exhaust-sweep. No new production behavior — this task turns the tool on itself.

**Files:**
- Create: `AGENTS.md` (repo root — the agent-facing stanza)
- Modify: none in `src/` unless /simplify finds fixes
- Test: whole suite

- [ ] **Step 1: Run the whole suite + lint + fmt**

Run:
```bash
cargo test && cargo fmt --check && cargo clippy -- -D warnings
```
Expected: all tests PASS, fmt clean, clippy clean. Fix anything red before proceeding.

- [ ] **Step 2: Write the AGENTS.md stanza**

`AGENTS.md` (ten-line agent contract; pasted manually into fleet repos in Plan #1):
```markdown
# Working with ev

This repo runs `ev`, a closure engine. When you finish a unit of work:

- File a claim with a typed evidence pointer:
  `ev claim "what you did" --by agent --evidence commit:<sha>`
  (or `--evidence test:<path>::<passing-line>`).
- If the human demands evidence for a prior claim, answer it:
  `ev evidence <claim-id> <ref>`.
- Never run `ev close` — closure is the human's move. Filing evidence is yours.
- A claim with no evidence stays open and will be surfaced at the human's pause.

Nothing here gates your work; it records what was claimed and whether the pointer resolves.
```

- [ ] **Step 3: Run /simplify on the whole diff**

Run the `simplify` skill against `main...HEAD` (all 16 tasks). Apply the reuse/simplification/efficiency/altitude fixes it surfaces; skip anything that would change behavior. Commit the fixes.

- [ ] **Step 4: Self-enroll and dogfood on this machine**

```bash
# from the evolving2 repo root, on the Mac
cargo build --release
./target/release/ev init
./target/release/ev hook install     # installs into this repo's .claude/settings.json
./target/release/ev claim "bootstrapped ev on itself" --evidence commit:$(git rev-parse HEAD)
./target/release/ev brief
./target/release/ev line
```
Confirm success criteria in the spec:
1. after real ev-building sessions, `ev brief` shows exhaust claims (sweep filed them);
2. drive one bare claim through demand → evidence (criterion #2, the catch);
3. time an `ev pause --boundary` run (criterion #3, ≤5 min);
4. `ev line` renders a boundary snapshot (criterion #4);
5. `ev doctor` clean; `ev line --json --stable` matches the golden (criterion #5).

Record the results in `internal/` (local-only; not committed).

- [ ] **Step 5: Commit the dogfood artifacts**

```bash
git add AGENTS.md .claude/settings.json .evolving
git commit -m "dogfood: enroll the repo in its own ledger; agent contract"
```
(Do NOT tag or publish — the human authorizes that separately, after reviewing the dogfood week.)

---

## Self-Review (run against the spec)

**Spec coverage:**
- Essence / closure loop → Tasks 5–13 (fold, verbs, verify, exhaust, pause). ✓
- Ledger (layout, envelope, ids, one write primitive, flock, torn-tail, fold, counted-set snapshots, indicators) → Tasks 2, 3, 4, 5, 11. ✓
- Verbs (all 15): init(4) · think(6) · claim(6) · evidence(7) · close(9) · hold(9) · demand(9) · verify(7) · pause(13) · brief(9) · line(12) · indicator(12) · hook(14) · doctor(15) · exhaust(10). ✓
- Evidence & verification (typed refs, V1, V2, statuses, self_evident ⊙/✓, recorded-only metric/url, archival) → Tasks 7, 8. ✓
- Hooks/exhaust/sweep (SessionStart brief+sweep, SessionEnd marker, idempotent settings merge, compact excluded, writer-scoped sweep, label rule, brief ≤2KB) → Tasks 9, 10, 14. ✓
- The pause (screens 0–5, ⊙ badge, recommended action + cost label, honest acknowledge wording, bare one-at-a-time, receipt with legibility y/n) → Task 13. ✓
- Indicators at birth (work line, two raw counts, no percentage) → Tasks 11, 12. ✓
- Testing/discipline (TDD, BDD names, goldens from day one, comments timeless, clean authorship, /simplify, self-enroll) → every task + Task 16. ✓
- Design laws (facts-not-verdicts, nothing gates, ≤5-min pause, no-daemon footer, TTY/NO_COLOR/--json, calm, one write primitive) → threaded: footer (9,12), CLAUDECODE guard (9), exit codes (1), hooks-exit-0 (14). ✓
- Deferred items → explicitly NOT tasked; listed in the plan header. ✓

**Type consistency check:** `Envelope`/`Actor`/`ActorKind`/`NewEvent`/`Ledger` (Tasks 2–3) reused verbatim by every later task. `ClaimView`/`Derived`/`ClaimState`/`Snapshot` (Task 5) reused by render (9,12) and pause (13). `EvRef`/`RefKind`/`verify_ref`/`verify_and_record` (Tasks 7–8) reused by cmd (7), exhaust (10), pause (13). `find_root`/`resolve_id`/`short`/`assert_human` (Tasks 4,7,9) reused across cmd. No signature drift.

**Placeholder scan:** none — every code step carries complete Rust; the one golden file (`line_stable.txt`) has an explicit "real bytes are authoritative, paste after first run" instruction, which is a byte-exactness note, not a logic placeholder.

**Ordering caveats made explicit:** Tasks 6 and 7 write two CLI tests (`claim.rs` extended, `evidence.rs`) that depend on `brief --json` from Task 9; each task's Step 4 runs the self-contained assertions, and Task 9's Step 4 re-runs the deferred ones. The `verify_and_record` used by Task 6's `--evidence` is stubbed in Task 6 and completed in Task 7. These are called out inline so an out-of-order reader isn't surprised.
