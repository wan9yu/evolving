//! Best-effort lexical lints over built-in, deterministic word lists.
//! R3: the subject of self-evolve/self-improve language must be a human, not the system.
//! R5: no auto-close / auto-prune / self-stop op language.
//! Honest limit: a re-wording evades these — they are heuristics, not semantic guarantees.

const R3_VERBS: &[&str] = &[
    "self-evolve",
    "self-improve",
    "self-grade",
    "self-optimize",
    "self-tune",
    "self-evaluate",
];
const R5_OPS: &[&str] = &["auto-close", "auto-prune", "self-stop", "auto-inherit"];

fn hits(text: &str, words: &[&str]) -> Vec<String> {
    let lower = text.to_lowercase();
    words
        .iter()
        .filter(|w| lower.contains(*w))
        .map(|w| w.to_string())
        .collect()
}

/// R3: free-text fields that carry a self-evolve verb (subject should be a human).
pub fn r3_self_evolve(text: &str) -> Vec<String> {
    hits(text, R3_VERBS)
}

/// R5: free-text fields that describe a forbidden machine-initiated op.
pub fn r5_forbidden_op(text: &str) -> Vec<String> {
    hits(text, R5_OPS)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn r3_should_return_the_verb_when_the_system_is_a_self_evolve_subject() {
        // given: text where the system is the subject of a self-evolve verb
        let text = "the retrieval system will self-evolve its schema";

        // when: the R3 lint scans it
        let hits = r3_self_evolve(text);

        // then: it returns the offending verb
        assert_eq!(hits, vec!["self-evolve".to_string()]);
    }

    #[test]
    fn r3_should_return_no_hits_when_text_is_ordinary() {
        // given: ordinary text with no self-evolve verb
        let text = "build our own retrieval; reject pgvector";

        // when: the R3 lint scans it
        let hits = r3_self_evolve(text);

        // then: it returns no hits
        assert!(hits.is_empty());
    }

    #[test]
    fn r5_should_return_the_op_when_text_describes_an_auto_close() {
        // given: text describing a machine-initiated auto-close op
        let text = "the system will auto-close stale grounds";

        // when: the R5 lint scans it
        let hits = r5_forbidden_op(text);

        // then: it returns the forbidden op
        assert_eq!(hits, vec!["auto-close".to_string()]);
    }
}
