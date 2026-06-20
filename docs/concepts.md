# The `ev` model

This is the model in depth — deeper than the project README. It describes the on-disk Tick
schema, the parts of a decision (Ground, Check), content-addressed identity and the frozen
golden vectors, append-only immutability, the refusals `ev verify` enforces, and the
honesty / trust boundary. Everything here is accurate to the code; nothing is
overstated.

For the commands that produce and read these records, see [commands.md](commands.md).

## The Tick

A **Tick** is one decision in the chain. Its fields split into two groups by whether they
enter the content hash.

**Hashed payload** — the four fields that define the decision's identity:

- `decision` — the decision text.
- `observe` — the situation observed when the decision was made (may be empty).
- `grounds` — the ordered list of reasons the decision rests on (see below).
- `parent_id` — the id of the predecessor tick; `""` on genesis.

**Bookkeeping** — recorded but **excluded** from the hash, so they can change without
forging a new identity:

- `id` — the content-addressed identifier (the hash output; see *Identity*).
- `status` — the tick's status string (`"live"`).
- `held_since` — an RFC 3339 timestamp stamped when the tick is written (at `ev decide` /
  `ev guard`). It is bookkeeping (excluded from the hash), so it never affects the `id`.
- `blame` — the human author on the hook for this decision.
- `authority` — an optional declared tag (`user-ruled` | `agent-disposable`), excluded from
  the hash; written only when set, and surfaced by `show`/`list`/`brief`/`reopen`.
- `jurisdiction` — an optional declared tag (`A` | `B` | `C` | `D`), excluded from the hash;
  written only when set. `A`/`B` may gate; `C`/`D` are **detect-only** — structurally
  ungateable (see *Jurisdiction* below). Surfaced by `show`/`list`/`reopen`.
- `round_id` — an optional declared join/dedup key (e.g. `R2289`, `#555`, `§3`), excluded
  from the hash; written only when set. A structured, durable substitute for fishing a round
  token out of `observe` — used by `ev migrate` to dedup and reconcile a backfill. Surfaced
  by `show`/`list`/`reopen`.

On disk a tick is stored as pretty JSON containing the hashed payload keys **plus** the
bookkeeping keys at top level (`id`, `status`, `held_since`, `blame`, and — when set —
`authority`, `jurisdiction`, `round_id`). `ev show` prints that file as-is. The genesis tick
on disk looks like:

```json
{
  "decision": "freeze the retrieval schema for v2",
  "observe": "evaluating retrieval backend",
  "grounds": [
    {
      "claim": "team still wants a frozen schema",
      "supports": "chosen",
      "check": { "by": "person", "ref": "Q3 infra review" }
    },
    { "claim": "pgvector would lock our schema", "supports": "rejected:pgvector" }
  ],
  "parent_id": "",
  "id": "e2b337f53a1f",
  "status": "live",
  "held_since": "<rfc3339-time>",
  "blame": "Wang Yu"
}
```

This is the frozen `genesis` golden vector, so its `id` is genuinely `e2b337f53a1f` (the
same id pinned in the *Identity* table below). The `held_since` is shown as a placeholder; the
real one is an RFC3339 time stamped at write time.

Because `blame`, `status`, `held_since`, `authority`, `jurisdiction`, and `round_id` sit
outside the hash, blanking `blame` on disk does **not** change the `id` — which is exactly
why `ev verify` checks `blame` separately (R5). Equally, tagging a `jurisdiction` or a
`round_id` on a decision leaves its `id` untouched: these are declared bookkeeping, never
part of the decision's identity.

## Ground

A **Ground** is a single reason a decision rests on. It has three parts:

- `claim` — the reason text (non-empty).
- `supports` — either the literal `"chosen"` (a reason **for** the decision taken) or
  `"rejected:<option>"` (a **road-not-taken**: a reason an alternative was declined). For a
  rejected support, the `<option>` part must be non-empty.
