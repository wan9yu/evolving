use std::process::Command;

fn run(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ev"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

fn fresh() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ev-doc-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    assert!(run(&dir, &["init"]).status.success());
    dir
}

fn fresh_git_doctor() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ev-doc-g-{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(&dir)
        .output()
        .unwrap();
    dir
}

fn git_commit_doctor(dir: &std::path::Path, msg: &str) {
    let git = |a: &[&str]| {
        std::process::Command::new("git")
            .args(a)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .output()
            .unwrap()
    };
    git(&["add", "-A"]);
    git(&["-c", "commit.gpgsign=false", "commit", "-m", msg]);
}

fn ledger_path_doctor(dir: &std::path::Path) -> std::path::PathBuf {
    std::fs::read_dir(dir.join(".evolving/ledger"))
        .unwrap()
        .filter_map(|e| e.ok())
        .find(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
        .unwrap()
        .path()
}

fn claim_id_doctor(dir: &std::path::Path, label: &str) -> String {
    let p = ledger_path_doctor(dir);
    for line in std::fs::read_to_string(p).unwrap().lines() {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        if v["type"] == "claim" && v["body"]["label"] == label {
            return v["id"].as_str().unwrap().to_string();
        }
    }
    panic!("no claim labelled {label}");
}

/// Drop the baseline marker from the ledger file: the shape of a ledger written
/// by ev 0.2.1, which `ev init` (0.2.2) would have baselined.
fn strip_baseline_doctor(dir: &std::path::Path) {
    let p = ledger_path_doctor(dir);
    let kept: Vec<String> = std::fs::read_to_string(&p)
        .unwrap()
        .lines()
        .filter(|line| {
            let v: serde_json::Value = serde_json::from_str(line).unwrap();
            !(v["type"] == "session" && v["body"]["marker"] == "baseline")
        })
        .map(|l| l.to_string())
        .collect();
    std::fs::write(&p, format!("{}\n", kept.join("\n"))).unwrap();
}

#[test]
fn doctor_on_a_clean_ledger_reports_ok_and_exits_zero() {
    let dir = fresh();
    assert!(run(&dir, &["claim", "x"]).status.success());
    let out = run(&dir, &["doctor"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stdout)
        .to_lowercase()
        .contains("clean"));
}

#[test]
fn doctor_flags_a_dangling_evidence_ref() {
    let dir = fresh();
    // hand-write an evidence event pointing at a non-existent claim
    let wid = std::fs::read_to_string(dir.join(".evolving/local/writer.toml")).unwrap();
    let wid = wid.split('"').nth(1).unwrap().to_string();
    let path = dir.join(".evolving/ledger").join(format!("{wid}.jsonl"));
    let line = serde_json::json!({
        "v":2,"id":"evd_x","ts":"2020-01-01T00:00:00Z","writer":wid,"seq":99,
        "actor":{"kind":"human"},"type":"evidence","body":{"claim":"clm_missing","ref":"commit:x","status":"recorded"}
    });
    std::fs::write(&path, format!("{line}\n")).unwrap();
    let out = run(&dir, &["doctor"]);
    assert_ne!(
        out.status.code(),
        Some(0),
        "dangling ref should be non-zero"
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("dangling"));
}

#[test]
fn doctor_should_report_the_liveness_census_of_every_anchor() {
    let dir = fresh_git_doctor();
    std::fs::write(dir.join("a.txt"), "hello\n").unwrap();
    git_commit_doctor(&dir, "one");
    assert!(run(&dir, &["init"]).status.success());

    assert!(run(&dir, &["claim", "live", "--by", "agent"])
        .status
        .success());
    let live = claim_id_doctor(&dir, "live");
    assert!(run(&dir, &["evidence", &live, "file:a.txt::hello"])
        .status
        .success());

    assert!(run(&dir, &["claim", "dead", "--by", "agent"])
        .status
        .success());
    let dead = claim_id_doctor(&dir, "dead");
    assert!(run(&dir, &["evidence", &dead, "file:a.txt"])
        .status
        .success());

    let out = run(&dir, &["doctor"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "the census never changes the exit code"
    );
    let sout = String::from_utf8_lossy(&out.stdout);
    assert!(
        sout.contains("anchor liveness"),
        "expected a census: {sout}"
    );
    assert!(sout.contains("content 1"), "one content anchor: {sout}");
    assert!(sout.contains("existence 1"), "one existence anchor: {sout}");
    assert!(
        sout.contains("cannot fail when the cited code changes"),
        "the census must state the fact plainly: {sout}"
    );
}

/// The ref-TYPE census is a different question from the liveness census: liveness
/// buckets `artifact:x::t` with `file:x::t` (both `content`) and `metric:` with
/// `url:` (both `asserted`). 0.2.3 decides whether `artifact:`/`url:`/`metric:`
/// have earned their existence, and that question is answerable only from counts
/// per scheme.
#[test]
fn doctor_should_report_the_ref_type_census() {
    let dir = fresh_git_doctor();
    std::fs::write(dir.join("a.txt"), "hello\n").unwrap();
    git_commit_doctor(&dir, "one");
    assert!(run(&dir, &["init"]).status.success());

    assert!(run(&dir, &["claim", "c", "--by", "agent"]).status.success());
    let c = claim_id_doctor(&dir, "c");
    assert!(run(&dir, &["evidence", &c, "file:a.txt::hello"])
        .status
        .success());
    assert!(run(&dir, &["evidence", &c, "metric:coverage=0.91"])
        .status
        .success());

    let out = run(&dir, &["doctor"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "the census never changes the exit code"
    );
    let sout = String::from_utf8_lossy(&out.stdout);
    assert!(
        sout.contains("ref types in use"),
        "expected a ref-type census: {sout}"
    );
    assert!(sout.contains("file 1"), "one file: ref: {sout}");
    assert!(sout.contains("metric 1"), "one metric: ref: {sout}");
    assert!(
        sout.contains("url 0"),
        "url: is unused, and says so: {sout}"
    );
    assert!(
        sout.contains("artifact 0"),
        "artifact: is unused, and says so: {sout}"
    );
}

/// A ref no current grammar accepts is counted, not dropped. The fold degrades it
/// to `unparseable`; a census that silently skipped it would undercount — the exact
/// failure this command exists to surface.
#[test]
fn doctor_census_counts_an_unparseable_ref_rather_than_dropping_it() {
    let dir = fresh_git_doctor();
    assert!(run(&dir, &["init"]).status.success());
    assert!(run(&dir, &["claim", "legacy", "--by", "agent"])
        .status
        .success());
    let id = claim_id_doctor(&dir, "legacy");

    // the shape a ledger can carry but no scheme accepts, hand-written as an older
    // ev would have left it. The attach guard refuses it now; the read path may not.
    let p = ledger_path_doctor(&dir);
    let mut text = std::fs::read_to_string(&p).unwrap();
    let last: serde_json::Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
    let legacy = serde_json::json!({
        "v": last["v"], "id": "evd_01UNPARSEABLE00000000000",
        "ts": last["ts"], "writer": last["writer"],
        "seq": last["seq"].as_u64().unwrap() + 1,
        "actor": { "kind": "agent", "id": "legacy" },
        "type": "evidence",
        "body": { "claim": id, "ref": "no-scheme-at-all", "status": "unreachable", "self_evident": false }
    });
    text.push_str(&serde_json::to_string(&legacy).unwrap());
    text.push('\n');
    std::fs::write(&p, text).unwrap();

    let out = run(&dir, &["doctor"]);
    assert_eq!(out.status.code(), Some(0));
    let sout = String::from_utf8_lossy(&out.stdout);
    assert!(
        sout.contains("unparseable 1"),
        "an unparseable ref must be counted out loud: {sout}"
    );
}

#[test]
fn doctor_should_say_that_asserted_anchors_cannot_fail() {
    let dir = fresh_git_doctor();
    assert!(run(&dir, &["init"]).status.success());
    assert!(run(&dir, &["claim", "m", "--by", "agent"]).status.success());
    let m = claim_id_doctor(&dir, "m");
    assert!(run(&dir, &["evidence", &m, "metric:coverage=0.91"])
        .status
        .success());

    let out = run(&dir, &["doctor"]);
    let sout = String::from_utf8_lossy(&out.stdout);
    assert!(sout.contains("asserted 1"));
    assert!(
        sout.contains("cannot fail by construction"),
        "asserted anchors must be named as unable to go red: {sout}"
    );
}

/// The census covers what its label claims: closed and grey claims live in
/// separate buckets of the fold, and their anchors are counted too. A census
/// that quietly skipped them would be the very undercount ev exists to expose.
#[test]
fn doctor_census_counts_the_anchors_of_closed_and_grey_claims() {
    let dir = fresh_git_doctor();
    std::fs::write(dir.join("a.txt"), "hello\n").unwrap();
    git_commit_doctor(&dir, "one");
    assert!(run(&dir, &["init"]).status.success());

    assert!(run(&dir, &["claim", "shut", "--by", "agent"])
        .status
        .success());
    let shut = claim_id_doctor(&dir, "shut");
    assert!(run(&dir, &["evidence", &shut, "file:a.txt::hello"])
        .status
        .success());
    assert!(run(&dir, &["close", &shut, "--i-am-the-human"])
        .status
        .success());

    assert!(run(&dir, &["claim", "held", "--by", "agent"])
        .status
        .success());
    let held = claim_id_doctor(&dir, "held");
    assert!(run(&dir, &["evidence", &held, "file:a.txt"])
        .status
        .success());
    assert!(run(
        &dir,
        &["hold", &held, "--reason", "waiting", "--i-am-the-human"]
    )
    .status
    .success());

    let out = run(&dir, &["doctor"]);
    assert_eq!(out.status.code(), Some(0));
    let sout = String::from_utf8_lossy(&out.stdout);
    assert!(
        sout.contains("content 1"),
        "the closed claim's content anchor must be counted: {sout}"
    );
    assert!(
        sout.contains("existence 1"),
        "the grey claim's existence anchor must be counted: {sout}"
    );
    assert!(
        sout.contains("open and closed"),
        "the census line must state the scope it covers: {sout}"
    );
}

/// A ledger written by ev 0.2.1 carries no baseline marker, so `hooks::sweep`
/// refuses to file an exhaust window — silently, because a hook may not fail the
/// host session. Doctor states the fact and the remedy, and still exits 0: an
/// unbaselined ledger is not corrupt, it is not yet configured.
#[test]
fn doctor_reports_a_ledger_with_no_baseline_marker_without_failing() {
    let dir = fresh_git_doctor();
    assert!(run(&dir, &["init"]).status.success());
    strip_baseline_doctor(&dir);

    let out = run(&dir, &["doctor"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "a missing baseline is a configuration fact, not a corruption"
    );
    let sout = String::from_utf8_lossy(&out.stdout);
    assert!(
        sout.contains("no baseline marker"),
        "doctor must state the missing baseline: {sout}"
    );
    assert!(
        sout.contains("the session-end sweep will not file a window"),
        "doctor must state the consequence it has actually checked — `ev exhaust --since \
         <sha>` carries its own start and files without a baseline: {sout}"
    );
    assert!(
        sout.contains("ev baseline"),
        "doctor must state the remedy: {sout}"
    );
}

#[test]
fn doctor_stays_quiet_about_the_baseline_on_a_freshly_initialized_ledger() {
    let dir = fresh_git_doctor();
    assert!(run(&dir, &["init"]).status.success());
    let out = run(&dir, &["doctor"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(!String::from_utf8_lossy(&out.stdout).contains("no baseline marker"));
}
