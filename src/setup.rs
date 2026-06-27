//! `ev setup` — self-contained one-step setup of the ev usage loop for Claude Code.
//!
//! Embeds the skill + hook templates at build time, so it works straight after `cargo install` with
//! no checkout: it co-locates the ledger, installs the skill where Claude Code discovers it
//! (`.claude/skills/ev/SKILL.md`), and wires the session-start brief + pre-commit gate.
//! Idempotent · `--dry-run` · non-destructive (backs up a differing skill, never edits an existing
//! settings.json, never overwrites an existing pre-commit). It raises enforcement; it does not make
//! `ev` an enforcer — the session-start hook makes settled decisions unmissable, the pre-commit hook
//! gates commits only.

use crate::store::Store;
use std::path::Path;
use std::process::ExitCode;

// Single source with the repo's skill + neutral hooks — embedded so the binary needs no checkout.
const SKILL: &str = include_str!("../skills/ev/SKILL.md");
const SESSION_HOOK: &str = include_str!("../integrations/agent-hooks/ev-brief-sessionstart.sh");
const PRE_COMMIT: &str = include_str!("../integrations/agent-hooks/pre-commit");

pub fn run(target: &Path, dry_run: bool) -> ExitCode {
    if !target.join(".git").exists() {
        eprintln!(
            "ev setup: '{}' is not a git working tree — co-location needs one.",
            target.display()
        );
        return ExitCode::FAILURE;
    }
    let target = match target.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("ev setup: {}: {e}", target.display());
            return ExitCode::FAILURE;
        }
    };
    println!("ev setup → {}", target.display());

    let steps = colocate(&target, dry_run)
        .and_then(|()| install_skill(&target, dry_run))
        .and_then(|()| wire_hooks(&target, dry_run));
    if let Err(e) = steps {
        eprintln!("ev setup: {e}");
        return ExitCode::FAILURE;
    }

    if dry_run {
        println!("(dry-run) — no changes made.");
    } else {
        println!(
            "done — a fresh Claude Code session here loads ev brief; a commit gates on a broken bound check."
        );
    }
    ExitCode::SUCCESS
}

// 1. The structure rung: a ledger at the working-tree root, tuned to catch local changes.
fn colocate(target: &Path, dry_run: bool) -> std::io::Result<()> {
    println!("1. ledger (co-located)");
    let store = Store::at(target);
    if store.exists() {
        println!("   .evolving/ present — kept");
        return Ok(());
    }
    if dry_run {
        println!("   [dry-run] ev init + staleness_ref=local-head");
        return Ok(());
    }
    store.init()?;
    // Track the WORKING HEAD, not @{upstream}: a local, not-yet-pushed change can go red, so the
    // catch-loop fires as you work rather than only after a push.
    let cfg = store.config_path();
    let text = std::fs::read_to_string(&cfg)?;
    std::fs::write(
        &cfg,
        text.replace(
            "staleness_ref = \"live-origin\"",
            "staleness_ref = \"local-head\"",
        ),
    )?;
    println!("   ✓ ev init + staleness_ref=local-head");
    Ok(())
}

// 2. Put the skill where Claude Code discovers it.
fn install_skill(target: &Path, dry_run: bool) -> std::io::Result<()> {
    println!("2. skill → .claude/skills/ev/SKILL.md");
    let dst = target.join(".claude/skills/ev/SKILL.md");
    let differs = dst.exists()
        && std::fs::read_to_string(&dst)
            .map(|c| c != SKILL)
            .unwrap_or(true);
    if differs {
        backup(&dst, dry_run)?;
    }
    write_file(&dst, SKILL, false, dry_run)?;
    if !dry_run {
        println!("   ✓ installed");
    }
    Ok(())
}

// 3. The deterministic rungs: heed (session-start brief) + gate (pre-commit).
fn wire_hooks(target: &Path, dry_run: bool) -> std::io::Result<()> {
    println!("3. hooks");
    let hook = target.join(".claude/hooks/ev-brief-sessionstart.sh");
    write_file(&hook, SESSION_HOOK, true, dry_run)?;

    let settings = target.join(".claude/settings.json");
    // The command quotes the path so the shell running the hook doesn't word-split a path with spaces.
    let command = format!("sh \"{}\"", hook.display());
    if settings.exists() {
        println!("   .claude/settings.json exists — not auto-editing JSON. Add this SessionStart command:");
        println!("     {command}");
    } else if dry_run {
        println!(
            "   [dry-run] write {} with a SessionStart hook",
            settings.display()
        );
    } else {
        let json = serde_json::json!({
            "hooks": { "SessionStart": [ { "hooks": [ { "type": "command", "command": command } ] } ] }
        });
        std::fs::write(
            &settings,
            serde_json::to_string_pretty(&json).unwrap() + "\n",
        )?;
        println!("   ✓ session-start brief wired (.claude/settings.json)");
    }

    let pre_commit = target.join(".git/hooks/pre-commit");
    if pre_commit.exists() {
        println!("   .git/hooks/pre-commit exists — not overwriting");
    } else {
        write_file(&pre_commit, PRE_COMMIT, true, dry_run)?;
        if !dry_run {
            println!("   ✓ pre-commit gate installed");
        }
    }
    Ok(())
}

fn write_file(path: &Path, content: &str, executable: bool, dry_run: bool) -> std::io::Result<()> {
    if dry_run {
        println!("   [dry-run] write {}", path.display());
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)?;
    if executable {
        set_executable(path)?;
    }
    Ok(())
}

fn backup(path: &Path, dry_run: bool) -> std::io::Result<()> {
    let bak = std::path::PathBuf::from(format!("{}.bak", path.display()));
    if dry_run {
        println!("   [dry-run] back up existing → {}", bak.display());
        return Ok(());
    }
    std::fs::copy(path, &bak)?;
    println!("   (existing differed → backed up to {})", bak.display());
    Ok(())
}

#[cfg(unix)]
fn set_executable(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(perms.mode() | 0o755);
    std::fs::set_permissions(path, perms)
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> std::io::Result<()> {
    Ok(())
}
