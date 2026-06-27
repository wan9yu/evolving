# `ev` for Claude Code

The Claude Code–specific kit. The rest of `integrations/` is runtime-neutral (it works on any agent that
loads `AGENTS.md` and a session-start hook); this directory is the vendor-specific glue that wires those
neutral pieces into Claude Code, and names Claude Code paths (`.claude/skills/`, `claude -p`) directly.

Everything here writes **local setup on your machine** — `.claude/`, `.git/hooks/`, `.evolving/`. None of
it is committed to the repo you guard.

## One-step setup

From an `ev` checkout, point the installer at the working tree you want guarded:

```sh
integrations/claude-code/ev-setup.sh /path/to/your/project
# preview without writing anything:
integrations/claude-code/ev-setup.sh --dry-run /path/to/your/project
```

It is idempotent and non-destructive (backs up or refuses to clobber), and prints every change. It sets up:

1. **A co-located ledger** — `ev init` at the working-tree root + `staleness_ref=local-head`, so bound
   checks read the code's real git state (reuses the neutral `../scaffold/ev-colocate.sh`).
2. **The skill** — copies `skills/ev/SKILL.md` to `.claude/skills/ev/SKILL.md`, where Claude Code
   discovers it.
3. **The hooks** — installs the session-start brief (so a fresh session heeds the settled decisions) and
   the pre-commit gate (`ev check --run --exit-on-red`). It writes `.claude/settings.json` if absent; if
   one already exists it does **not** edit your JSON — it prints the one line to add.

## Headless / unattended (`claude -p`)

Skills work the same in non-interactive mode, including with permissions pre-granted. The **permission
mode does not gate skills** — availability is independent of it (a skill is gated only by an explicit
`Skill`/`Skill(<name>)` rule). Recipe for a runner (CI, a Raspberry Pi, the dogfood):

```sh
# 1. set up the trial working tree (the installer does steps 1–3 above)
integrations/claude-code/ev-setup.sh /path/to/trial

# 2. confirm this box's Claude Code supports skills in -p (verify the version on THIS box)
claude -v          # user-invoked skills in -p need a recent build (~v2.1.181+) — confirm before relying on it

# 3. each round, run from the co-located working tree, NAMING the skill to guarantee it loads
cd /path/to/trial
claude -p "use /ev to record/guard this decision: <round task>" --permission-mode=bypassPermissions
```

In `-p`, Claude auto-invokes a skill whose `description` matches the task; naming `/ev` in the prompt
guarantees it. Gotchas (verify on the box): `--bare` skips skill auto-discovery; `-p` does not
auto-install plugins; `--permission-mode=bypassPermissions` does not override a skill's own `allowed-tools`
or a `Skill(<name>)` deny rule.

## Manual setup (no installer)

```sh
sh integrations/scaffold/ev-colocate.sh /path/to/project           # 1. co-locate
mkdir -p /path/to/project/.claude/skills/ev
cp skills/ev/SKILL.md /path/to/project/.claude/skills/ev/SKILL.md  # 2. skill
cp integrations/agent-hooks/ev-brief-sessionstart.sh /path/to/project/.claude/hooks/   # 3. hooks
# add a SessionStart command to .claude/settings.json (see ../agent-hooks/settings.snippet.json);
cp integrations/agent-hooks/pre-commit /path/to/project/.git/hooks/pre-commit && chmod +x "$_"
```

## Honest notes

- The installer assumes you have an `ev` **checkout** (it copies the skill + hooks from this repo). A
  `cargo install`-only user without the repo can clone it or download a release tarball; a self-contained
  `ev setup` subcommand that embeds these files is a possible future step.
- It raises enforcement; it does not make `ev` an enforcer. The session-start hook makes settled decisions
  *unmissable* (it does not compel obedience); the pre-commit hook gates commits only.
- Version + `--bare` behavior above are from the Claude Code docs — `claude -v` and a one-shot
  `claude -p "/ev"` smoke test on the actual box before trusting them in an unattended run.
