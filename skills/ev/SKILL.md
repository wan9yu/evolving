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

### Participant role — an ephemeral / fresh-start agent

You are not a blank slate. At the **start of a session**, before proposing anything, load the
judgment a human has already ruled on and let it govern what you propose:

```sh
ev brief --json                # machine-readable boot-read for an agent to parse (recommended)
ev brief                       # the same content as human text
ev brief --limit 5             # cap the count (default brief_limit=10; --limit 0 shows all)
```

`ev brief` surfaces only the **live, `user-ruled`** decisions and, under each, the options they
explicitly rejected. It does no git and no receipt I/O — a near-zero-cost boot read drawn **only
from human-ratified rulings** (an `agent-proposed` proposal never appears; it cannot govern you
until a human vouches for it). **Load-bearing rulings** — user-ruled decisions that closed a road
via `--reject` — are **pinned above the cap** so recency never buries a closed road you must not
re-walk; the rest follow **most-recent-first**, and the output is **capped** (default
`brief_limit`=10; `--limit N` overrides; `--limit 0` shows all), with a footer counting any hidden
rulings (and how many closed a road) so nothing drops silently.

**`ev brief --json` is the boot-read an agent should parse.** It emits one frozen object —
`{kind:"ev-brief", decisions:[{id, decision, load_bearing, rejected_roads:[{option, claim}],
source_ref?}], shown, total, elided, elided_load_bearing}`. Each decision carries a **citable
`id`**, and the `elided` / `elided_load_bearing` counts make any capped-off ruling **visible**: if
`elided` > 0, re-pull with `--limit 0` (or a higher cap) rather than act on a partial view.

**Let it govern, and cite it:**

- Do **not** re-open a settled user ruling, and do **not** re-propose a road it shows as
  rejected. A `user-ruled` decision is the human's call; you are reading it, not re-deciding it.