- `check` — an optional Check that keeps the ground honest over time. When absent, the
  `check` key is **omitted entirely** from the JSON — it never serializes as `null`.

`ev decide --assume <claim>` opens a chosen ground; `ev decide --reject "<opt>: <why>"`
opens a rejected road with `claim = <why>` and `supports = rejected:<opt>`.

## Check

A **Check** is what keeps a chosen ground honest as the world changes. It is one of two
shapes, distinguished on disk by the `by` field:

- **Person** — a human re-check. `{ "by": "person", "ref": <reference> }`, where
  `reference` names when/where a person re-affirms the ground (e.g. `"Q3 infra review"`).
  Created with `--revisit`.

- **Test** — a test that guards the ground. `{ "by": "test", "ref": <selector>,
  "verified_at_sha": <40-hex>, "counter_test": <selector>, "liveness": { … } }`:
  - `reference` (`ref`) — the test selector that should pass while the claim holds.
  - `verified_at_sha` — the commit the test was last verified at; exactly 40 lowercase hex.
  - `counter_test` — **optional**: the test that should flip **red** if the claim breaks.
    When present it is a non-empty selector; when **absent** the key is **omitted entirely**
    from the JSON (it never serializes as `null` or `""`). An authored binding from
    `ev decide` / `ev guard` always carries one (such a binding without it is refused as
    vacuous). A **harvested** binding (`ev migrate`) deliberately carries **none** — see
    *Harvested bindings* below.
  - `liveness` — three **non-empty** string sets that say where the test must keep running
    for the binding to be considered alive: `platforms`, `triggered_by`, `surfaces`. In the
    canonical form these sets are sorted and de-duplicated, so their order does not affect
    identity.
  Created with `--assume-test` (plus `--counter-test`, `--on-platform`, `--triggered-by`,
  `--surface`), or after the fact with `ev guard`; or, **without** a counter-test, harvested
  by `ev migrate`.

A ground may carry **at most one** check, and never both shapes at once.

### Harvested bindings (counter-test absent)

A **harvested** Test binding is one whose `counter_test` is **absent**. It is the shape
`ev migrate` produces when it adopts an *existing* test as a check: the test is real and its
liveness is fully declared (a harvest still demands a platform, a trigger, and a surface — you
cannot half-harvest), but its **falsifiability was never proven** — no one has shown a
counter-test that flips red, so the binding could in principle be vacuous.

`ev check` evaluates a harvested binding **exactly** as a normal one — a passing harvested
test still reads `green`, a failing one still reads `red` — and never silently upgrades it.
What it adds is an honest **annotation**: a harvested row is tagged
`(harvested — falsifiability not proven; …)`, and a trailing
`harvested-unproven: N of M test bindings have no counter-test (run ev guard to add one)` line
counts the debt. `ev guard` is the way out: add a `--counter-test` and the harvested binding
becomes a proven, authored one (a new child tick, since the check is hashed).

Because the canonical encoding **omits** `counter_test` on absence (rather than emitting it as
`null`), a harvested id is just as byte-stable as a counter-test-carrying one — a third frozen
golden, `harvested` (`0cf784b51331`), pins exactly that.

## Identity

`id = first 12 hex characters of SHA-256` over the **canonical JSON** of the hashed payload
`{decision, observe, grounds, parent_id}` — and only those fields. The canonical encoding
is RFC 8785 / JCS: object keys sorted, compact separators, raw (un-escaped) UTF-8. This
holds here precisely because the payload is **string-only** — it carries no numbers,
booleans, or nulls, so JCS number canonicalization never has to be applied. Liveness sets
are sorted and de-duplicated before hashing; the `grounds` array keeps its authored order.

Because identity is the hash of the payload, **any change to a hashed field produces a
different `id`** — there is no in-place edit (see *Append-only*). Conversely, editing a
bookkeeping field (e.g. `blame`) leaves the `id` unchanged.

Three **frozen golden vectors** pin this function so the hashing can never silently drift:

