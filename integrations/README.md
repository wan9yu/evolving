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

## Honest boundary

These raise enforcement; they do not turn `ev` into an enforcer. `ev` stays detect-not-prevent: the
SessionStart hook makes settled decisions *unmissable* (heed rate up), it does not compel obedience; the
git hook gates commits only; the scaffold removes a setup judgment, it does not police the filesystem. A
genuinely advisory decision (a judgment call no check can watch) will still rely on heed — that is honest,
not a gap. Push each decision to the highest rung it can sit on; accept that the top of the ladder is
where a check can run.
