//! `ev check` rendering: the rich grammar on a colour TTY / `--color=always`, and byte-stable legacy
//! on a pipe / `--plain` (scriptability sacred — a `| grep` / CI must keep today's exact bytes).
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ev() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ev"))
}
fn repo() -> std::path::PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "ev-render-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    assert!(ev()
        .arg("init")
        .current_dir(&p)
        .output()
        .unwrap()
        .status
        .success());
    p
}
fn run(repo: &std::path::Path, args: &[&str]) -> std::process::Output {
    ev().args(args).current_dir(repo).output().unwrap()
}
// A test-bound decision declaring a platform with no receipt -> not-run -> a gating (attention) row.
fn decide_bound(repo: &std::path::Path) {
    let out = run(
        repo,
        &[
            "decide",
            "no-Redis posture",
            "--assume",
            "no Redis; multi-pod via existing DB",
            "--assume-test",
            "pytest x",
            "--counter-test",
            "pytest x::flips",
            "--on-platform",
            "linux-ci",
            "--triggered-by",
            "pyproject.toml",
            "--surface",
            "pyproject-deps",
            "--verified-at-sha",
            "d308afac1b2c3d4e5f60718293a4b5c6d7e8f901",
            "--blame",
            "Wang Yu",
        ],
    );
    assert!(
        out.status.success(),
        "decide: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn check_should_render_rich_glyphs_tally_and_breadcrumb_under_color_always() {
    // given: a gating (not-run) decision
    let r = repo();
    decide_bound(&r);

    // when: check is forced rich (as if a colour TTY, or piped to `less -R`)
    let out = run(&r, &["check", "--color", "always"]);
    let s = String::from_utf8_lossy(&out.stdout);

    // then: the attention verdict glyph leads the row, the plain-count tally shows, and the resurface
    // is signposted — and the legacy tab row is gone
    assert!(
        s.contains('◆'),
        "rich check shows the attention verdict glyph; was {s:?}"
    );
    assert!(
        s.contains("gating"),
        "rich check shows the gating/green/non-gating tally; was {s:?}"
    );
    assert!(
        s.contains("tip: ev why"),
        "rich check signposts the resurface; was {s:?}"
    );
    assert!(
        !s.contains("not-run\t"),
        "rich must not emit the legacy tab row; was {s:?}"
    );
}

#[test]
fn check_should_emit_legacy_tab_bytes_on_a_pipe_and_plain_should_win_over_color_always() {
    // given: the same gating decision
    let r = repo();
    decide_bound(&r);

    // when: check runs over a pipe (the test harness stdout is not a TTY) → legacy
    let piped = run(&r, &["check"]);
    let p = String::from_utf8_lossy(&piped.stdout);

    // then: today's exact tab-separated bytes, no glyphs
    assert!(
        p.contains("not-run\t"),
        "a pipe gets today's tab-separated bytes; was {p:?}"
    );
    assert!(!p.contains('◆'), "a pipe must carry no glyphs; was {p:?}");

    // and: --plain forces legacy even alongside --color=always (the explicit opt-out wins)
    let plain = run(&r, &["check", "--color", "always", "--plain"]);
    let pl = String::from_utf8_lossy(&plain.stdout);
    assert!(
        pl.contains("not-run\t") && !pl.contains('◆'),
        "--plain must win over --color=always; was {pl:?}"
    );
}

// A user-ruled decision that closed a road — surfaces in brief as a load-bearing ruling.
fn decide_user_ruled(repo: &std::path::Path) {
    let out = run(
        repo,
        &[
            "decide",
            "freeze the schema for v2",
            "--authority",
            "user-ruled",
            "--reject",
            "v3-now: too risky before the migration lands",
            "--blame",
            "You",
        ],
    );
    assert!(
        out.status.success(),
        "decide: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn brief_should_render_masthead_and_provenance_glyph_under_color_always() {
    // given: a user-ruled ruling
    let r = repo();
    decide_user_ruled(&r);

    // when: the boot-read is forced rich
    let out = run(&r, &["brief", "--color", "always"]);
    let s = String::from_utf8_lossy(&out.stdout);

    // then: the masthead counts the rulings, the human provenance glyph leads, and the legacy
    // `[user-ruled]` per-row tag is gone (the count moved to the masthead)
    assert!(
        s.contains("user-ruled decisions"),
        "rich brief shows the masthead count; was {s:?}"
    );
    assert!(
        s.contains('●'),
        "rich brief leads with the human provenance glyph; was {s:?}"
    );
    assert!(
        !s.contains("[user-ruled]"),
        "rich brief drops the legacy per-row tag; was {s:?}"
    );

    // and a pipe still gets today's exact bytes
    let piped = run(&r, &["brief"]);
    let p = String::from_utf8_lossy(&piped.stdout);
    assert!(
        p.contains("[user-ruled]"),
        "a pipe keeps today's [user-ruled] tag; was {p:?}"
    );
    assert!(
        !p.contains('●'),
        "a pipe carries no provenance glyph; was {p:?}"
    );
}

#[test]
fn list_should_lead_with_a_provenance_glyph_under_color_always_and_stay_tabbed_on_a_pipe() {
    // given: a decision in the ledger
    let r = repo();
    decide_user_ruled(&r);

    // when rich: the decision-led row leads with the human provenance glyph + shows the name
    let rich = run(&r, &["list", "--color", "always"]);
    let s = String::from_utf8_lossy(&rich.stdout);
    assert!(
        s.contains('●'),
        "rich list leads each decision with a provenance glyph; was {s:?}"
    );
    assert!(
        s.contains("freeze the schema for v2"),
        "rich list shows the decision name; was {s:?}"
    );

    // when piped: today's tab-separated bytes, no glyph
    let piped = run(&r, &["list"]);
    let p = String::from_utf8_lossy(&piped.stdout);
    assert!(
        p.contains('\t'),
        "a pipe gets tab-separated bytes; was {p:?}"
    );
    assert!(!p.contains('●'), "a pipe carries no glyph; was {p:?}");
}

#[test]
fn log_should_show_the_edge_verb_and_glyph_under_color_always_and_stay_tabbed_on_a_pipe() {
    // given: a decision in the lineage
    let r = repo();
    decide_user_ruled(&r);

    // when rich: the lineage row carries the edge-verb slot + the provenance glyph
    let rich = run(&r, &["log", "--color", "always"]);
    let s = String::from_utf8_lossy(&rich.stdout);
    assert!(
        s.contains('●'),
        "rich log leads with a provenance glyph; was {s:?}"
    );
    assert!(
        s.contains("decided"),
        "rich log shows the edge-verb slot (decided); was {s:?}"
    );

    // when piped: today's tab-separated lineage, no glyph
    let piped = run(&r, &["log"]);
    let p = String::from_utf8_lossy(&piped.stdout);
    assert!(
        p.contains('\t') && !p.contains('●'),
        "a pipe stays tab-separated with no glyph; was {p:?}"
    );
}

#[test]
fn reopen_should_render_a_provenance_headline_under_color_always_and_stay_plain_on_a_pipe() {
    // given: a recorded decision (capture its id)
    let r = repo();
    let out = run(
        &r,
        &[
            "decide",
            "freeze the schema for v2",
            "--authority",
            "user-ruled",
            "--reject",
            "v3-now: risky",
            "--blame",
            "You",
        ],
    );
    assert!(out.status.success());
    let id = String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_string();

    // when rich: the decision headline leads with the provenance glyph (shown ONCE, decision-level)
    let rich = run(&r, &["reopen", &id, "--color", "always"]);
    let s = String::from_utf8_lossy(&rich.stdout);
    assert!(
        s.contains('●'),
        "rich reopen leads the headline with a provenance glyph; was {s:?}"
    );
    assert!(
        s.contains("freeze the schema for v2"),
        "rich reopen shows the decision name; was {s:?}"
    );

    // when piped: today's exact `decision <id>: …` form, no glyph
    let piped = run(&r, &["reopen", &id]);
    let p = String::from_utf8_lossy(&piped.stdout);
    assert!(
        p.contains("decision ") && !p.contains('●'),
        "a pipe gets today's 'decision <id>:' form with no glyph; was {p:?}"
    );
}
