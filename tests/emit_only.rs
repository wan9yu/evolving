//! Proof that the instrumentation is EMIT-ONLY: `reading_snapshot` and `reading_census` are
//! written by `src/` and read NOWHERE in it — the moment ev branches on either, the measurement
//! becomes a judgment. The same proof 0.2.3 owed for `at_verify`.

fn src_files() -> Vec<std::path::PathBuf> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "rs"))
        .collect()
}

#[test]
fn the_reading_instruments_are_written_but_never_read() {
    // A READ of a JSON body key is an accessor: `.get("<key>")`, `["<key>"]`, or a fold match
    // arm `"<key>" =>`. The keys and event types below must appear in NO such form anywhere in
    // src. (They may be WRITTEN — as a json! key or an etype string literal — freely.)
    let forbidden = [
        // reading_snapshot: a body key on disposition events
        r#".get("reading_snapshot")"#,
        r#"["reading_snapshot"]"#,
        r#""reading_snapshot" =>"#,
        // reading_census: an event type
        r#".get("reading_census")"#,
        r#"["reading_census"]"#,
        r#""reading_census" =>"#,
    ];
    for path in src_files() {
        let text = std::fs::read_to_string(&path).unwrap();
        for needle in forbidden {
            assert!(
                !text.contains(needle),
                "{} reads an emit-only instrument ({needle}) — it must only be written",
                path.display()
            );
        }
    }
}
