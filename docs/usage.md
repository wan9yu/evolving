# Using `ev` — a task-oriented guide

This guide is organized by **what you are trying to do**, not by command. For the full flag
reference (every flag, exit code, exact output string) see
[commands.md](commands.md); for the model behind it — Ticks, Grounds, Checks, the refusals —
see [concepts.md](concepts.md).

`ev` is **git for decisions**: it records a decision and the grounds it rests on as an
immutable, content-addressed chain, binds a falsifiable test (or a human re-check) to each
ground, and **resurfaces the decision — named — when a bound check goes red**. It deals in
**facts, not verdicts**.

Create the store once per repo:

```sh
ev init
```

---

## "I just made a decision (and rejected some alternatives)"

Capture the decision, the reason(s) it rests on, and the roads you did *not* take. Mark it
`user-ruled` so a fresh agent (or a future you) sees it before re-deciding, and name who is
on the hook:

```sh
ev decide "build our own retrieval; reject pgvector" \
  --assume "team has bandwidth to maintain it long-term" \
  --reject "pgvector: would lock our schema" \
  --authority user-ruled \
  --blame "You"
```

Each `--assume` opens a **chosen** ground; each `--reject "<option>: <why>"` records a
**road-not-taken**. Repeat either to add more. `--authority agent-disposable` instead marks
a working call an agent may later revise.

---

## "Capture a decision that already lives in a commit"

Seed the decision straight from a commit: its subject becomes the decision text. Blame defaults
to a leading `<Role>:` subject prefix when present (the closed set Dev / QA / Product / Mac / User),
else the commit author — and `--blame` overrides either. Provenance is carried into `observe`: the
subject's own `#<n>` / `R<n>` tokens first, then any `Refs #<n>` lines in the body. The
**grounds are still yours to add** — they are never inferred from the diff:

```sh
ev decide --from-git <commit> \
  --assume "<why this holds>" \
  --reject "<option>: <why declined>" \
  --authority user-ruled
```

(Pass either a decision in quotes **or** `--from-git`, not both.)

---

## "What have we already decided / what's ruled?"

Start with the rulings and the closed roads — a fast, no-network read; load-bearing rulings
(those that closed a road) come first, then newest first:

```sh
ev brief            # the live, user-ruled decisions + the options each one rejected
ev brief --limit 5  # cap the count; --limit 0 shows all (default cap is brief_limit=10)
```

`ev brief` pins **load-bearing rulings** — user-ruled decisions that closed a road via
`--reject` — **above the cap** so recency never buries the closed roads a fresh agent must not
re-walk; the rest follow **most-recent-first**, and the output is **capped** (default
`brief_limit`=10 from config; `--limit N` overrides; `--limit 0` shows all). When the cap
drops decisions it prints a `… N more user-ruled decision(s) — \`ev list\` for all` footer
(with a `, M with rejected roads` clause when any hidden ruling closed a road), so nothing —
least of all a closed-road ruling — is silently hidden.

See everything, not just the rulings, or pull one decision in full:

```sh
ev list         # every decision: id, status, text (and its authority tag when set)
ev reopen <id>  # one decision in full: each ground's current verdict + the roads-not-taken
```

`ev reopen` only **presents** a decision. To change a ruling, author a new `ev decide` — the
chain is append-only, never edited in place.

---

## "Is any decision's assumption broken, or its check not running?"

Run the resurface gate. It evaluates every test-bound ground and prints one flat, **unscored**
row per ground:

```sh
ev check --exit-on-red          # non-zero exit if anything is not green (a CI gate)
ev check --run --platform linux-ci   # also run each bound test for this platform first, recording a receipt
```

Reading the flat verdict (each is a co-equal **fact**, never a rank or score):

- **green** — the bound check ran and passed; the assumption still holds.
- **red** — the check ran and failed; the decision's assumption is broken — re-decide.
- **not-run** — the check has never run on a platform it declares; its liveness is unestablished.
- **stale** — a triggering commit landed after the last run, the run is older than the
  staleness window, or the verified-at commit is behind the live origin.
- **gray->red** — the last run was inconclusive (`gray`); treated as red, never silently dropped.
- **unproven** — `ev check --run` ran the counter-test and it did **not** flip (it agreed with the
  bound check); the check is **vacuous** and proves nothing until the counter-test is fixed.
- **silently-unbound** — a binding that is not in the selected set, so it can never be counted
  green — surfaced rather than ignored.
- **memo** — a not-green verdict on a **`C`/`D`-jurisdiction** (detect-only) decision: still
  printed and named, but **non-gating** by construction (it can never trip `--exit-on-red`).

