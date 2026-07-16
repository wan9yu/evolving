use std::process::Command;

fn run(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ev"))
        .args(args)
        .current_dir(dir)
        .env_remove("CLAUDECODE")
        .output()
        .unwrap()
}

fn fresh() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ev-reading-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    Command::new("git")
        .args(["init", "-q"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(run(&dir, &["init"]).status.success());
    dir
}

fn ledger_events(dir: &std::path::Path) -> Vec<serde_json::Value> {
    let p = std::fs::read_dir(dir.join(".evolving/ledger"))
        .unwrap()
        .filter_map(|e| e.ok())
        .find(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
        .unwrap()
        .path();
    std::fs::read_to_string(p)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn claim_id(dir: &std::path::Path) -> String {
    ledger_events(dir)
        .into_iter()
        .find(|e| e["type"] == "claim")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn reading_attaches_a_url_slot_and_lists_the_grid_with_empties() {
    let dir = fresh();
    assert!(run(&dir, &["claim", "c", "--by", "agent"]).status.success());
    let id = claim_id(&dir);

    let out = run(
        &dir,
        &[
            "reading",
            &id,
            "--depth",
            "plain",
            "--lang",
            "zh",
            "url:docs/x.md",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let ev = ledger_events(&dir)
        .into_iter()
        .rfind(|e| e["type"] == "reading")
        .unwrap();
    assert_eq!(ev["body"]["ref"].as_str().unwrap(), "url:docs/x.md");

    let list = run(&dir, &["reading", &id]);
    let s = String::from_utf8_lossy(&list.stdout);
    assert!(s.contains("plain/zh"), "the filled slot is listed: {s}");
    assert!(
        s.contains("plain/en") && s.contains("(empty)"),
        "an unfilled slot is stated as empty — a fact, not a grade: {s}"
    );
    assert!(
        !s.to_lowercase().contains("quality") && !s.to_lowercase().contains("score"),
        "ev never grades a slot: {s}"
    );
}

#[test]
fn reading_refuses_inline_prose_and_a_non_pointer_ref() {
    // R1: a slot holds a POINTER, never text; and never a file:/commit:/metric: ref.
    let dir = fresh();
    assert!(run(&dir, &["claim", "c", "--by", "agent"]).status.success());
    let id = claim_id(&dir);

    let prose = run(
        &dir,
        &[
            "reading",
            &id,
            "--depth",
            "plain",
            "--lang",
            "en",
            "it parses the header",
        ],
    );
    assert_eq!(prose.status.code(), Some(1), "prose in a slot is refused");
    assert!(String::from_utf8_lossy(&prose.stderr).contains("pointer"));

    let fileref = run(
        &dir,
        &[
            "reading",
            &id,
            "--depth",
            "plain",
            "--lang",
            "en",
            "file:a.txt::x",
        ],
    );
    assert_eq!(
        fileref.status.code(),
        Some(1),
        "a file: ref is not a reading pointer"
    );
}

#[test]
fn reading_refuses_maintainer_as_a_stored_slot() {
    // maintainer is the claim body itself — implicit, never a filed ref.
    let dir = fresh();
    assert!(run(&dir, &["claim", "c", "--by", "agent"]).status.success());
    let id = claim_id(&dir);
    let out = run(
        &dir,
        &[
            "reading",
            &id,
            "--depth",
            "maintainer",
            "--lang",
            "zh",
            "url:x",
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "maintainer is the claim proper, not a slot"
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("claim body"));
}

#[test]
fn a_second_fill_appends_and_never_rewrites() {
    // R4: two fills of the same slot leave two events on disk; the fold shows the latest.
    let dir = fresh();
    assert!(run(&dir, &["claim", "c", "--by", "agent"]).status.success());
    let id = claim_id(&dir);
    assert!(run(
        &dir,
        &["reading", &id, "--depth", "ground", "--lang", "en", "url:one"]
    )
    .status
    .success());
    assert!(run(
        &dir,
        &["reading", &id, "--depth", "ground", "--lang", "en", "url:two"]
    )
    .status
    .success());

    let readings: Vec<_> = ledger_events(&dir)
        .into_iter()
        .filter(|e| e["type"] == "reading")
        .collect();
    assert_eq!(
        readings.len(),
        2,
        "each fill is a new event — nothing is rewritten"
    );
    assert_eq!(
        readings[0]["body"]["ref"].as_str().unwrap(),
        "url:one",
        "the first event's bytes are frozen"
    );
}

#[test]
fn reading_refuses_a_concept_combined_with_a_slot_but_concept_alone_still_works() {
    // A concept pointer and a slot assignment are separate dispatch paths; combining them
    // must not silently drop the slot half.
    let dir = fresh();
    assert!(run(&dir, &["claim", "c", "--by", "agent"]).status.success());
    let id = claim_id(&dir);

    let combined = run(
        &dir,
        &[
            "reading",
            &id,
            "--concept",
            "url:x",
            "--depth",
            "plain",
            "--lang",
            "en",
            "url:y",
        ],
    );
    assert_eq!(
        combined.status.code(),
        Some(1),
        "a concept plus a slot assignment together is refused, not partially applied"
    );

    let concept_only = run(&dir, &["reading", &id, "--concept", "url:x"]);
    assert!(
        concept_only.status.success(),
        "{}",
        String::from_utf8_lossy(&concept_only.stderr)
    );
}

#[test]
fn reading_refuses_a_thk_ref_with_no_matching_note() {
    // guard_slot_ref's thk_ branch: a reference that starts with thk_ but resolves to no
    // note is refused, naming the fact that no think note matches.
    let dir = fresh();
    assert!(run(&dir, &["claim", "c", "--by", "agent"]).status.success());
    let id = claim_id(&dir);

    let out = run(
        &dir,
        &[
            "reading",
            &id,
            "--depth",
            "plain",
            "--lang",
            "zh",
            "thk_doesnotexist",
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "a thk_ ref with no matching note is refused"
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("no think note matches"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn a_thk_slot_resolves_to_the_note_text_and_a_url_slot_to_its_link() {
    // R1's two pointer kinds both resolve at display.
    let dir = fresh();
    assert!(run(&dir, &["claim", "c", "--by", "agent"]).status.success());
    let id = claim_id(&dir);
    assert!(
        run(&dir, &["think", "the header is parsed before the body"])
            .status
            .success()
    );
    let thk = ledger_events(&dir)
        .into_iter()
        .find(|e| e["type"] == "thought")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    assert!(run(
        &dir,
        &["reading", &id, "--depth", "plain", "--lang", "en", &thk]
    )
    .status
    .success());
    assert!(run(
        &dir,
        &[
            "reading",
            &id,
            "--depth",
            "ground",
            "--lang",
            "en",
            "url:docs/parse.md"
        ]
    )
    .status
    .success());

    let s = String::from_utf8_lossy(&run(&dir, &["reading", &id]).stdout).to_string();
    assert!(
        s.contains("the header is parsed before the body"),
        "a thk_ slot resolves to the note's text: {s}"
    );
    assert!(
        s.contains("url:docs/parse.md") || s.contains("docs/parse.md"),
        "a url: slot resolves to its link: {s}"
    );
}

fn run_human(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    // dispositions refuse under CLAUDECODE; fresh() already env_remove'd it, kept here for clarity
    run(dir, args)
}

#[test]
fn a_disposition_snapshots_the_reading_grid_at_that_instant() {
    let dir = fresh();
    std::fs::write(dir.join("a.txt"), "hello\n").unwrap();
    Command::new("git")
        .args(["add", "-A"])
        .current_dir(&dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["-c", "commit.gpgsign=false", "commit", "-m", "one"])
        .current_dir(&dir)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .output()
        .unwrap();
    assert!(run(&dir, &["claim", "c", "--by", "agent"]).status.success());
    let id = claim_id(&dir);
    assert!(run(
        &dir,
        &["reading", &id, "--depth", "plain", "--lang", "zh", "url:x"]
    )
    .status
    .success());

    assert!(run_human(
        &dir,
        &["hold", &id, "--reason", "thinking", "--i-am-the-human"]
    )
    .status
    .success());

    let ev = ledger_events(&dir)
        .into_iter()
        .rfind(|e| e["type"] == "hold")
        .unwrap();
    let snap = &ev["body"]["reading_snapshot"];
    assert!(
        snap.is_object(),
        "every disposition records the reading grid at that instant: {ev}"
    );
    assert_eq!(snap["grid"]["plain/zh"].as_str().unwrap(), "present");
    assert_eq!(snap["grid"]["plain/en"].as_str().unwrap(), "empty");
    assert_eq!(
        snap["viewed_depth"].as_str().unwrap(),
        "maintainer",
        "a CLI hold saw only the claim proper"
    );
    assert!(!snap["hit_empty"].as_bool().unwrap());
    // R3: at_verify still rides the same event, unchanged.
    assert!(
        ev["body"]["at_verify"].is_array(),
        "the pair's snapshot is untouched: {ev}"
    );
}
