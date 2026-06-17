//! The .evolving/ store: a committed hashed chain + a non-hashed results cache.
use crate::tick::{full_value, Tick};
use std::fs;
use std::path::{Path, PathBuf};

pub struct Store {
    pub root: PathBuf, // <repo>/.evolving
}

const DEFAULT_CONFIG: &str = "schema_version = 1\n\n\
[runner]\n\
template = \"pytest {selector}\"\n\
green_exit_code = 0\n\n\
[liveness]\n\
platforms = [\"linux-ci\", \"mac\", \"ship-image\"]\n\
staleness_days = 7\n\
not_run_lookback_commits = 20\n";

impl Store {
    pub fn at(repo: &Path) -> Store {
        Store { root: repo.join(".evolving") }
    }
    pub fn ticks_dir(&self) -> PathBuf { self.root.join("ticks") }
    pub fn head_path(&self) -> PathBuf { self.root.join("HEAD") }
    pub fn config_path(&self) -> PathBuf { self.root.join("config") }
    pub fn exists(&self) -> bool { self.root.exists() }

    /// Create the layout. Returns Ok(true) if created, Ok(false) if it already existed (idempotent).
    pub fn init(&self) -> std::io::Result<bool> {
        if self.root.exists() {
            return Ok(false);
        }
        fs::create_dir_all(self.ticks_dir())?;
        fs::create_dir_all(self.root.join("results").join("receipts"))?;
        fs::create_dir_all(self.root.join("results").join("state"))?;
        fs::write(self.head_path(), "")?;
        fs::write(self.config_path(), DEFAULT_CONFIG)?;
        Ok(true)
    }

    /// Write a tick file (pretty JSON; the id is recomputed on verify, not from these bytes) and advance HEAD.
    pub fn write_tick(&self, t: &Tick) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(&full_value(t)).expect("serializable");
        fs::write(self.ticks_dir().join(&t.id), json)?;
        fs::write(self.head_path(), &t.id)?;
        Ok(())
    }

    /// Read every tick file as (filename, raw JSON Value). Order is unspecified.
    pub fn read_all(&self) -> std::io::Result<Vec<(String, serde_json::Value)>> {
        let mut out = Vec::new();
        for entry in fs::read_dir(self.ticks_dir())? {
            let p = entry?.path();
            if p.is_file() {
                let name = p.file_name().unwrap().to_string_lossy().to_string();
                let text = fs::read_to_string(&p)?;
                let v: serde_json::Value = serde_json::from_str(&text)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{name}: {e}")))?;
                out.push((name, v));
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tick::{Ground, Tick};

    fn tmp() -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("ev-store-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn a_tick(id: &str, parent: &str) -> Tick {
        Tick {
            id: id.into(), parent_id: parent.into(),
            observe: "o".into(), decision: "d".into(),
            grounds: vec![Ground { claim: "c".into(), supports: "chosen".into(), check: None }],
            status: "live".into(), held_since: "".into(), blame: "Wang Yu".into(),
        }
    }

    #[test]
    fn init_creates_the_store_layout() {
        let repo = tmp();
        let s = Store::at(&repo);
        assert!(s.init().unwrap()); // true = created
        assert!(s.ticks_dir().is_dir());
        assert!(s.head_path().is_file());
        assert!(s.config_path().is_file());
        assert!(repo.join(".evolving/results/receipts").is_dir());
    }

    #[test]
    fn init_is_idempotent() {
        let repo = tmp();
        let s = Store::at(&repo);
        assert!(s.init().unwrap());
        assert!(!s.init().unwrap()); // false = no-op, did not overwrite
    }

    #[test]
    fn writing_a_tick_persists_it_and_advances_head() {
        let repo = tmp();
        let s = Store::at(&repo);
        s.init().unwrap();
        let t = a_tick("aaaaaaaaaaaa", "");
        s.write_tick(&t).unwrap();
        assert!(s.ticks_dir().join("aaaaaaaaaaaa").is_file());
        assert_eq!(std::fs::read_to_string(s.head_path()).unwrap(), "aaaaaaaaaaaa");
        let all = s.read_all().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].0, "aaaaaaaaaaaa");
    }
}
