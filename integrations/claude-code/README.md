# `ev` for Claude Code

The Claude Code–specific kit. The rest of `integrations/` is runtime-neutral (it works on any agent that
loads `AGENTS.md` and a session-start hook); this directory names Claude Code paths (`.claude/skills/`,
`claude -p`) directly.

Everything below writes **local setup on your machine** — `.claude/`, `.git/hooks/`, `.evolving/`. None of
it is committed to the repo you guard.

## One-step setup

`ev setup` is built into the binary (no checkout needed — it embeds the skill + hook templates). Run it in
the working tree you want guarded:

```sh
ev setup                 # set up the current working tree
ev setup /path/to/proj   # …or a given one
ev setup --dry-run       # preview every change, write nothing
```

It is idempotent and non-destructive (backs up a differing skill, never edits an existing
`settings.json`, never overwrites an existing pre-commit), and prints every change. It sets up:

1. **A co-located ledger** — `ev init` at the working-tree root + `staleness_ref=local-head`, so bound
   checks read the code's real git state and a *local* change can go red (the catch-loop fires as you
   work, not only after a push).
2. **The skill** — writes `.claude/skills/ev/SKILL.md`, where Claude Code discovers it.
3. **The hooks** — installs the session-start brief (so a fresh session heeds the settled decisions) and
   the pre-commit gate (`ev check --run --exit-on-red`). It writes `.claude/settings.json` if absent; if
   one already exists it does **not** edit your JSON — it prints the one line to add.

## Headless / unattended (`claude -p`)

Skills work the same in non-interactive mode, including with permissions pre-granted. The **permission
mode does not gate skills** — availability is independent of it (a skill is gated only by an explicit
`Skill`/`Skill(<name>)` rule). Recipe for a runner (CI, a Raspberry Pi, the dogfood):

```sh
ev setup /path/to/trial            # 1. set up the trial working tree (steps 1–3 above)
claude -v                          # 2. confirm this box's Claude Code does skills in -p (~v2.1.181+) — verify before relying on it
cd /path/to/trial                  # 3. run from the co-located tree, NAMING the skill to guarantee it loads
claude -p "use /ev to record/guard this decision: <round task>" --permission-mode=bypassPermissions
```

In `-p`, Claude auto-invokes a skill whose `description` matches the task; naming `/ev` in the prompt
guarantees it. Gotchas (verify on the box): `--bare` skips skill auto-discovery; `-p` does not
auto-install plugins; `--permission-mode=bypassPermissions` does not override a skill's own `allowed-tools`
or a `Skill(<name>)` deny rule.

## Manual setup (no `ev setup`)

```sh
sh integrations/scaffold/ev-colocate.sh /path/to/project           # 1. co-locate
mkdir -p /path/to/project/.claude/skills/ev
cp skills/ev/SKILL.md /path/to/project/.claude/skills/ev/SKILL.md  # 2. skill
cp integrations/agent-hooks/ev-brief-sessionstart.sh /path/to/project/.claude/hooks/   # 3. hooks
# add a SessionStart command to .claude/settings.json (see ../agent-hooks/settings.snippet.json);
cp integrations/agent-hooks/pre-commit /path/to/project/.git/hooks/pre-commit && chmod +x "$_"
```

## Honest notes

- `ev setup` raises enforcement; it does not make `ev` an enforcer. The session-start hook makes settled
  decisions *unmissable* (it does not compel obedience); the pre-commit hook gates commits only.
- Version + `--bare` behavior above are from the Claude Code docs — `claude -v` and a one-shot
  `claude -p "/ev"` smoke test on the actual box before trusting them in an unattended run.
