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

impl Actor {
    pub fn human() -> Self {
        Actor {
            kind: ActorKind::Human,
            id: None,
            via: None,
        }
    }
    pub fn engine() -> Self {
        Actor {
            kind: ActorKind::Engine,
            id: None,
            via: None,
        }
    }
    pub fn agent(id: impl Into<String>) -> Self {
        Actor {
            kind: ActorKind::Agent,
            id: Some(id.into()),
            via: None,
        }
    }
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

use crate::{EvError, Result, SCHEMA_VERSION};
use fs4::FileExt;
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
    root: PathBuf,
    writer: String,
}

impl Ledger {
    /// Open the ledger rooted at `root` (the dir containing `.evolving/`).
    pub fn open(root: &Path) -> Result<Ledger> {
        let writer = load_or_make_writer(root)?;
        Ok(Ledger {
            root: root.to_path_buf(),
            writer,
        })
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
    /// or a torn final line is skipped on read.
    pub fn append_batch(&self, events: Vec<NewEvent>) -> Result<Vec<Envelope>> {
        if events.is_empty() {
            return Ok(vec![]);
        }
        fs::create_dir_all(self.ledger_dir())?;
        let lockf = OpenOptions::new()
            .create(true)
            .truncate(false)
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
            buf.push_str(
                &serde_json::to_string(&env).map_err(|e| EvError::Failure(e.to_string()))?,
            );
            buf.push('\n');
            minted.push(env);
        }
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.writer_path())?;
        self.heal_torn_tail(&mut f)?;
        f.write_all(buf.as_bytes())?;
        f.sync_all()?;
        lockf.unlock()?;
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
        let last = last_line(&content);
        if serde_json::from_str::<Envelope>(last).is_err() {
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

fn last_line(s: &str) -> &str {
    match s.rfind('\n') {
        Some(i) => &s[i + 1..],
        None => s,
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
    let suffix: String = ulid::Ulid::new()
        .to_string()
        .chars()
        .rev()
        .take(4)
        .collect();
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
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    if slug.is_empty() {
        "host".into()
    } else {
        slug
    }
}

pub fn now_rfc3339() -> String {
    use time::format_description::well_known::Rfc3339;
    time::OffsetDateTime::now_utc()
        .replace_nanosecond(0)
        .unwrap()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}
