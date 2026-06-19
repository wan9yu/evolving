//! The .evolving/config reader: one typed Config parsed once from the flat `key = value`
//! file. Defaults match DEFAULT_CONFIG. No TOML dependency — the file is ev-authored and
//! fixed-shape, so a whole-token line scan is enough.
use crate::store::Store;

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub staleness_days: u64,
    pub green_exit_code: i32,
    pub staleness_ref: String, // "live-origin" | "local-head" | "none"
    pub brief_limit: usize,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            staleness_days: 7,
            green_exit_code: 0,
            staleness_ref: "live-origin".into(),
            brief_limit: 10,
        }
    }
}

/// The value of a `key = value` line (exact whole-key match), trimmed; None if absent.
fn value_of<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|line| {
        let (k, v) = line.split_once('=')?;
        (k.trim() == key).then_some(v.trim())
    })
}

fn unquote(s: &str) -> &str {
    s.strip_prefix('"')
        .and_then(|x| x.strip_suffix('"'))
        .unwrap_or(s)
}

/// Parse the store's config; any missing or malformed key falls back to its default.
pub fn read(store: &Store) -> Config {
    let text = std::fs::read_to_string(store.config_path()).unwrap_or_default();
    let d = Config::default();
    Config {
        staleness_days: value_of(&text, "staleness_days")
            .and_then(|v| v.parse().ok())
            .unwrap_or(d.staleness_days),
        green_exit_code: value_of(&text, "green_exit_code")
            .and_then(|v| v.parse().ok())
            .unwrap_or(d.green_exit_code),
        staleness_ref: value_of(&text, "staleness_ref")
            .map(|v| unquote(v).to_string())
            .unwrap_or(d.staleness_ref),
        brief_limit: value_of(&text, "brief_limit")
            .and_then(|v| v.parse().ok())
            .unwrap_or(d.brief_limit),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

    fn store() -> (std::path::PathBuf, Store) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "ev-config-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        let s = Store::at(&p);
        s.init().unwrap();
        (p, s)
    }

    #[test]
    fn read_should_parse_every_key_when_the_config_sets_them() {
        // given: a config that overrides all three keys
        let (_p, s) = store();
        std::fs::write(
            s.config_path(),
            "[runner]\ngreen_exit_code = 1\n\n[liveness]\nstaleness_days = 3\nstaleness_ref = \"local-head\"\n",
        )
        .unwrap();

        // when: the config is read
        let c = read(&s);

        // then: each typed field reflects the file
        assert_eq!(c.staleness_days, 3);
        assert_eq!(c.green_exit_code, 1);
        assert_eq!(c.staleness_ref, "local-head");
    }

    #[test]
    fn read_should_parse_brief_limit_when_present() {
        // given: a config that sets brief_limit
        let (_p, s) = store();
        std::fs::write(s.config_path(), "brief_limit = 5\n").unwrap();

        // when: the config is read
        let c = read(&s);

        // then: the typed field reflects the file
        assert_eq!(c.brief_limit, 5);
    }

    #[test]
    fn read_should_use_defaults_when_the_keys_are_absent() {
        // given: a config with none of the keys
        let (_p, s) = store();
        std::fs::write(s.config_path(), "schema_version = 1\n").unwrap();

        // when: the config is read
        let c = read(&s);

        // then: it falls back to the defaults
        assert_eq!(c, Config::default());
    }

    #[test]
    fn read_should_not_match_a_longer_key_when_a_prefix_collides() {
        // given: a config with only a longer key that shares the staleness_days prefix
        let (_p, s) = store();
        std::fs::write(s.config_path(), "staleness_days_extra = 99\n").unwrap();

        // when: the config is read
        let c = read(&s);

        // then: staleness_days is the default, not 99 (whole-token match)
        assert_eq!(c.staleness_days, 7);
    }

    #[test]
    fn read_should_equal_the_defaults_for_a_freshly_initialized_store() {
        // given: a store carrying the canonical DEFAULT_CONFIG that `init` writes
        let (_p, s) = store();

        // when: that default config is read back
        let c = read(&s);

        // then: it matches Config::default() — pins DEFAULT_CONFIG and Config::default() in lockstep
        assert_eq!(c, Config::default());
    }
}