Under `--attest <p1,p2>` (the platforms **this runner speaks for**), a declared platform this
runner does not attest is reported **exempt** (non-gating here) instead of not-run — so one
runner never falsely fails another's platform. A **harvested** binding (no counter-test, from
`ev migrate`) reads its real verdict but is tagged `harvested — falsifiability not proven`,
with a trailing `harvested-unproven: …` debt line — run `ev guard` to add a counter-test.

> `ev check --run` **executes** each counter-test to prove the binding can actually flip; a binding
> whose counter-test does not flip is reported **unproven** (vacuous). A red/not-run/stale/unproven
> row is an invitation to re-decide, not a machine verdict.

---

## "Bind a falsifiable check to an assumption after the fact"

When an assumption deserves a guarding test you did not bind at decision time, attach it to an
unbound ground of the **current HEAD** decision. Because the check is part of the hashed
record, this writes a **new child** (the chain is immutable):

```sh
ev guard "pytest tests/test_schema_frozen.py" <HEAD-id> "schema stays frozen" \
  --counter-test "pytest tests/test_schema_frozen.py::test_schema_change_flips_red" \
  --on-platform linux-ci \
  --triggered-by schema.sql \
  --surface schema-ddl
```

A test binding is **never vacuous**: the `--counter-test` (the test that should flip red if
the claim breaks) is mandatory, along with at least one platform, trigger, and surface.
`<HEAD-id>` is the id printed by the most recent `ev decide` / `ev guard`.

---

## "Catch a dependency that silently changes behavior"

A decision often rests on *how an external dependency behaves* — the shape of its output, the set
of fields it redacts, the columns it returns. That shape can drift under you when the dependency
upgrades, with no compile error and no failing build: a silent false-green.

The pattern that catches it has four parts:

1. **Snapshot the behavior-shape to a tracked file.** A small exporter (a script you own, run
   outside `ev`) writes the dependency's current behavior-shape to a file, e.g.
   `shape-snapshot.txt`. A human **reviews and commits** that file — the snapshot is the reviewed
   baseline, and its diff is the review surface.

2. **Capture the decision, bound to a PURE compare-check.** The check only *reads* the snapshot
   and the current shape and compares them — it is 0-network, builds no fixture, and so cannot
   fail-soft to a false-green:

   ```sh
   # the exporter refreshes the current shape on every CI run (your script, not ev):
   your-exporter > current-shape.txt

   ev decide "the behavior surface = {redaction set frozen}" --authority user-ruled \
     --assume "the dependency's behavior-shape matches the reviewed snapshot"

   ev guard "diff -q current-shape.txt shape-snapshot.txt" <HEAD-id> \
     "the dependency's behavior-shape matches the reviewed snapshot" \
     --counter-test "! diff -q current-shape.txt shape-snapshot.txt" \
     --on-platform linux-ci \
     --triggered-by current-shape.txt \
     --surface shape
   ```

3. **Gate every CI run.** `ev check --run` runs the compare-check (and proves it falsifiable via
   the inverse counter-test). While the shape matches, it is **green and proven**; the instant the
   dependency's output drifts, the check goes **red** and the decision resurfaces — naming what was
   assumed and why:

   ```sh
   ev check --run --platform linux-ci --exit-on-red
   ```

4. **Re-decide on the diff.** A red row points at the snapshot. Diff the current shape against it,
   decide whether the drift is acceptable, and — if it is — re-snapshot (review + commit the new
   `shape-snapshot.txt`) so the green is earned again, not assumed.

Because `diff -q` and `! diff -q` are logical inverses, `ev check --run` can *prove* the check is
falsifiable: a binding that can never flip is reported **unproven** and gates, so this pattern can
never decay into a check that always passes.

> **What this actually locks (honest scope).** This pattern is a **fixture-regression-lock**: it
> fires when the snapshot (or the freshly-exported current-shape) file changes **in a commit** —
> a git-recorded event on a `triggered_by` path. It is **not** a sentinel for the *silent runtime
> drift itself*: if a dependency's behavior changes but no exporter re-runs and no file is
> committed, nothing fires (`ev` is decision memory, not an environment monitor — see the
> external-state-drift boundary below). It locks the **reviewed fixture** against regression,
> which is narrower than catching every silent drift — run the exporter in CI so the current
> shape is re-committed on every run, and the lock has something to compare against.

---

## "Migrate your existing decisions, and harvest the tests you already have"

You do not start from an empty ledger. A team usually already has its decisions written
somewhere — a chat-room/git log, a `RESOLVED` / `FLAG` to-human doc, a numbered
decisions-immutable document, an escalation log — and a pile of tests that already guard those
decisions. `ev migrate` backfills that history into the ledger and adopts those tests, without
re-typing anything.

