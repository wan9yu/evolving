# `ev` integrations — runnable practices, not a manual

`ev` itself is the honest map: it records decisions, binds falsifiable checks, and is honest about
which decisions are *watched* (a check can flip them red) versus *advisory* (nothing watches them — they
rely on heed, so they drift). Acting on that map should not be prose you have to remember. **Each piece
here is a runnable artifact** — a hook, a skill, a scaffold — that places a decision on the highest
enforcement rung its nature allows. A rule you must remember is a sign; these are the speed bumps.

| Rung | The drift it removes | Runnable artifact |
|---|---|---|
| **structure** (no judgment to drift on) | the right setup is the *only* setup | [`scaffold/ev-colocate.sh`](scaffold/ev-colocate.sh) — births the ledger co-located with the guarded working tree, so `ev check` always sees the code's real git state |
| **heed** (auto, not remembered) | a fresh agent acting before loading the settled decisions | [`agent-hooks/ev-brief-sessionstart.sh`](agent-hooks/ev-brief-sessionstart.sh) — a session-start hook that injects `ev brief` automatically |
| **gate** (deterministic) | a commit that breaks a bound decision | [`agent-hooks/pre-commit`](agent-hooks/pre-commit) — a git hook running `ev check --exit-on-red` |
| **propose** (the one judgment verb) | an agent forging a human ruling | the `ev:propose` skill — see [`../skills/ev/SKILL.md`](../skills/ev/SKILL.md) (agents propose; a human ratifies) |

## Why a scaffold and not a "remember to co-locate" note

`ev` derives all git state — staleness, receipts, the triggered-by diff — from the directory it runs in,
the same root as the ledger. So a ledger kept in a *separate* repo from the guarded code can never reflect
the code's real working state, and bound checks stall (stale / not-run) instead of going red on a real
change. A note saying "remember to co-locate" is just another sign — it drifts. The scaffold makes the
wrong layout unrepresentable: the ledger is born at the root of the guarded working tree. (`ev check` also
emits a co-location hint at runtime if a `--run` resolves no pass/fail — the cheap detector behind the
structural fix.)

## Using the skill in a headless / unattended agent

An agent invoked NON-INTERACTIVELY (a runner that runs it with permissions pre-granted — CI, an
unattended box) can use [`../skills/ev/SKILL.md`](../skills/ev/SKILL.md) the same as an interactive one.
Three deployment notes, stated as runtime conventions so this repo names no vendor:

- **Placement.** A runtime auto-discovers a skill from a per-project skills directory in the working
  directory (then a per-user one). Place `skills/ev/SKILL.md` there **on the deploy box** — that path is
  vendor-specific local config, copied/generated on the box, **never committed here** (so this repo stays
  vendor-neutral). The co-location scaffold is the natural place to also drop it.
- **Invocation.** Non-interactive mode auto-invokes a skill when its `description` matches the task; to
  GUARANTEE it loads, name the skill in the round prompt. **A permission-bypass flag does NOT gate skills**
  — skill availability is independent of the permission mode (a skill is gated only by an explicit
  skill-permission rule, not by bypass).
- **Version.** Headless skill invocation needs a recent runtime build — confirm the deploy box's version
  supports skills in non-interactive mode before relying on auto-invocation.

(The exact, vendor-specific commands for our own deploy boxes are kept **outside this repo** — they name a
vendor, and this repo stays vendor-neutral.)

## Honest boundary

These raise enforcement; they do not turn `ev` into an enforcer. `ev` stays detect-not-prevent: the
session-start hook makes settled decisions *unmissable* (heed rate up), it does not compel obedience; the
git hook gates commits only; the scaffold removes a setup judgment, it does not police the filesystem. A
genuinely advisory decision (a judgment call no check can watch) will still rely on heed — that is honest,
not a gap. Push each decision to the highest rung it can sit on; accept that the top of the ladder is
where a check can run.
