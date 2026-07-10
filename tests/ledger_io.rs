use evolving::ledger::{self, Actor, ActorKind, Ledger};
use std::fs;

fn tmp() -> std::path::PathBuf {
    let base = std::env::temp_dir().join(format!("ev-io-{}", ulid::Ulid::new()));
    fs::create_dir_all(base.join(".evolving/ledger")).unwrap();
    fs::create_dir_all(base.join(".evolving/local")).unwrap();
    base
}

fn ev(kind: &str, body: serde_json::Value) -> ledger::NewEvent {
    ledger::NewEvent {
        etype: kind.into(),
        actor: Actor {
            kind: ActorKind::Human,
            id: None,
            via: None,
        },
        body,
    }
}

#[test]
fn appended_batch_is_read_back_in_seq_order() {
    let root = tmp();
    let l = Ledger::open(&root).unwrap();
    l.append_batch(vec![ev("claim", serde_json::json!({"label":"a"}))])
        .unwrap();
    l.append_batch(vec![
        ev("claim", serde_json::json!({"label":"b"})),
        ev("evidence", serde_json::json!({"label":"c"})),
    ])
    .unwrap();
    let events = l.scan().unwrap();
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].seq, 1);
    assert_eq!(events[2].seq, 3);
}

#[test]
fn torn_trailing_line_is_skipped_not_fatal() {
    let root = tmp();
    let l = Ledger::open(&root).unwrap();
    l.append_batch(vec![ev("claim", serde_json::json!({"label":"a"}))])
        .unwrap();
    // simulate a killed mid-write: append a partial JSON line
    let wid = l.writer_id().to_string();
    let path = root.join(".evolving/ledger").join(format!("{wid}.jsonl"));
    let mut content = fs::read_to_string(&path).unwrap();
    content.push_str("{\"v\":2,\"id\":\"clm_partial\"");
    fs::write(&path, content).unwrap();
    let events = l.scan().unwrap(); // must not error
    assert_eq!(events.len(), 1);
}

#[test]
fn duplicate_ids_across_files_are_deduped() {
    let root = tmp();
    let l = Ledger::open(&root).unwrap();
    l.append_batch(vec![ev("claim", serde_json::json!({"label":"a"}))])
        .unwrap();
    let events = l.scan().unwrap();
    let line = serde_json::to_string(&events[0]).unwrap();
    // a second writer file carrying the same id
    fs::write(
        root.join(".evolving/ledger/other-1111.jsonl"),
        format!("{line}\n"),
    )
    .unwrap();
    let events = l.scan().unwrap();
    assert_eq!(events.len(), 1, "same id must dedupe");
}
