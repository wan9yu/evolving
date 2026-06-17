//! Best-effort lexical lints over built-in, deterministic word lists.
//! R3: the subject of self-evolve/self-improve language must be a human, not the system.
//! R5: no auto-close / auto-prune / self-stop op language.
//! Honest limit: a re-wording evades these — they are heuristics, not semantic guarantees.

const R3_VERBS: &[&str] = &["self-evolve", "self-improve", "self-grade", "self-optimize", "self-tune", "self-evaluate"];
const R5_OPS: &[&str] = &["auto-close", "auto-prune", "self-stop", "auto-inherit"];

fn hits(text: &str, words: &[&str]) -> Vec<String> {
    let lower = text.to_lowercase();
    words.iter().filter(|w| lower.contains(*w)).map(|w| w.to_string()).collect()
}

/// R3: free-text fields that carry a self-evolve verb (subject should be a human).
pub fn r3_self_evolve(text: &str) -> Vec<String> { hits(text, R3_VERBS) }

/// R5: free-text fields that describe a forbidden machine-initiated op.
pub fn r5_forbidden_op(text: &str) -> Vec<String> { hits(text, R5_OPS) }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn r3_flags_a_system_subject_self_evolve_phrase() {
        assert_eq!(r3_self_evolve("the retrieval system will self-evolve its schema"), vec!["self-evolve".to_string()]);
    }
    #[test]
    fn r3_is_quiet_on_ordinary_text() {
        assert!(r3_self_evolve("build our own retrieval; reject pgvector").is_empty());
    }
    #[test]
    fn r5_flags_an_auto_close_phrase() {
        assert_eq!(r5_forbidden_op("the system will auto-close stale grounds"), vec!["auto-close".to_string()]);
    }
}
