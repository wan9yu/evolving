use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ev() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ev"))
}
fn repo() -> std::path::PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "ev-brief-{}-{}",
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
/// Record a decision, passing through any extra flags (e.g. --authority, --reject).
fn decide(repo: &std::path::Path, text: &str, extra: &[&str]) -> String {
    let mut args: Vec<&str> = vec!["decide", text, "--assume", "a reason", "--blame", "Wang Yu"];
    args.extend_from_slice(extra);
    let out = ev().args(&args).current_dir(repo).output().unwrap();
    assert!(
        out.status.success(),
        "decide: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_string()
}

#[test]
fn brief_should_show_a_user_ruled_decision_and_its_rejected_road_when_one_is_recorded() {
    // given: a user-ruled decision that records a road it rejected
    let r = repo();
    decide(
        &r,
        "keep the slice locked",
        &[
            "--authority",
            "user-ruled",
            "--reject",
            "v1.9: re-milestoned without sign-off",
        ],
    );

    // when: ev brief runs
    let out = ev().arg("brief").current_dir(&r).output().unwrap();

    // then: it exits 0 and names the decision, its authority, and the rejected road
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("keep the slice locked"));
    assert!(stdout.contains("[user-ruled]"));
    assert!(stdout.contains("rejected v1.9"));
}

#[test]
fn brief_should_omit_a_decision_that_is_not_user_ruled_when_briefing() {
    // given: a plain decision with no declared authority
    let r = repo();
    decide(&r, "a disposable decision", &[]);

    // when: ev brief runs
    let out = ev().arg("brief").current_dir(&r).output().unwrap();

    // then: it exits 0 and shows none (the decision is not user-ruled)
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.contains("a disposable decision"));
    assert!(stdout.contains("no user-ruled decisions"));
}

