//! The external selected-list: which check refs the latest diff selected, and which declared
//! triggers it changed. An affinity tool / CI writes results/selected.json; ev READS it and
//! never recomputes affinity. Absent ⇒ L2 (silently-unbound) is not evaluated.
use crate::store::Store;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SelectedList {
    pub commit: String,        // the diff's commit (informational)
    pub changed: Vec<String>,  // declared triggers/paths the diff touched
    pub selected: Vec<String>, // check refs the diff selected
}

fn str_array(obj: &serde_json::Map<String, Value>, k: &str) -> Result<Vec<String>, String> {
    match obj.get(k) {
        None => Ok(Vec::new()),
        Some(Value::Array(a)) => a
            .iter()
            .map(|e| {
                e.as_str()
                    .map(|s| s.to_string())
                    .ok_or(format!("selected-list.{k} element is not a string"))
            })
            .collect(),
        Some(_) => Err(format!("selected-list.{k} must be an array")),
    }
}

/// Strict parse of a selected-list value (closed schema; missing arrays default to empty).
pub fn from_value(v: &Value) -> Result<SelectedList, String> {
    let obj = v.as_object().ok_or("selected-list is not an object")?;
    for k in obj.keys() {
        if !["commit", "changed", "selected"].contains(&k.as_str()) {
            return Err(format!("selected-list: field outside closed schema: {k}"));
        }
    }
    let commit = obj
        .get("commit")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    Ok(SelectedList {
        commit,
        changed: str_array(obj, "changed")?,
        selected: str_array(obj, "selected")?,
    })
}

/// Read results/selected.json, or None if it is absent. Errors on malformed JSON / schema.
pub fn read(store: &Store) -> std::io::Result<Option<SelectedList>> {
    let path = store.root.join("results").join("selected.json");
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let v: Value = serde_json::from_str(&text)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    from_value(&v)
        .map(Some)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

    fn store() -> (std::path::PathBuf, Store) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "ev-selected-{}-{}",
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
    fn read_should_parse_the_list_when_results_selected_json_exists() {
        // given: a store with a written selected-list
        let (_p, s) = store();
        std::fs::write(
            s.root.join("results").join("selected.json"),
            r#"{"commit":"d308afac1b2c3d4e5f60718293a4b5c6d7e8f901","changed":["pyproject.toml"],"selected":["pytest x"]}"#,
        )
        .unwrap();

        // when: the selected-list is read
        let sl = read(&s).unwrap().expect("present");

        // then: its changed and selected sets round-trip
        assert_eq!(sl.changed, vec!["pyproject.toml".to_string()]);
        assert_eq!(sl.selected, vec!["pytest x".to_string()]);
    }

    #[test]
    fn read_should_be_none_when_no_selected_list_exists() {
        // given: a store with no selected-list
        let (_p, s) = store();

        // when: the selected-list is read
        let sl = read(&s).unwrap();

        // then: it is None (L2 simply not evaluated)
        assert!(sl.is_none());
    }

    #[test]
    fn from_value_should_reject_the_list_when_it_has_an_unknown_field() {
        // given: a selected-list value with a field outside the closed schema
        let v = serde_json::json!({ "commit": "x", "selected": [], "health": 1 });

        // when: it is parsed
        let parsed = from_value(&v);

        // then: parsing fails
        assert!(parsed.is_err());
    }
}