| Vector | id |
| --- | --- |
| `genesis` | `e2b337f53a1f` |
| `case1` | `638c47b0c9dd` |
| `harvested` | `0cf784b51331` |

`ev verify --self-test` recomputes all three and fails if any id moves. `harvested` is
`case1` with its first ground's `counter_test` **omitted** — it pins that omit-on-absence
keeps a harvested binding's id byte-stable, so adding the optional-counter-test schema moved
no existing id.

## Append-only immutability

The chain is never edited in place. **A change is a new child** whose `parent_id` points at
its predecessor, and whose own `id` is the hash of its (new) payload. This is why
`ev guard` — which adds a check, a *hashed* field — writes a **new child** rather than
mutating the tick it targets. `HEAD` tracks the latest tick; `ev guard` can only amend the
current `HEAD`.

## Jurisdiction — and the C/D *structurally ungateable* guarantee

A decision may carry a declared **`jurisdiction`** tag from the closed vocabulary
`{A, B, C, D}` (out-of-vocabulary is refused). It is bookkeeping — not hashed — and it answers
one question: *may a not-green check on this decision fail a build?*

- **`A` / `B` — may gate.** A decision in jurisdiction `A` or `B` behaves exactly as an
  un-tagged one: a bound check that reads red (or stale, not-run, …) trips `ev check
  --exit-on-red`. These are the decisions this repo owns and is willing to be stopped by.
- **`C` / `D` — detect-only, *structurally* ungateable.** A decision in jurisdiction `C` or
  `D` may be **surfaced** but can **never gate**. This is enforced by two independent locks,
  not by convention:
  - **Gate-time lock.** In `ev check`, any not-green verdict on a `C`/`D` decision is mapped to
    the non-gating **`memo`** verdict *before* the `--exit-on-red` writer sees it. The row
    still prints (with the `memo` label, naming the decision), so the fact is never hidden —
    it just cannot flip the exit code. `memo` is a co-equal, non-gating fact, the sibling of
    `exempt`.
  - **At-rest lock.** `ev verify` refuses a `C`/`D` tick that carries **any** `Check::Test`
    on a ground (`a C/D jurisdiction (detect-only) tick may carry no test check`). A
    detect-only decision must hold no runnable test binding at all — so there is nothing that
    *could* gate. (This is a distinct invariant from the no-vacuous-binding rule; it is
    checked separately.)

The two locks together make "detect-only" a **structural** property of the record, not a flag
a future code path might forget to honor: a `C`/`D` decision is ungateable at the gate *and*
cannot even store a gating check at rest. This is what lets a repo import another team's
history — rulings it wants to *watch* but has no authority to *fail on* — as jurisdiction `C`,
honestly: surfaced forever, gating never.

## Forward-compat — the two-tier schema

`ev`'s on-disk schema is **closed** for everything that defines identity, and **tolerant** for
everything that does not — a two-tier rule that lets a newer writer add a bookkeeping field
without bricking an older reader:

- **Tier 1 — the hashed/identity set is STRICT.** The keys `{decision, observe, grounds,
  parent_id, id, status, held_since, blame}`, and every nested key inside `grounds` / `check`
  / `liveness`, are parsed against a closed schema: a missing identity field, or any *unknown*
  key **inside** the hashed payload, is an error (`field outside closed schema: <k>`). The
  content-addressed id can never carry an unvalidated field.
- **Tier 2 — unknown top-level non-hashed keys are TOLERATED.** A truly-unknown top-level key
  (one outside both the identity set and the known-non-hashed allow-list `{authority,
  jurisdiction, round_id}`) is **parsed through**, not rejected, so a tick written by a future
  `ev` still loads. `ev verify` surfaces it as a **`warning:`** (not a violation), naming the
  key, so a *typo'd* field name stays visible rather than silently swallowed.

There is an inert `schema_version` recorded in the store config; it is read **lazily**, only at
this tolerate-vs-reject decision, and is not a parsed config field.