#[test]
fn brief_should_accept_a_limit_flag_when_one_is_passed() {
    // given: a user-ruled decision in the store
    let r = repo();
    decide(&r, "keep the slice locked", &["--authority", "user-ruled"]);

    // when: ev brief runs with --limit
    let out = ev()
        .args(["brief", "--limit", "1"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: the flag is accepted (exits 0) and the decision still shows
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("keep the slice locked"));
}

#[test]
fn brief_should_order_user_ruled_decisions_most_recent_first_when_multiple_exist() {
    // given: two user-ruled decisions recorded in order — A first, then B (B is later)
    let r = repo();
    decide(&r, "decision A", &["--authority", "user-ruled"]);
    decide(&r, "decision B", &["--authority", "user-ruled"]);

    // when: ev brief runs
    let out = ev().arg("brief").current_dir(&r).output().unwrap();

    // then: B's text appears before A's (most-recent-first by held_since)
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let b = stdout.find("decision B").expect("B shown");
    let a = stdout.find("decision A").expect("A shown");
    assert!(b < a, "B should appear before A:\n{stdout}");
}

#[test]
fn brief_should_cap_and_note_the_remainder_when_over_the_limit() {
    // given: three user-ruled decisions
    let r = repo();
    decide(&r, "decision one", &["--authority", "user-ruled"]);
    decide(&r, "decision two", &["--authority", "user-ruled"]);
    decide(&r, "decision three", &["--authority", "user-ruled"]);

    // when: ev brief runs with --limit 2
    let out = ev()
        .args(["brief", "--limit", "2"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: exactly two decisions show plus a remainder footer pointing at `ev list`
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let shown = ["decision one", "decision two", "decision three"]
        .iter()
        .filter(|d| stdout.contains(**d))
        .count();
    assert_eq!(shown, 2, "exactly two decisions shown:\n{stdout}");
    assert!(stdout.contains("1 more"), "remainder count:\n{stdout}");
    assert!(stdout.contains("ev list"), "points at ev list:\n{stdout}");
}

#[test]
fn brief_should_show_all_when_limit_is_zero() {
    // given: three user-ruled decisions
    let r = repo();
    decide(&r, "decision one", &["--authority", "user-ruled"]);
    decide(&r, "decision two", &["--authority", "user-ruled"]);
    decide(&r, "decision three", &["--authority", "user-ruled"]);

    // when: ev brief runs with --limit 0
    let out = ev()
        .args(["brief", "--limit", "0"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: all three show and there is no remainder footer
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let shown = ["decision one", "decision two", "decision three"]
        .iter()
        .filter(|d| stdout.contains(**d))
        .count();
    assert_eq!(shown, 3, "all three shown:\n{stdout}");
    assert!(!stdout.contains("more user-ruled"), "no footer:\n{stdout}");
}

#[test]
fn brief_should_fail_when_there_is_no_store() {
    // given: a bare directory with no .evolving store
    let p = std::env::temp_dir().join(format!("ev-brief-nostore-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();

    // when: ev brief runs
    let out = ev().arg("brief").current_dir(&p).output().unwrap();

    // then: it errors and exits non-zero
    assert!(!out.status.success());
}

#[test]
fn brief_should_pin_a_rejected_road_ruling_above_the_cap_over_newer_plain_rulings() {
    // given: an OLD user-ruled decision that closed a road, then several NEWER plain user-ruled ones
    let r = repo();
    decide(
        &r,
        "the closed road",
        &["--authority", "user-ruled", "--reject", "x: closed"],
    );
    decide(&r, "newer plain one", &["--authority", "user-ruled"]);
    decide(&r, "newer plain two", &["--authority", "user-ruled"]);
    decide(&r, "newer plain three", &["--authority", "user-ruled"]);

    // when: ev brief runs with a cap of 2
    let out = ev()
        .args(["brief", "--limit", "2"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: the OLD rejected-road decision is pinned and shown, not buried by the newer plain ones
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("the closed road"),
        "load-bearing ruling must be pinned above the cap:\n{stdout}"
    );
}

#[test]
fn brief_should_count_hidden_load_bearing_rulings_in_the_footer_when_they_exceed_the_cap() {
    // given: three user-ruled decisions, EACH closing a road
    let r = repo();
    decide(
        &r,
        "closed road one",
        &["--authority", "user-ruled", "--reject", "a: one"],
    );
    decide(
        &r,
        "closed road two",
        &["--authority", "user-ruled", "--reject", "b: two"],
    );
    decide(
        &r,
        "closed road three",
        &["--authority", "user-ruled", "--reject", "c: three"],
    );

    // when: ev brief runs with a cap of 1
    let out = ev()
        .args(["brief", "--limit", "1"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: the footer makes the elided load-bearing rulings visible by counting them
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("2 with rejected roads"),
        "footer must count hidden load-bearing rulings:\n{stdout}"
    );
}

#[test]
fn brief_should_emit_a_structured_json_object_when_json_is_passed() {
    // given: a user-ruled decision that closed a road, carrying an opaque producer source_ref
    let r = repo();
    decide(
        &r,
        "keep the slice locked",
        &[
            "--authority",
            "user-ruled",
            "--source-ref",
            "R1",
            "--reject",
            "v1.9: re-milestoned without sign-off",
        ],
    );

    // when: ev brief --json runs
    let out = ev()
        .args(["brief", "--json"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: stdout is ONE json object carrying the ruling — its id (citable), source_ref, the
    // load-bearing flag, and the rejected road parsed into option + claim — plus the elision counts
    assert!(out.status.success());
    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("brief --json must emit valid JSON");
    assert_eq!(v["kind"], "ev-brief");
    let d = &v["decisions"][0];
    assert!(d["id"].is_string(), "each decision carries a citable id");
    assert_eq!(d["decision"], "keep the slice locked");
    assert_eq!(d["source_ref"], "R1");
    assert_eq!(d["load_bearing"], true);
    assert_eq!(d["rejected_roads"][0]["option"], "v1.9");
    assert_eq!(
        d["rejected_roads"][0]["claim"],
        "re-milestoned without sign-off"
    );
    assert_eq!(v["shown"], 1);
    assert_eq!(v["total"], 1);
    assert_eq!(v["elided"], 0);
    assert_eq!(v["elided_load_bearing"], 0);
}

#[test]
fn brief_json_should_make_elision_visible_with_counts_when_over_the_limit() {
    // given: three user-ruled decisions, each closing a road
    let r = repo();
    decide(
        &r,
        "closed road one",
        &["--authority", "user-ruled", "--reject", "a: one"],
    );
    decide(
        &r,
        "closed road two",
        &["--authority", "user-ruled", "--reject", "b: two"],
    );
    decide(
        &r,
        "closed road three",
        &["--authority", "user-ruled", "--reject", "c: three"],
    );

    // when: ev brief --json runs with a cap of 1
    let out = ev()
        .args(["brief", "--json", "--limit", "1"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: exactly one decision is shown, and the counts make the elided two (both load-bearing)
    // visible so a consumer can re-pull with a higher limit rather than silently miss them
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["decisions"].as_array().unwrap().len(), 1);
    assert_eq!(v["shown"], 1);
    assert_eq!(v["total"], 3);
    assert_eq!(v["elided"], 2);
    assert_eq!(v["elided_load_bearing"], 2);
}

#[test]
fn brief_json_should_emit_an_empty_object_when_there_are_no_user_ruled_decisions() {
    // given: a store with only a plain (non-user-ruled) decision
    let r = repo();
    decide(&r, "a disposable decision", &[]);

    // when: ev brief --json runs
    let out = ev()
        .args(["brief", "--json"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: a consumer parsing --json always gets valid JSON — never the human "no user-ruled" text
    assert!(out.status.success());
    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("empty brief --json is still valid JSON");
    assert_eq!(v["kind"], "ev-brief");
    assert_eq!(v["decisions"].as_array().unwrap().len(), 0);
    assert_eq!(v["total"], 0);
}

#[test]
fn brief_should_not_mention_rejected_roads_in_the_footer_when_none_are_hidden() {
    // given: one rejected-road ruling plus three plain user-ruled decisions
    let r = repo();
    decide(
        &r,
        "the closed road",
        &["--authority", "user-ruled", "--reject", "x: closed"],
    );
    decide(&r, "plain one", &["--authority", "user-ruled"]);
    decide(&r, "plain two", &["--authority", "user-ruled"]);
    decide(&r, "plain three", &["--authority", "user-ruled"]);

    // when: ev brief runs with a cap of 2 (the rejected-road ruling is pinned + shown)
    let out = ev()
        .args(["brief", "--limit", "2"])
        .current_dir(&r)
        .output()
        .unwrap();

    // then: the footer counts the two hidden plain rulings and does NOT mention rejected roads
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("… 2 more user-ruled decision(s) — `ev list` for all"),
        "footer must read exactly without rejected-roads clause:\n{stdout}"
    );
    assert!(
        !stdout.contains("with rejected roads"),
        "no hidden load-bearing ruling, so no rejected-roads clause:\n{stdout}"
    );
}
