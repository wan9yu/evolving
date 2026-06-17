---
name: recording-decisions-with-ev
description: Use when a technical decision is being made, a premise needs an invariant guarding it, or a guarding test has gone red — record and resurface it with `ev` (git for decisions), an immutable content-addressed decision ledger, instead of letting the reasoning scroll away in chat or a docstring.
---

# Recording & resurfacing decisions with `ev`

`ev` is **git for decisions**: it records a human-vetted decision and the grounds it
rests on as an immutable, content-addressed chain, binds a falsifiable test (or a human
re-check) to each ground, and **resurfaces the whole decision — named — when a bound check
goes red**. It deals in **facts, not verdicts**: no scores, no ranks, no auto-judgements.

Install: `cargo install evolving` (the command is `ev`). The store lives in `.evolving/`.

## When to use this

- A technical decision is being made (a library, a pattern, an architecture choice, an
  invariant like "no Redis") and its *why* would otherwise live only in chat or a docstring.
- You want a premise guarded by a **test that goes red if the premise breaks** — so the
  decision resurfaces instead of silently rotting.
- A bound test has gone red and you need to know **which decision it guards** (`ev why`) and
  pull the **full decision object** to re-judge (`ev reopen`).

## The flow

```sh
ev init                       # once per repo: create .evolving/
```

**Record a decision.** Each `--assume` opens a *chosen* ground; `--reject "<opt>: <why>"`
records a road-not-taken. Flags after a ground attach to it.

```sh
ev decide "restore-safety counter DB-backed; reject Redis" \
  --observe "multi-pod restore-safety counter" \
  --assume "no Redis; multi-pod coordination via the existing DB" \
  --assume-test "pytest tests/test_redis_absent.py" \
  --counter-test "pytest tests/test_redis_absent.py::test_redis_injection_flips_red" \
  --on-platform linux-ci --triggered-by pyproject.toml --surface pyproject-deps \
  --verified-at-sha <40-hex-commit> \
  --reject "Redis: a new infra dependency" \
  --blame "<the human accountable for this call>"
```

A **test binding** (`--assume-test`) is self-verifying: it MUST carry a `--counter-test`
(the test that should flip red if the claim breaks — proving the check can fail) plus at
least one `--on-platform` / `--triggered-by` / `--surface`. A **human re-check** instead is
`--revisit "<when/where a person re-affirms it>"`.

**Bind a test after the fact** to an unbound ground of the *current HEAD* decision (writes a
new child — the chain is immutable):

```sh
ev guard "<test selector>" <HEAD-id> "<ground claim>" \
  --counter-test "<selector>" --on-platform linux-ci --triggered-by schema.sql --surface ddl
```

**Audit, evaluate, resurface:**

```sh
ev verify                       # the chain is intact and every refusal holds
ev check --run --exit-on-red    # run bound tests, record receipts; exit non-zero on any not-green
ev why "<test selector>"        # which decision + ground does this test guard?
ev reopen <id>                  # pull the full decision object (frozen vs current + roads-not-taken)
ev show <id>                    # the raw tick
```

`ev check` reports a flat, unscored set — `green` / `red` / `gray→red` / `not-run` / `stale`
/ `silently-unbound` — each row naming the decision + ground. `--exit-on-red` makes it a CI
gate. (`ev reopen` only *presents*; the re-judgment is a new `ev decide` you author.)

## Work with the refusals, do not fight them

`ev` refuses, by design — if a command errors, satisfy the refusal rather than working around it:

- **Every decision names a human** — pass `--blame "<name>"` (or have `git config user.name`
  set). Be honest about who is on the hook for the call.
- **A human re-check can never be force-bound to a test** (`--revisit` and `--assume-test`
  are exclusive on one ground).
- **A road-not-taken carries no check** — `--reject` grounds record *why* an option was
  declined; they take no `--assume-test`.
- **A test binding is never vacuous** — it needs a `--counter-test` and non-empty
  platform/trigger/surface.
- **The system is never the subject of self-evolve language** — write "the team will
  re-vet…", not "the system will self-improve…".

## Honesty boundary

`ev` answers one question well — *does a human-vetted decision stay live, and is the check
guarding it itself alive?* It surfaces liveness as a **fact** and never claims tamper-
resistance of offline test outcomes, nor does it judge for you. A red check is an invitation
for a human to re-decide, not a verdict.
