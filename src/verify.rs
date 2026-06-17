//! ev verify (Phase 1 subset): R1 (closed schema), R2 (check shape), R4/R6
//! (id == hash + chain integrity). R3/R5 lexical lints arrive in Plan 2.
use crate::canonical::compute_id;
use crate::store::Store;
use crate::tick::from_value;
use std::collections::{HashMap, HashSet};

/// Returns the list of violations (empty == clean). Reports ALL of them.
pub fn verify(store: &Store) -> std::io::Result<Vec<String>> {
    let mut violations = Vec::new();
    let files = store.read_all()?;
    let mut ids: HashSet<String> = HashSet::new();
    let mut parent_of: HashMap<String, String> = HashMap::new();

    for (filename, raw) in &files {
        match from_value(raw) {
            Err(e) => violations.push(format!("{filename}: R1/R2 {e}")),
            Ok(t) => {
                let recomputed = compute_id(&t);
                if recomputed != *filename {
                    violations.push(format!("{filename}: id != hash(payload) (R4/R6) — recomputed {recomputed}"));
                }
                if t.id != *filename {
                    violations.push(format!("{filename}: stored id field {} != filename (R6)", t.id));
                }
                ids.insert(filename.clone());
                parent_of.insert(filename.clone(), t.parent_id.clone());
            }
        }
    }

    // Chain (R6): parent resolves; genesis "" ok; forward-only / acyclic.
    for (id, parent) in &parent_of {
        if parent.is_empty() {
            continue;
        }
        if !ids.contains(parent) {
            violations.push(format!("{id}: parent_id {parent} does not resolve (R6)"));
        }
    }
    for start in parent_of.keys() {
        let mut seen = HashSet::new();
        let mut cur = start.clone();
        loop {
            if !seen.insert(cur.clone()) {
                violations.push(format!("{start}: parent chain has a cycle (R6)"));
                break;
            }
            match parent_of.get(&cur) {
                Some(p) if !p.is_empty() && ids.contains(p) => cur = p.clone(),
                _ => break,
            }
        }
    }

    Ok(violations)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical::compute_id;
    use crate::store::Store;
    use crate::tick::{Ground, Tick};

    fn tmp() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "ev-verify-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }
    fn tick(parent: &str) -> Tick {
        let mut t = Tick {
            id: String::new(), parent_id: parent.into(),
            observe: "o".into(), decision: "d".into(),
            grounds: vec![Ground { claim: "c".into(), supports: "chosen".into(), check: None }],
            status: "live".into(), held_since: "".into(), blame: "Wang Yu".into(),
        };
        t.id = compute_id(&t);
        t
    }

    #[test]
    fn verify_passes_a_clean_two_tick_chain() {
        let repo = tmp();
        let s = Store::at(&repo);
        s.init().unwrap();
        let g = tick("");
        s.write_tick(&g).unwrap();
        let child = tick(&g.id);
        s.write_tick(&child).unwrap();
        assert!(verify(&s).unwrap().is_empty());
    }

    #[test]
    fn verify_flags_a_hand_edited_tick() {
        let repo = tmp();
        let s = Store::at(&repo);
        s.init().unwrap();
        let g = tick("");
        s.write_tick(&g).unwrap();
        // tamper: change decision text on disk without changing the filename/id
        let p = s.ticks_dir().join(&g.id);
        let text = std::fs::read_to_string(&p).unwrap().replace("\"d\"", "\"TAMPERED\"");
        std::fs::write(&p, text).unwrap();
        let v = verify(&s).unwrap();
        assert!(v.iter().any(|x| x.contains("id != hash")));
    }

    #[test]
    fn verify_flags_an_unresolved_parent() {
        let repo = tmp();
        let s = Store::at(&repo);
        s.init().unwrap();
        let orphan = tick("deadbeefdead");
        s.write_tick(&orphan).unwrap();
        let v = verify(&s).unwrap();
        assert!(v.iter().any(|x| x.contains("does not resolve")));
    }

    #[test]
    fn verify_flags_a_field_outside_the_closed_schema() {
        let repo = tmp();
        let s = Store::at(&repo);
        s.init().unwrap();
        let g = tick("");
        s.write_tick(&g).unwrap();
        let p = s.ticks_dir().join(&g.id);
        let text = std::fs::read_to_string(&p).unwrap().replace("\"status\"", "\"health\"");
        std::fs::write(&p, text).unwrap();
        let v = verify(&s).unwrap();
        assert!(v.iter().any(|x| x.contains("closed schema")));
    }
}
