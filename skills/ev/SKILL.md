---
name: recording-decisions-with-ev
description: Use when a technical decision is being made, a premise needs an invariant guarding it, or a guarding test has gone red — record and resurface it with `ev` (git for decisions), an immutable content-addressed decision ledger, instead of letting the reasoning scroll away in chat or a docstring. Also use at session start to load the decisions a human has already ruled on, so a fresh agent does not re-open a settled call.
---

# Recording & resurfacing decisions with `ev`

`ev` is **git for decisions**: it records a human-vetted decision and the grounds it
rests on as an immutable, content-addressed chain, binds a falsifiable test (or a human
re-check) to each ground, and **resurfaces the whole decision — named — when a bound check
goes red**. It deals in **facts, not verdicts**: no scores, no ranks, no auto-judgements.

Install: `cargo install evolving` (the command is `ev`). The store lives in `.evolving/`.

## Two roles

An agent uses `ev` in one of two roles. Know which one you are in.

### Reader role — an ephemeral / fresh-start agent

At the **start of a session**, before proposing anything, load what a human has already
ruled on:

```sh
ev brief                       # 0-network, local read: the user-ruled decisions + the roads they rejected
```

`ev brief` prints only the **live, `user-ruled`** decisions and, under each, the options
they explicitly rejected (`rejected <option>: <why>`). It does no git and no receipt I/O —
it is a near-zero-cost boot read.

**Respect what it prints:**

- Do **not** re-open a settled user ruling, and do **not** re-propose a road it shows as
  rejected. A `user-ruled` decision is the human's call; you are reading it, not re-deciding it.
- Need the detail behind one? `ev reopen <id>` shows the full decision object (grounds, each
  ground's current verdict, the roads-not-taken). `ev list` inventories every decision (its
  `authority=` tag is printed when set); `ev log` walks the lineage newest-first.
- `ev reopen` only **presents** a decision. If a ruling genuinely needs to change, that is a
  new `ev decide` a human authors — not an in-place edit.

### Curator role — a persistent / orchestrating agent

You record decisions, bind checks, and run the resurface gate.

**Record a decision.** Each `--assume` opens a *chosen* ground; `--reject "<opt>: <why>"`
records a road-not-taken. Flags after a ground attach to it. Set `--authority user-ruled`
when you are capturing a **human's** ruling (so a future fresh agent sees it via `ev brief`);
use `--authority agent-disposable` for a working call an agent may later revise.

```sh
ev decide "restore-safety counter DB-backed; reject Redis" \
  --observe "multi-pod restore-safety counter" \
  --assume "no Redis; multi-pod coordination via the existing DB" \
  --assume-test "pytest tests/test_redis_absent.py" \
  --counter-test "pytest tests/test_redis_absent.py::test_redis_injection_flips_red" \
  --on-platform linux-ci --triggered-by pyproject.toml --surface pyproject-deps \
  --verified-at-sha <40-hex-commit> \
  --reject "Redis: a new infra dependency" \
  --authority user-ruled \
  --blame "<the human accountable for this call>"
```

A **test binding** (`--assume-test`) is self-verifying: it MUST carry a `--counter-test`
(the test that should flip red if the claim breaks — proving the check can fail) plus at
least one `--on-platform` / `--triggered-by` / `--surface`. A **human re-check** instead is
`--revisit "<when/where a person re-affirms it>"`.

**Seed a decision that already lives in a commit** with `--from-git <commit>`: the decision
text becomes the commit subject, the default `--blame` becomes the commit author, and any
`Refs #<n>` body lines are carried into `observe` as provenance. The **grounds are still
added by hand** (`--assume` / `--reject`) — they are never inferred from the diff or body:

```sh
ev decide --from-git <commit> \
  --assume "<why this holds>" \
  --reject "<option>: <why declined>" \
  --authority user-ruled
```

**Bind a test after the fact** to an unbound ground of the *current HEAD* decision (writes a
new child — the chain is immutable):

```sh
ev guard "<test selector>" <HEAD-id> "<ground claim>" \
  --counter-test "<selector>" --on-platform linux-ci --triggered-by schema.sql --surface ddl
```

**Run the resurface / liveness gate** and surface anything not-green to the human:

```sh
ev check --run --platform linux-ci --exit-on-red --attest linux-ci,linux-arm
ev why "<test selector>"        # which decision + ground does this selector guard?
ev reopen <id>                  # pull the full decision object (frozen vs current + roads-not-taken)
```

`ev check` reports a flat, **unscored** set of facts — `green` / `red` / `gray->red` /
`not-run` / `stale` / `silently-unbound` (and `exempt` under `--attest`) — each row naming
the decision + ground. `--exit-on-red` makes any not-green a non-zero exit (a CI gate). Pass
`--attest <p1,p2>` with the platforms **this runner speaks for**: a declared platform this
runner does not attest is reported `exempt` (non-gating) here rather than `not-run`, so a
single runner never falsely fails another runner's platform. As the curator, **surface any
red / not-run / stale / silently-unbound to the human** — these are invitations to re-decide,
not verdicts the agent should silently act on.

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

## Honesty boundary — what `ev` does and does NOT promise

`ev` answers one question well — *does a human-vetted decision stay live, and is the check
guarding it itself alive?* Respect these limits; do not over-claim them to the human:

- **Facts, not verdicts.** `ev check` emits flat states, never a score or a rank. A red check
  is an **invitation for a human to re-decide**, not a pass/fail judgement and not an
  instruction to the agent.
- **Detect, not prevent.** `ev` surfaces a broken assumption; it does not block the change
  that broke it.
- **The counter-test is declared, not executed** in this version. `ev check` prints a note
  saying so. Do **not** trust a guard that has never been *shown* to flip red — author-declared
  falsifiability is not machine-proven falsifiability.
- **`ev` does not fire on external-state drift.** Its triggers are **git-recorded**: a bound
  check going red, or a commit touching a declared `triggered_by` path. A UI click, an
  org/config change, or an upstream-API behavior change that leaves **no git commit** will not
  trigger `ev`. It is decision memory, not an environment sentinel.
- **Only ~half of decisions are machine-bindable.** The rest are capture plus a human re-check
  reminder (`--revisit`) — and that is fine. Do not invent a contrived test just to bind a
  ground; an honest "a person re-affirms this at <when>" is the correct check.

It never claims tamper-resistance of offline test outcomes, nor does it judge for you.