- When your work **proceeds on or near a settled ruling, cite its `id`** (e.g. "per ruling
  `<id>` …") so your reasoning traces back to the governing decision. `ev show <id>` /
  `ev reopen <id>` pull the full object (grounds, each ground's current verdict, the
  roads-not-taken); `ev list` inventories every decision (its `authority=` tag printed when set);
  `ev log` walks the lineage newest-first.
- If a ruling genuinely needs to change, that is a **new `ev decide` a human authors** — never an
  in-place edit, and never an agent quietly overriding it. You **surface the conflict** to the
  human; you do not resolve it yourself.
- If you run the gate and hit a **not-green** check, surface it — it is an invitation for a human
  to re-decide, not a verdict you should silently act on.

### Curator role — a persistent / orchestrating agent

You record decisions, bind checks, and run the resurface gate.

**Record a decision.** Each `--assume` opens a *chosen* ground; `--reject "<opt>: <why>"`
records a road-not-taken. Per-ground flags (`--revisit`, `--assume-test`, `--counter-test`,
`--on-platform`, `--triggered-by`, `--surface`) attach to the ground they follow; the
decision-global flags (`--observe`, `--blame`, `--authority`, `--verified-at-sha`,
`--from-git`) may appear anywhere. Set `--authority user-ruled`
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
text becomes the commit subject; the default `--blame` is a leading `<Role>:` prefix on the
subject (`Dev`/`QA`/`Product`/`Mac`/`User`) when present, else the commit author; and
provenance from the subject's own `#<n>` / `R<n>` tokens plus any `Refs #<n>` body lines is
carried into `observe`. The **grounds are still added by hand** (`--assume` / `--reject`) —
they are never inferred from the diff or body:

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

**Ingest an existing decision history** with `ev migrate` — don't re-type a ledger you
already have written down. The **primary intake** is the **Canonical Decision Intake Contract**
(`--source canonical:<path.jsonl>`): one JSON object per line on the closed envelope
`{kind:"ev-decision-intake", decision, observe?, grounds, blame?, authority?, jurisdiction?,
source_ref?, provenance?}`. An adopter with a bespoke format writes a small adapter (any
language) that parses *their* format and emits canonical JSONL — `ev` never sees the bespoke
markdown — and `ev` re-validates every line through its own read-path validators on the way in
(`ev` owns `id` / `parent_id`; the producer never supplies identity). The same JSONL a one-shot
adapter emits is what a future live runner emits natively — same contract, two producers. On
this path `provenance` defaults to `imported`. Four built-in convenience extractors handle
simple substrates: `--source <kind>:<path>` with kind ∈ `gitlog` / `to-human` /
`decisions-immutable` / `escalation`, each harvesting **rulings** and **structured**
roads-not-taken (`rejected: <opt>: <why>`) only — a prose reason is **never** NLP'd into a
ground (a block with no structured road imports as an honest zero-grounds capture). A migrate is
**idempotent** (a re-run writes nothing; records dedup on the key derived from `source_ref`),
**keeps the chain** (a back-dated insert is reported *re-linked*, never rewritten), and **never
invents an author** (a record with no author and no `--blame` fallback is a source-only gap, R5
intact). `ev` validates grounds are well-*formed*, never that an adapter parsed its source
*faithfully* — a mis-parse is a producer bug `ev` cannot catch:

```sh
ev migrate --source canonical:decisions.jsonl --blame "<fallback author>"   # primary, format-neutral intake
ev migrate --source gitlog:chat-room.md --source decisions-immutable:DECISIONS.md --blame "<fallback author>"
# → imported N, skipped M, re-linked K, J source-only gap(s)
ev migrate --reconcile --against to-human:to-human.md   # find the capture gap (source-only rulings)
```

**Tag the imports as you backfill** with `--jurisdiction-map <path>` — this is **how a bulk-imported
decision gets its jurisdiction** (an untagged import can gate; a `C`/`D` one cannot). The map is a
plain `source_key → bucket` file: one `<source_key> <bucket>` pair per line, `#` comments + blanks
skipped, bucket ∈ `{A, B, C, D}`. A record whose key is in the map carries that bucket; a record
**absent** from the map imports **untagged**. Because jurisdiction is non-hashed, tagging never moves
a tick id (the backfill stays idempotent); a bad bucket is a hard error naming the line. So a `C`/`D`
import becomes **structurally detect-only** — the gateway record `#1194` mapped to bucket `C` imports
as a permanent detect-only MISS (surfaced forever via `memo`, gating never):

```sh
ev migrate --source escalation:escalation.md --jurisdiction-map gateway.map --blame "<fallback author>"
# gateway.map:  `#1194 C`  on one line  →  #1194 imports detect-only, never able to gate
```

**Harvest an existing test** as a check with `ev migrate --bind-check <selector>` (full
liveness required; **no counter-test**, so falsifiability is not yet proven). A harvested
binding is evaluated like any other but `ev check` tags its row `harvested — falsifiability not
proven` and prints a `harvested-unproven: N of M …` debt line. **The way out is `ev guard`** —
add a `--counter-test` and the binding becomes proven. Do not present a harvested green as a
proven one.

**Correct a stale non-hashed tag** (`authority` / `jurisdiction` / `provenance`) on an existing
decision with `ev correct <id> [--authority <v>] [--jurisdiction <v>] [--provenance <v>] --blame
"<name>"`. Under append-only immutability it never rewrites the target — it appends a corrective
**child** that copies the target's hashed payload verbatim and carries the corrected tag (`ev
brief` / `ev list` then surface the child; the stale parent stays in `ev log`). At least one tag is
required; a no-op is refused (`nothing to correct`); an override wins, an unspecified tag inherits.
This is the remedy when **`ev migrate` reports a discrepancy** — a re-import whose resolved tags
differ from the stored tick is surfaced loudly (`… N discrepancy(ies) — see above`, never silently
skipped) precisely so a corrected ruling is not invisibly dropped; resolve it with `ev correct`,
not by editing a tick:

```sh
ev correct <id> --authority user-ruled --blame "<the human accountable>"   # a ruling imported as an open item now surfaces in `ev brief`
```

**Import a ruling to *watch*, not to *fail on*** — tag it `--jurisdiction C` (or `D`) on `ev decide`,
or, for a **bulk import**, give `ev migrate` a `--jurisdiction-map` so the backfilled record lands
`C`/`D` instead of untagged-and-gateable. A `C`/`D`-jurisdiction decision is **detect-only**: any
not-green verdict on it becomes the non-gating `memo` label (it can never trip `--exit-on-red`), and
`ev verify` refuses to let it carry a runnable test check at all. Use it for another team's rulings
you must surface but have no authority to gate on. (`--jurisdiction A`/`B` gate normally;
`--source-ref <key>` sets a durable, opaque source identity `ev` dedups on but never interprets.)

**Run the resurface / liveness gate** and surface anything not-green to the human:

```sh
ev check --run --platform linux-ci --exit-on-red --attest linux-ci,linux-arm
ev why "<test selector>"        # which decision + ground does this selector guard?
ev reopen <id>                  # pull the full decision object (frozen vs current + roads-not-taken)
```

`ev check` reports a flat, **unscored** set of facts — `green` / `red` / `gray->red` /
`not-run` / `stale` / `unproven` / `silently-unbound` (plus `exempt` under `--attest`, and
`memo` for a not-green on a `C`/`D` detect-only decision) — each row naming
the decision + ground. `--exit-on-red` makes any not-green a non-zero exit (a CI gate);
`exempt` and `memo` are non-gating. Pass
`--attest <p1,p2>` with the platforms **this runner speaks for**: a declared platform this
runner does not attest is reported `exempt` (non-gating) here rather than `not-run`, so a
single runner never falsely fails another runner's platform. As the curator, **surface any
red / not-run / stale / unproven / silently-unbound to the human** — these are invitations to re-decide,
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

And one item is a *warning*, not a hard refusal:

- **Self-evolve language gets a best-effort warning (not a refusal)** — if a free-text field
  makes *the system* the subject of self-evolve/self-improve language, `ev` prints a
  non-blocking `warning:` and still records the tick (a re-wording evades it). Heed it anyway:
  write "the team will re-vet…", not "the system will self-improve…".

## Honesty boundary — what `ev` does and does NOT promise

`ev` answers one question well — *does a human-vetted decision stay live, and is the check
guarding it itself alive?* Respect these limits; do not over-claim them to the human:

- **Facts, not verdicts.** `ev check` emits flat states, never a score or a rank. A red check
  is an **invitation for a human to re-decide**, not a pass/fail judgement and not an
  instruction to the agent.
- **Detect, not prevent.** `ev` surfaces a broken assumption; it does not block the change
  that broke it.
- **`ev check --run` executes the counter-test and proves falsifiability.** A binding whose
  counter-test does **not** flip is reported `unproven` (vacuous) and gates under `--exit-on-red`
  — do **not** trust a guard that has not been *shown* to flip. Without `--run` it is not re-proven,
  so run it in CI.
- **A harvested binding's falsifiability is *not* proven.** A binding from `ev migrate` carries
  no counter-test, so even a green is unproven — `ev check` says so (`harvested — falsifiability
  not proven`, plus a `harvested-unproven: …` debt line). Treat it as adopted-but-unproven debt
  and pay it down with `ev guard --counter-test`; never present a harvested green as a proven one.
- **`C`/`D`-jurisdiction decisions are detect-only.** They are surfaced (`memo`) but
  **structurally cannot gate** — by design, for rulings you watch but do not own. Do not present
  a `memo` as a passing gate, and do not try to force a test check onto one (`ev verify` refuses).
- **`ev` does not fire on external-state drift.** Its triggers are **git-recorded**: a bound
  check going red, or a commit touching a declared `triggered_by` path. A UI click, an
  org/config change, or an upstream-API behavior change that leaves **no git commit** will not
  trigger `ev`. It is decision memory, not an environment sentinel.
- **Only ~half of decisions are machine-bindable.** The rest are capture plus a human re-check
  reminder (`--revisit`) — and that is fine. Do not invent a contrived test just to bind a
  ground; an honest "a person re-affirms this at <when>" is the correct check.

It never claims tamper-resistance of offline test outcomes, nor does it judge for you.
