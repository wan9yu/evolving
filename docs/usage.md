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

Under `--attest <p1,p2>` (the platforms **this runner speaks for**), a declared platform this
runner does not attest is reported **exempt** (non-gating here) instead of not-run — so one
runner never falsely fails another's platform.

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