### The forward-compatibility limit

Forward-compat is forward-only and cannot be retrofitted. A binary that predates a bookkeeping
field has a schema closed for **all** top-level keys, so a tick that carries a newer field
(`jurisdiction`, `round_id`, or any future tolerated key) is **rejected** by that older
`ev verify`, not tolerated — there is no way to teach an already-shipped reader to ignore a
field added after it. The two-tier rule buys tolerance for *future* fields going forward; it
cannot reach backward to a reader that already shipped. Stated plainly so no one assumes a
guarantee that does not exist.

## The refusals (R1–R6) as `ev verify` enforces them

`ev verify` scans every tick file and reports **all** violations it finds (not just the
first). The refusals:

- **R1 — closed schema (hashed) + tolerant (non-hashed).** Every tick, ground, check, and
  liveness object is parsed strictly *for the hashed/identity tier*: any field inside the
  hashed payload outside its fixed key set is rejected (`field outside closed schema: <k>`).
  A truly-unknown *top-level non-hashed* key is **tolerated** (parsed through) and surfaced as
  a `warning:`, never an error — see *Forward-compat* above. A `C`/`D`-jurisdiction tick that
  carries any test check is rejected (`a C/D jurisdiction (detect-only) tick may carry no test
  check`). Reported as `R1/R2`.
- **R2 — check shape.** A check must be exactly a Person (`by`/`ref`) or a Test
  (`by`/`ref`/`verified_at_sha`/`counter_test`/`liveness`) — never a mix; `by` must be
  `"test"` or `"person"`; a test's `verified_at_sha` must be 40 lowercase hex; liveness
  sets must be non-empty. (At write time, R2 also forbids a single ground being both
  `--revisit` and `--assume-test`, and `ev guard` refuses to force a test onto a Person
  ground.) Reported as `R1/R2`.
- **R4 / R6 — id == hash + chain integrity.** Each stored tick's recomputed hash must equal
  its filename (`id != hash(payload) (R4/R6)`), the in-file `id` field must equal the
  filename (`stored id field … != filename (R6)`), every non-empty `parent_id` must resolve
  to an existing tick (`parent_id … does not resolve (R6)`), and the parent chain must be
  acyclic (`parent chain has a cycle (R6)`).
- **R5 — every mutating op names a human.** A tick with empty `blame` is a violation
  (`empty blame (R5)`). A best-effort lexical lint also flags forbidden machine-initiated
  op language (e.g. `auto-close`, `auto-prune`, `self-stop`, `auto-inherit`).
- **R3 — the system is never the subject of self-evolve language.** A best-effort lexical
  lint flags self-evolve / self-improve verbs (e.g. `self-evolve`, `self-improve`,
  `self-grade`) in the free-text fields, where the subject should be a human, not the
  system.

The R3 and R5 lints are **heuristics over fixed word lists**: a re-wording evades them. They
are surfaced honestly as best-effort, not as semantic guarantees.

## Honesty / trust boundary

`ev` completes one specific picture: *does a human-vetted decision stay live, and is the
check guarding it itself alive?* It does that by content-addressing the decision record and
by demanding that every test binding name a counter-test and the surfaces that keep it
live — so a check that has quietly died becomes visible.

It does **not** claim tamper-resistance of offline test outcomes. `ev` records that a test
was bound and the commit it was verified at, but it cannot prove an offline test result was
honest. That is a documented boundary, not a guarantee — the same framing as the project
[README](../README.md).

`ev`'s triggers are **git-recorded**: a binding's `triggered_by` paths and a bound check
going red are both detected from the commit history (`ev check` compares the latest receipt's
commit against the declared `triggered_by` paths). **External-state drift** — a UI click, an
org/config change, or an upstream-API behavior change that leaves **no git commit** — does
**not** fire `ev`. `ev` is decision memory, not an environment sentinel; a check that can only
fail on external state should be run on a timer (not currently supported), not bound to
`triggered_by`.
