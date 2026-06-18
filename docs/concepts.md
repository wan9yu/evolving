# The `ev` model

This is the model in depth — deeper than the project README. It describes the on-disk Tick
schema, the parts of a decision (Ground, Check), content-addressed identity and the frozen
golden vectors, append-only immutability, the refusals `ev verify` enforces, and the
honesty / trust boundary. Everything here is accurate to the `0.0.1` code; nothing is
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
- `held_since` — a reserved timestamp slot (unused).
- `blame` — the human author on the hook for this decision.

On disk a tick is stored as pretty JSON containing the hashed payload keys **plus** the
four bookkeeping keys at top level. `ev show` prints that file as-is. A minimal genesis
tick on disk looks like:

```json
{
  "decision": "d",
  "observe": "o",
  "grounds": [
    { "claim": "c", "supports": "chosen", "check": { "by": "person", "ref": "Q3 review" } }
  ],
  "parent_id": "",
  "id": "e2b337f53a1f",
  "status": "live",
  "held_since": "",
  "blame": "Wang Yu"
}
```

Because `blame`, `status`, and `held_since` sit outside the hash, blanking `blame` on disk
does **not** change the `id` — which is exactly why `ev verify` checks `blame` separately
(R5).

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
  - `counter_test` — the test that should flip **red** if the claim breaks (a binding with
    no counter-test is refused as vacuous).
  - `liveness` — three **non-empty** string sets that say where the test must keep running
    for the binding to be considered alive: `platforms`, `triggered_by`, `surfaces`. In the
    canonical form these sets are sorted and de-duplicated, so their order does not affect
    identity.
  Created with `--assume-test` (plus `--counter-test`, `--on-platform`, `--triggered-by`,
  `--surface`), or after the fact with `ev guard`.

A ground may carry **at most one** check, and never both shapes at once.

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

Two **frozen golden vectors** pin this function so the hashing can never silently drift:

| Vector | id |
| --- | --- |
| `genesis` | `e2b337f53a1f` |
| `case1` | `638c47b0c9dd` |

`ev verify --self-test` recomputes both and fails if either id moves.

## Append-only immutability

The chain is never edited in place. **A change is a new child** whose `parent_id` points at
its predecessor, and whose own `id` is the hash of its (new) payload. This is why
`ev guard` — which adds a check, a *hashed* field — writes a **new child** rather than
mutating the tick it targets. `HEAD` tracks the latest tick; `ev guard` can only amend the
current `HEAD`.

## The refusals (R1–R6) as `ev verify` enforces them

`ev verify` scans every tick file and reports **all** violations it finds (not just the
first). The refusals:

- **R1 — closed schema.** Every tick, ground, check, and liveness object is parsed
  strictly: any field outside its fixed key set is rejected
  (`field outside closed schema: <k>`). Reported as `R1/R2`.
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
fail on external state should be run on a timer (a 0.1.x capability), not bound to
`triggered_by`.