```sh
ev migrate \
  --source gitlog:chat-room.md \
  --source decisions-immutable:DECISIONS.md \
  --blame "Wang Yu"          # fallback author for any un-attributed record
```

Each source is read by a format-aware extractor (`gitlog` / `to-human` /
`decisions-immutable` / `escalation`). It harvests the **rulings** and the **structured**
roads-not-taken (`rejected: <option>: <why>`) — and **only** those. A free-text prose reason is
**never** mined into a ground: a block with no structured road imports as an honest
zero-grounds capture, not a synthesized one.

It is **idempotent** — run it twice and the second pass writes nothing (records are deduped on
their durable `round_id` / round token). It **keeps the chain**: a back-dated mid-chain insert
is reported as *re-linked*, never rewritten. And it **never invents an author** — a record with
no author and no `--blame` fallback is reported as a source-only gap (R5 stays intact), not
imported with a fabricated name:

```sh
ev migrate --source gitlog:chat-room.md --blame "Wang Yu"
# → imported 7, skipped 0, re-linked 0, 2 source-only gap(s)
```

**Tag the imports as you backfill** with `--jurisdiction-map <path>` — this is **how a bulk-imported
decision gets its jurisdiction**. The map is a plain `source_key → bucket` file (one `<source_key>
<bucket>` pair per line; `#` comments and blank lines skipped; bucket ∈ `{A, B, C, D}`):

```sh
cat gateway.map
# # round-id -> bucket
# #1194 C
# R2289 C

ev migrate --source escalation:escalation.md --jurisdiction-map gateway.map --blame "Wang Yu"
```

A record whose key is in the map carries that jurisdiction; a record **absent** from the map imports
**untagged** (the map is additive). So a `C`/`D` import becomes **structurally detect-only** instead of
a gating record — the gateway record `#1194` mapped to bucket `C` imports as a permanent detect-only
MISS (surfaced forever via `memo`, gating never; see the watch-not-fail section below). Because
jurisdiction is non-hashed, tagging never moves a tick id, so the backfill stays idempotent. An
out-of-vocabulary or malformed map line is a hard error that names the offending line.

**Find the capture gap** — which rulings your source records but the ledger never captured:

```sh
ev migrate --reconcile --against to-human:to-human.md
# → reconcile: in-both 5, source-only 3 (the capture gap), store-only 1, un-keyable 0
```

**Harvest a test you already have** as a check. `ev migrate --bind-check` builds a *harvested*
binding — a real test with full liveness, but **no counter-test**, so its falsifiability is not
yet proven:

```sh
ev migrate --bind-check "pytest tests/test_redis_absent.py" \
  --on-platform linux-ci --triggered-by pyproject.toml --surface pyproject-deps
```

A harvested binding is honest about its debt. `ev check` evaluates it exactly like any other
(a passing harvested test reads `green`, a failing one `red`), but tags the row
`(harvested — falsifiability not proven; …)` and prints a trailing
`harvested-unproven: N of M test bindings have no counter-test (run ev guard to add one)`. The
way out is `ev guard`: add a `--counter-test` and the binding becomes a proven, authored
check.

### "Import another team's rulings to *watch*, but not to *fail on*"

Sometimes you want a decision in the ledger as a **detect-only** record — surfaced when it goes
red, but never able to fail *your* build (it is not yours to gate on). Tag it jurisdiction `C`
(or `D`):

```sh
ev decide "the gateway's #1194 invariant holds" --jurisdiction C \
  --observe "backfilled from the gateway history" \
  --assume "the imported invariant still holds" --blame "Wang Yu"
```

A `C`/`D`-jurisdiction decision is **structurally ungateable**: any not-green verdict on it is
mapped to the non-gating `memo` label, so it can never trip `ev check --exit-on-red`, and
`ev verify` refuses to let it carry a runnable test check at all. It is surfaced forever,
gating never — see [concepts.md](concepts.md) for the two-lock guarantee.

To tag rulings the **same way at import time** — so a bulk backfill lands `C`/`D` rather than
untagged-and-gateable — pass `ev migrate --jurisdiction-map <path>` (the `source_key → A/B/C/D`
map above): the gateway record `#1194` mapped to bucket `C` imports as a permanent detect-only
MISS, while any record absent from the map imports untagged.

---

## "Review the chain / its history"

```sh
ev log          # the decision lineage from HEAD back to genesis, newest first
ev verify       # audit the whole chain: every id == hash(payload), lineage forward-only, schema + refusals hold
```

`ev verify --self-test` reproduces the frozen golden vectors so the hashing can never silently
drift.

---

For the exact flags, exit codes, and output of every command, see
[commands.md](commands.md). For the model — the Tick schema, Grounds, Checks, content-addressed
identity, and the honesty / trust boundary — see [concepts.md](concepts.md).
