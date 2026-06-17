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
        Store {
            root: repo.join(".evolving"),
        }
    }
    pub fn ticks_dir(&self) -> PathBuf {
        self.root.join("ticks")
    }
    pub fn head_path(&self) -> PathBuf {
        self.root.join("HEAD")
    }
    pub fn config_path(&self) -> PathBuf {
        self.root.join("config")
    }
    pub fn exists(&self) -> bool {
        self.root.exists()
    }

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

    /// The current HEAD id ("" if genesis / empty store).
    pub fn read_head(&self) -> std::io::Result<String> {
        match std::fs::read_to_string(self.head_path()) {
            Ok(s) => Ok(s.trim().to_string()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
            Err(e) => Err(e),
        }
    }

    /// Read one tick (parsed) by id, or None if absent.
    pub fn read_tick(&self, id: &str) -> std::io::Result<Option<crate::tick::Tick>> {
        let p = self.ticks_dir().join(id);
        if !p.is_file() {
            return Ok(None);
        }
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&p)?)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        crate::tick::from_value(&v)
            .map(Some)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Read every tick file as (filename, raw JSON Value). Order is unspecified.
    pub fn read_all(&self) -> std::io::Result<Vec<(String, serde_json::Value)>> {
        let mut out = Vec::new();
        for entry in fs::read_dir(self.ticks_dir())? {
            let p = entry?.path();
            if p.is_file() {
                let name = p.file_name().unwrap().to_string_lossy().to_string();
                let text = fs::read_to_string(&p)?;
                let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{name}: {e}"))
                })?;
                out.push((name, v));
            }
        }
        Ok(out)
    }

    /// The cached live-origin sha (results/origin-sha), or None if absent/empty. No network.
    pub fn read_origin_sha(&self) -> Option<String> {
        std::fs::read_to_string(self.root.join("results").join("origin-sha"))
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    /// The staleness window in days from the `[liveness] staleness_days` config key (default 7).
    pub fn staleness_days(&self) -> u64 {
        std::fs::read_to_string(self.config_path())
            .ok()
            .and_then(|text| {
                text.lines().find_map(|line| {
                    let line = line.trim();
                    line.strip_prefix("staleness_days")
                        .and_then(|rest| rest.trim_start().strip_prefix('='))
                        .and_then(|v| v.trim().parse::<u64>().ok())
                })
            })
            .unwrap_or(7)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tick::{Ground, Tick};

    fn tmp() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "ev-store-test-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn a_tick(id: &str, parent: &str) -> Tick {
        Tick {
            id: id.into(),
            parent_id: parent.into(),
            observe: "o".into(),
            decision: "d".into(),
            grounds: vec![Ground {
                claim: "c".into(),
                supports: "chosen".into(),
                check: None,
            }],
            status: "live".into(),
            held_since: "".into(),
            blame: "Wang Yu".into(),
        }
    }

    #[test]
    fn init_should_create_the_full_store_layout_when_the_store_is_new() {
        // given: a store rooted at a fresh empty repo
        let repo = tmp();
        let s = Store::at(&repo);

        // when: the store is initialized
        let created = s.init().unwrap();

        // then: it reports creation and the full layout exists on disk
        assert!(created); // true = created
        assert!(s.ticks_dir().is_dir());
        assert!(s.head_path().is_file());
        assert!(s.config_path().is_file());
        assert!(repo.join(".evolving/results/receipts").is_dir());
    }

    #[test]
    fn init_should_be_a_no_op_when_the_store_already_exists() {
        // given: a store that has already been initialized
        let repo = tmp();
        let s = Store::at(&repo);
        assert!(s.init().unwrap());

        // when: init is called again
        let created_again = s.init().unwrap();

        // then: it reports no creation and does not overwrite
        assert!(!created_again); // false = no-op, did not overwrite
    }

    #[test]
    fn write_tick_should_persist_the_tick_and_advance_head_when_a_tick_is_written() {
        // given: an initialized store and a tick to write
        let repo = tmp();
        let s = Store::at(&repo);
        s.init().unwrap();
        let t = a_tick("aaaaaaaaaaaa", "");

        // when: the tick is written
        s.write_tick(&t).unwrap();

        // then: the tick file is persisted, HEAD advances to it, and it is the only tick
        assert!(s.ticks_dir().join("aaaaaaaaaaaa").is_file());
        assert_eq!(
            std::fs::read_to_string(s.head_path()).unwrap(),
            "aaaaaaaaaaaa"
        );
        let all = s.read_all().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].0, "aaaaaaaaaaaa");
    }

    #[test]
    fn read_origin_sha_should_return_the_trimmed_sha_when_the_cache_file_exists() {
        // given: an initialized store with a cached origin-sha file
        let repo = tmp();
        let s = Store::at(&repo);
        s.init().unwrap();
        std::fs::write(
            s.root.join("results").join("origin-sha"),
            "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901\n",
        )
        .unwrap();

        // when: the cached origin sha is read
        let sha = s.read_origin_sha();

        // then: it is the trimmed value
        assert_eq!(
            sha.as_deref(),
            Some("d308afac1b2c3d4e5f60718293a4b5c6d7e8f901")
        );
    }

    #[test]
    fn read_origin_sha_should_be_none_when_no_cache_file_exists() {
        // given: an initialized store with no origin-sha cache
        let repo = tmp();
        let s = Store::at(&repo);
        s.init().unwrap();

        // when: the cached origin sha is read
        let sha = s.read_origin_sha();

        // then: it is None (no network is consulted)
        assert!(sha.is_none());
    }

    #[test]
    fn staleness_days_should_read_the_configured_value_when_present() {
        // given: a store whose config sets staleness_days = 3
        let repo = tmp();
        let s = Store::at(&repo);
        s.init().unwrap();
        let cfg = std::fs::read_to_string(s.config_path())
            .unwrap()
            .replace("staleness_days = 7", "staleness_days = 3");
        std::fs::write(s.config_path(), cfg).unwrap();

        // when: the staleness window is read
        let days = s.staleness_days();

        // then: it is the configured value
        assert_eq!(days, 3);
    }

    #[test]
    fn staleness_days_should_default_to_7_when_the_key_is_absent() {
        // given: a store whose config has no staleness_days line
        let repo = tmp();
        let s = Store::at(&repo);
        s.init().unwrap();
        std::fs::write(s.config_path(), "schema_version = 1\n").unwrap();

        // when: the staleness window is read
        let days = s.staleness_days();

        // then: it falls back to the 7-day default
        assert_eq!(days, 7);
    }
}
