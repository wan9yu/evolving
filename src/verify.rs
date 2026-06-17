//! ev verify: R1 (closed schema), R2 (check shape), R4/R6 (id == hash + chain
//! integrity), R3 (self-evolve subject) + R5 (blame present + forbidden-op).
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
                // R5: every tick names a human.
                if t.blame.trim().is_empty() {
                    violations.push(format!("{filename}: empty blame (R5) — every mutating op names a human"));
                }
                // R3 / R5 lexical lints over the free-text fields (best-effort; a re-wording evades).
                let mut texts = vec![t.decision.clone(), t.observe.clone()];
                texts.extend(t.grounds.iter().map(|g| g.claim.clone()));
                for text in &texts {
                    for verb in crate::lint::r3_self_evolve(text) {
                        violations.push(format!("{filename}: R3 self-evolve subject \"{verb}\" should be a human (best-effort lint)"));
                    }
                    for op in crate::lint::r5_forbidden_op(text) {
                        violations.push(format!("{filename}: R5 forbidden op language \"{op}\" (best-effort lint)"));
                    }
                }
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

    #[test]
    fn verify_flags_a_system_subject_self_evolve_tick() {
        let repo = tmp();
        let s = Store::at(&repo);
        s.init().unwrap();
        let mut t = tick("");
        t.decision = "the index will self-improve its own ranking".into();
        t.id = compute_id(&t);
        s.write_tick(&t).unwrap();
        let v = verify(&s).unwrap();
        assert!(v.iter().any(|x| x.contains("self-improve") || x.to_lowercase().contains("r3")));
    }

    #[test]
    fn verify_flags_a_tick_with_empty_blame() {
        let repo = tmp();
        let s = Store::at(&repo);
        s.init().unwrap();
        let t = tick("");
        s.write_tick(&t).unwrap();
        // blank the blame on disk (excluded from hash, so id stays valid)
        let p = s.ticks_dir().join(&t.id);
        let text = std::fs::read_to_string(&p).unwrap().replace("\"Wang Yu\"", "\"\"");
        std::fs::write(&p, text).unwrap();
        let v = verify(&s).unwrap();
        assert!(v.iter().any(|x| x.to_lowercase().contains("blame")));
    }
}
