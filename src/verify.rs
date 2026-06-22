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
                    violations.push(format!(
                        "{filename}: id != hash(payload) (R4/R6) — recomputed {recomputed}"
                    ));
                }
                if t.id != *filename {
                    violations.push(format!(
                        "{filename}: stored id field {} != filename (R6)",
                        t.id
                    ));
                }
                ids.insert(filename.clone());
                parent_of.insert(filename.clone(), t.parent_id.clone());
                // R5: every tick names a human.
                if t.blame.trim().is_empty() {
                    violations.push(format!(
                        "{filename}: empty blame (R5) — every mutating op names a human"
                    ));
                }
                // LOCK 2 (at-rest, structural): a C/D-jurisdiction (detect-only) tick may carry NO
                // Test check on any ground — a detect-only decision must not be able to gate, so it
                // must hold no runnable test binding. A distinct invariant from no-vacuous-binding;
                // shares one predicate with the migrate ingest gate so the two can never drift.
                if crate::tick::detect_only_carries_test(t.jurisdiction.as_deref(), &t.grounds) {
                    violations.push(format!(
                        "{filename}: a C/D jurisdiction (detect-only) tick may carry no test check"
                    ));
                }
                // R3 / R5 lexical lints over the free-text fields (best-effort; a re-wording evades).
                // PROVENANCE PARTITION: only the R5 op-arm softens, and only for `imported` — a tick
                // that faithfully transcribes historical text is not authoring a forbidden op now, so
                // an op-word in it is surfaced as a non-gating warning (see `imported_op_warnings`),
                // never a violation. EVERY other arm stays hard for all provenance including imported:
                // R3 self-evolve here, plus empty-blame and C/D-no-test above. `agent-proposed` and
                // `human-now` (the absent default) keep the R5 op-arm a hard violation.
                let imported = t.provenance.as_deref() == Some("imported");
                let mut texts = vec![t.decision.clone(), t.observe.clone()];
                texts.extend(t.grounds.iter().map(|g| g.claim.clone()));
                for text in &texts {
                    for verb in crate::lint::r3_self_evolve(text) {
                        violations.push(format!("{filename}: R3 self-evolve subject \"{verb}\" should be a human (best-effort lint)"));
                    }
                    if imported {
                        continue; // imported R5 op-words are warnings, not violations
                    }
                    for op in crate::lint::r5_forbidden_op(text) {
                        violations.push(format!(
                            "{filename}: R5 forbidden op language \"{op}\" (best-effort lint)"
                        ));
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

/// Forward-compat surfacing (T3): a `warning:` (NOT a violation) per tolerated unknown top-level
/// key, so a typo'd field name stays visible instead of silently parsing through. `schema_version`
/// is read LAZILY here — at the tolerate-vs-reject decision — so a future reader can sharpen the
/// rule per declared baseline without making schema_version a parsed `Config` field.
pub fn unknown_key_warnings(store: &Store) -> std::io::Result<Vec<String>> {
    let baseline = crate::config::schema_version(store);
    let mut warnings = Vec::new();
    for (filename, raw) in &store.read_all()? {
        let Some(obj) = raw.as_object() else { continue };
        for key in crate::tick::unknown_top_level_keys(obj) {
            warnings.push(format!(
                "{filename}: warning: tolerated unknown top-level field {key:?} (schema_version {baseline}) — a typo'd field name parses through but is ignored"
            ));
        }
    }
    Ok(warnings)
}

/// The provenance-partitioned R5 surfacing: an `imported` tick faithfully transcribes historical text,
/// so an R5 forbidden-op word in it is a non-gating `warning:` (recorded, not authored now), NOT a
/// violation — `verify` skips it as a violation for imported ticks, and this surfaces it instead so the
/// op-word stays visible with a named human still on the hook. Fresh authorship (`human-now` /
/// `agent-proposed`) keeps the op-arm a hard violation in `verify`, and every other arm stays hard.
pub fn imported_op_warnings(store: &Store) -> std::io::Result<Vec<String>> {
    let mut warnings = Vec::new();
    for (filename, raw) in &store.read_all()? {
        let Ok(t) = from_value(raw) else { continue };
        if t.provenance.as_deref() != Some("imported") {
            continue;
        }
        let mut texts = vec![t.decision.clone(), t.observe.clone()];
        texts.extend(t.grounds.iter().map(|g| g.claim.clone()));
        for text in &texts {
            for op in crate::lint::r5_forbidden_op(text) {
                warnings.push(format!(
                    "{filename}: warning: R5 op language \"{op}\" in imported historical text (recorded, not authored — best-effort lint)"
                ));
            }
        }
    }
    Ok(warnings)
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
            id: String::new(),
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
            authority: None,
            jurisdiction: None,
            source_ref: None,
            provenance: None,
            corrects: None,
            ratifies: None,
        };
        t.id = compute_id(&t);
        t
    }

    #[test]
    fn verify_should_return_no_violations_when_the_chain_is_a_clean_two_tick_chain() {
        // given: a store with a genesis tick and a child tick that links to it
        let repo = tmp();
        let s = Store::at(&repo);
        s.init().unwrap();
        let g = tick("");
        s.write_tick(&g).unwrap();
        let child = tick(&g.id);
        s.write_tick(&child).unwrap();

        // when: verify scans the store
        let v = verify(&s).unwrap();

        // then: there are no violations
        assert!(v.is_empty());
    }

    #[test]
    fn verify_should_flag_id_not_hash_when_a_tick_is_hand_edited_on_disk() {
        // given: a stored genesis tick whose decision text is tampered without changing the filename/id
        let repo = tmp();
        let s = Store::at(&repo);
        s.init().unwrap();
        let g = tick("");
        s.write_tick(&g).unwrap();
        let p = s.ticks_dir().join(&g.id);
        let text = std::fs::read_to_string(&p)
            .unwrap()
            .replace("\"d\"", "\"TAMPERED\"");
        std::fs::write(&p, text).unwrap();

        // when: verify scans the store
        let v = verify(&s).unwrap();

        // then: it reports an id != hash violation
        assert!(v.iter().any(|x| x.contains("id != hash")));
    }

    #[test]
    fn verify_should_flag_an_unresolved_parent_when_a_tick_points_at_a_missing_parent() {
        // given: a store with a tick whose parent_id does not exist
        let repo = tmp();
        let s = Store::at(&repo);
        s.init().unwrap();
        let orphan = tick("deadbeefdead");
        s.write_tick(&orphan).unwrap();

        // when: verify scans the store
        let v = verify(&s).unwrap();

        // then: it reports an unresolved-parent violation
        assert!(v.iter().any(|x| x.contains("does not resolve")));
    }

    #[test]
    fn verify_should_flag_a_closed_schema_violation_when_the_hashed_payload_has_a_field_outside_the_schema(
    ) {
        // given: a stored genesis tick whose ground claim (hashed payload) is renamed on disk to an
        // unknown field — the hashed payload stays a STRICTLY closed schema (the two-tier rule
        // tolerates unknown TOP-LEVEL keys, never unknown keys inside the hashed payload)
        let repo = tmp();
        let s = Store::at(&repo);
        s.init().unwrap();
        let g = tick("");
        s.write_tick(&g).unwrap();
        let p = s.ticks_dir().join(&g.id);
        let text = std::fs::read_to_string(&p)
            .unwrap()
            .replace("\"claim\"", "\"health\"");
        std::fs::write(&p, text).unwrap();

        // when: verify scans the store
        let v = verify(&s).unwrap();

        // then: it reports a closed-schema violation
        assert!(v.iter().any(|x| x.contains("closed schema")));
    }

    #[test]
    fn verify_should_flag_an_r3_violation_when_a_tick_decision_has_a_system_subject_self_evolve() {
        // given: a stored tick whose decision text names the system as the subject of a self-evolve verb
        let repo = tmp();
        let s = Store::at(&repo);
        s.init().unwrap();
        let mut t = tick("");
        t.decision = "the index will self-improve its own ranking".into();
        t.id = compute_id(&t);
        s.write_tick(&t).unwrap();

        // when: verify scans the store
        let v = verify(&s).unwrap();

        // then: it reports an R3 self-evolve violation
        assert!(v
            .iter()
            .any(|x| x.contains("self-improve") || x.to_lowercase().contains("r3")));
    }

    #[test]
    fn verify_should_reject_a_c_tagged_tick_that_carries_a_test_check() {
        // given: a stored tick tagged jurisdiction=C whose ground carries a Test check
        use crate::tick::{Check, Liveness};
        let repo = tmp();
        let s = Store::at(&repo);
        s.init().unwrap();
        let mut t = tick("");
        t.jurisdiction = Some("C".into());
        t.grounds[0].check = Some(Check::Test {
            reference: "pytest x".into(),
            verified_at_sha: "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901".into(),
            counter_test: Some("pytest x::flips".into()),
            liveness: Liveness {
                platforms: vec!["linux-ci".into()],
                triggered_by: vec!["f".into()],
                surfaces: vec!["s".into()],
            },
        });
        t.id = compute_id(&t);
        s.write_tick(&t).unwrap();

        // when: verify scans the store
        let v = verify(&s).unwrap();

        // then: it reports a C/D-carries-a-test violation (a detect-only jurisdiction may not gate)
        assert!(
            v.iter()
                .any(|x| x.to_lowercase().contains("jurisdiction")
                    && x.to_lowercase().contains("test")),
            "expected a C/D-with-test violation; got: {v:?}"
        );
    }

    #[test]
    fn verify_should_accept_a_c_tagged_tick_when_it_carries_no_test_check() {
        // given: a stored tick tagged jurisdiction=C whose grounds carry no Test check
        let repo = tmp();
        let s = Store::at(&repo);
        s.init().unwrap();
        let mut t = tick("");
        t.jurisdiction = Some("C".into());
        t.id = compute_id(&t);
        s.write_tick(&t).unwrap();

        // when: verify scans the store
        let v = verify(&s).unwrap();

        // then: there are no violations (a test-free C tick is well-formed)
        assert!(v.is_empty(), "unexpected violations: {v:?}");
    }

    // The R5 op-word the lint catches; isolated here so the provenance partition is the only variable.
    const OP_TEXT: &str = "the stale cron tracker will auto-close after a week";

    fn op_tick_with_provenance(
        provenance: Option<&str>,
    ) -> (std::path::PathBuf, Store, Vec<String>) {
        let repo = tmp();
        let s = Store::at(&repo);
        s.init().unwrap();
        let mut t = tick("");
        t.decision = OP_TEXT.into();
        t.provenance = provenance.map(String::from);
        t.id = compute_id(&t);
        s.write_tick(&t).unwrap();
        let v = verify(&s).unwrap();
        (repo, s, v)
    }

    #[test]
    fn verify_should_warn_not_violate_on_an_op_word_when_provenance_is_imported() {
        // given/when: an imported tick whose transcribed text carries an R5 op-word
        let (_repo, s, v) = op_tick_with_provenance(Some("imported"));

        // then: it is NOT a gating violation, and the op-word is surfaced as a non-gating warning
        assert!(
            !v.iter().any(|x| x.contains("R5 forbidden op")),
            "imported history must not gate on an op-word; got: {v:?}"
        );
        let w = imported_op_warnings(&s).unwrap();
        assert!(
            w.iter()
                .any(|x| x.contains("auto-close") && x.contains("warning")),
            "the op-word must surface as a warning; got: {w:?}"
        );
    }

    #[test]
    fn verify_should_still_violate_on_an_op_word_when_provenance_is_agent_proposed() {
        // given/when: an agent-proposed tick with the same op-word (a live agent draft, not history)
        let (_repo, _s, v) = op_tick_with_provenance(Some("agent-proposed"));

        // then: it is a hard violation (a live agent draft must not smuggle op-language)
        assert!(
            v.iter().any(|x| x.contains("R5 forbidden op")),
            "agent-proposed must keep the op-arm hard; got: {v:?}"
        );
    }

    #[test]
    fn verify_should_still_violate_on_an_op_word_when_provenance_is_human_now() {
        // given/when: a fresh human-now tick (absent provenance) with the same op-word
        let (_repo, _s, v) = op_tick_with_provenance(None);

        // then: it is a hard violation (fresh authorship keeps the op-arm hard)
        assert!(
            v.iter().any(|x| x.contains("R5 forbidden op")),
            "human-now must keep the op-arm hard; got: {v:?}"
        );
    }

    #[test]
    fn verify_should_keep_empty_blame_and_c_d_no_test_hard_even_when_imported() {
        // given: an IMPORTED tick that ALSO violates a hard arm — blanked blame and a C/D test check
        use crate::tick::{Check, Liveness};
        let repo = tmp();
        let s = Store::at(&repo);
        s.init().unwrap();
        let mut t = tick("");
        t.provenance = Some("imported".into());
        t.jurisdiction = Some("C".into());
        t.blame = "".into();
        t.grounds[0].check = Some(Check::Test {
            reference: "pytest x".into(),
            verified_at_sha: "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901".into(),
            counter_test: Some("pytest x::flips".into()),
            liveness: Liveness {
                platforms: vec!["linux-ci".into()],
                triggered_by: vec!["f".into()],
                surfaces: vec!["s".into()],
            },
        });
        t.id = compute_id(&t);
        s.write_tick(&t).unwrap();

        // when: verify scans the store
        let v = verify(&s).unwrap();

        // then: the hard arms still fire for imported — only the R5 lexical op-arm ever softens
        assert!(
            v.iter().any(|x| x.contains("empty blame")),
            "empty-blame stays hard for imported; got: {v:?}"
        );
        assert!(
            v.iter()
                .any(|x| x.to_lowercase().contains("jurisdiction")
                    && x.to_lowercase().contains("test")),
            "C/D-no-test stays hard for imported; got: {v:?}"
        );
    }

    #[test]
    fn verify_should_flag_an_empty_blame_when_a_tick_blame_is_blanked_on_disk() {
        // given: a stored tick whose blame is blanked on disk (excluded from hash, so id stays valid)
        let repo = tmp();
        let s = Store::at(&repo);
        s.init().unwrap();
        let t = tick("");
        s.write_tick(&t).unwrap();
        let p = s.ticks_dir().join(&t.id);
        let text = std::fs::read_to_string(&p)
            .unwrap()
            .replace("\"Wang Yu\"", "\"\"");
        std::fs::write(&p, text).unwrap();

        // when: verify scans the store
        let v = verify(&s).unwrap();

        // then: it reports an empty-blame violation
        assert!(v.iter().any(|x| x.to_lowercase().contains("blame")));
    }

    #[test]
    fn unknown_key_warnings_should_warn_but_not_violate_when_a_tick_carries_a_tolerated_unknown_key(
    ) {
        // given: a stored tick that carries a tolerated unknown (forward-compat) top-level key,
        // added on disk — the key is non-hashed, so the content-addressed id stays valid
        let repo = tmp();
        let s = Store::at(&repo);
        s.init().unwrap();
        let t = tick("");
        s.write_tick(&t).unwrap();
        let p = s.ticks_dir().join(&t.id);
        let text = std::fs::read_to_string(&p)
            .unwrap()
            .replace("\"blame\"", "\"future_field\": \"x\",\n  \"blame\"");
        std::fs::write(&p, text).unwrap();

        // when: verify scans the store and warnings are collected
        let v = verify(&s).unwrap();
        let w = unknown_key_warnings(&s).unwrap();

        // then: there is no violation (the unknown key is tolerated) but a warning names the key
        assert!(
            v.is_empty(),
            "a tolerated unknown key must not violate: {v:?}"
        );
        assert!(
            w.iter()
                .any(|x| x.contains("future_field") && x.contains("warning")),
            "expected a warning naming the tolerated key; got: {w:?}"
        );
    }
}
