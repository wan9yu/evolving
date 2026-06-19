//! Run-receipts: the non-hashed evidence that a bound test ran — one JSON object
//! per line in results/receipts/<test-key>.jsonl. Deleting receipts never changes a
//! tick id (the hashed/cached split). Unsigned, trust-on-write for 0.1.0.
use crate::store::Store;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::io::Write;

#[derive(Debug, Clone, PartialEq)]
pub struct Receipt {
    pub test: String,              // == the bound check ref, byte-for-byte
    pub platform: String,          // one of the binding's liveness.platforms
    pub commit: String,            // 40-hex sha
    pub ran_at: String,            // RFC 3339 UTC
    pub result: String,            // "green" | "red" | "gray"
    pub falsifiable: Option<bool>, // counter-test produced the opposite? set by --run; non-hashed
}

/// Stable, filesystem-safe key for a bound test's receipt log: first 12 hex of SHA-256(ref).
pub fn test_key(reference: &str) -> String {
    let digest = Sha256::digest(reference.as_bytes());
    hex::encode(&digest[..6])
}

/// Strict parse of one receipt line: closed schema + result enum.
pub fn from_value(v: &Value) -> Result<Receipt, String> {
    use crate::tick::{only_keys, req_str};
    let obj = v.as_object().ok_or("receipt is not an object")?;
    only_keys(
        obj,
        &[
            "test",
            "platform",
            "commit",
            "ran_at",
            "result",
            "falsifiable",
        ],
        "receipt",
    )?;
    let result = req_str(obj, "result")?;
    if !["green", "red", "gray"].contains(&result.as_str()) {
        return Err(format!("receipt.result must be green|red|gray: {result}"));
    }
    Ok(Receipt {
        test: req_str(obj, "test")?,
        platform: req_str(obj, "platform")?,
        commit: req_str(obj, "commit")?,
        ran_at: req_str(obj, "ran_at")?,
        result,
        falsifiable: obj.get("falsifiable").and_then(|x| x.as_bool()),
    })
}

fn to_line(r: &Receipt) -> String {
    let mut m = Map::new();
    m.insert("test".into(), Value::String(r.test.clone()));
    m.insert("platform".into(), Value::String(r.platform.clone()));
    m.insert("commit".into(), Value::String(r.commit.clone()));
    m.insert("ran_at".into(), Value::String(r.ran_at.clone()));
    m.insert("result".into(), Value::String(r.result.clone()));
    if let Some(b) = r.falsifiable {
        m.insert("falsifiable".into(), Value::Bool(b));
    }
    serde_json::to_string(&Value::Object(m)).expect("serializable")
}

/// Append one receipt as a JSON line to results/receipts/<test-key>.jsonl.
pub fn append(store: &Store, r: &Receipt) -> std::io::Result<()> {
    let dir = store.root.join("results").join("receipts");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.jsonl", test_key(&r.test)));
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(f, "{}", to_line(r))
}

/// Read every receipt for a bound test ref (its <test-key>.jsonl). Empty vec if the file is absent.
pub fn read_for(store: &Store, reference: &str) -> std::io::Result<Vec<Receipt>> {
    let path = store
        .root
        .join("results")
        .join("receipts")
        .join(format!("{}.jsonl", test_key(reference)));
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut out = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        out.push(
            from_value(&v).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?,
        );
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

    fn store() -> (std::path::PathBuf, Store) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "ev-receipt-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        let s = Store::at(&p);
        s.init().unwrap();
        (p, s)
    }

    fn receipt(platform: &str, ran_at: &str, result: &str) -> Receipt {
        Receipt {
            test: "pytest x".into(),
            platform: platform.into(),
            commit: "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901".into(),
            ran_at: ran_at.into(),
            result: result.into(),
            falsifiable: None,
        }
    }

    #[test]
    fn from_value_should_round_trip_falsifiable_when_present() {
        // given: a receipt line carrying falsifiable=false
        let v = serde_json::json!({
            "test": "pytest x", "platform": "linux-ci",
            "commit": "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901",
            "ran_at": "2026-01-01T00:00:00Z", "result": "green", "falsifiable": false
        });
        // when: parsed
        let r = from_value(&v).expect("valid");
        // then: falsifiable is preserved
        assert_eq!(r.falsifiable, Some(false));
    }

    #[test]
    fn from_value_should_default_falsifiable_to_none_when_absent() {
        // given: a receipt with no falsifiable field
        let v = serde_json::json!({
            "test": "pytest x", "platform": "linux-ci",
            "commit": "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901",
            "ran_at": "2026-01-01T00:00:00Z", "result": "green"
        });
        // when: parsed
        let r = from_value(&v).expect("valid");
        // then: falsifiable is None (absent = not evaluated)
        assert_eq!(r.falsifiable, None);
    }

    #[test]
    fn test_key_should_be_stable_and_12_hex_when_given_a_ref() {
        // given: a check reference
        let reference = "pytest tests/test_redis_absent.py";

        // when: its receipt-log key is computed twice
        let a = test_key(reference);
        let b = test_key(reference);

        // then: it is a stable 12-char lowercase-hex string
        assert_eq!(a, b);
        assert_eq!(a.len(), 12);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn append_then_read_for_should_round_trip_the_receipt_when_one_is_written() {
        // given: an initialized store and a green receipt
        let (_p, s) = store();
        let r = receipt("linux-ci", "2026-01-01T00:00:00Z", "green");

        // when: it is appended and read back by ref
        append(&s, &r).unwrap();
        let back = read_for(&s, "pytest x").unwrap();

        // then: exactly that receipt round-trips
        assert_eq!(back, vec![r]);
    }

    #[test]
    fn read_for_should_return_empty_when_no_receipt_file_exists() {
        // given: an initialized store with no receipts
        let (_p, s) = store();

        // when: receipts are read for an unbound ref
        let back = read_for(&s, "pytest never-run").unwrap();

        // then: the result is empty (absence is not an error)
        assert!(back.is_empty());
    }

    #[test]
    fn from_value_should_reject_the_receipt_when_result_is_not_in_the_enum() {
        // given: a receipt line whose result is outside green|red|gray
        let v = serde_json::json!({
            "test": "pytest x", "platform": "linux-ci",
            "commit": "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901",
            "ran_at": "2026-01-01T00:00:00Z", "result": "purple"
        });

        // when: it is parsed
        let parsed = from_value(&v);

        // then: parsing fails
        assert!(parsed.is_err());
    }
}
