# `ev` command reference

The authoritative reference for the `ev` command surface: the write side (`init`, `decide`,
`guard`) and the read side (`show`, `verify`, `why`, `reopen`, `list`, `log`). The package is
named `evolving`; the command is `ev`.

Every command returns a process exit code: **`0`** on success, **`1`** (failure) otherwise.
Throughout, errors are written to **stderr** as `error: <message>`; the per-command
sections quote the real strings.

For the model behind these commands — Ticks, Grounds, Checks, identity, and the refusals —
see [concepts.md](concepts.md).

- [`ev init`](#ev-init)
- [`ev decide`](#ev-decide)
- [`ev guard`](#ev-guard)
- [`ev show`](#ev-show)
- [`ev verify`](#ev-verify)
- [`ev why`](#ev-why)
- [`ev reopen`](#ev-reopen)
- [`ev list`](#ev-list)
- [`ev log`](#ev-log)

---

## `ev init`

**Synopsis:** create the `.evolving/` store in the current directory.

```
ev init
```

**Flags:** none.

**What it does:** creates the store layout — `.evolving/ticks/`, `.evolving/results/`
(a `receipts/` and a `state/` cache), an empty `.evolving/HEAD`, and a default
`.evolving/config`. It is **idempotent**: running it again on an existing store is a no-op
and does not overwrite anything.

**Exit code:** `0` when the store is created or already exists; `1` if the directory could
not be created.

**Output (stdout / stderr):**

- created: `created .evolving/  (content-addressed chain + results cache)`
- already present: `.evolving/ already exists (no-op)`
- error (stderr): `error: could not create .evolving/: <io error>`

**Example:**

```sh
ev init
# → created .evolving/  (content-addressed chain + results cache)
```

---

## `ev decide`

**Synopsis:** record a decision with its grounds (chosen reasons and roads-not-taken),
optionally binding a check to each chosen ground.

```
ev decide "<decision>" [trailing flags…]
```

The first positional argument is the **decision text** (required, non-empty). Everything
after it is a stream of trailing flags parsed **left-to-right** (see the grammar below).

**Flags:**

| Flag | Takes | Required | Effect |
| --- | --- | --- | --- |
| `--observe` | a string | no | Sets the decision's `observe` field (the situation being observed). |
| `--blame` | a name | no* | The author on the hook. *If omitted, falls back to `git config user.name`; one of the two must resolve to a non-empty name.* |
| `--verified-at-sha` | 40 lowercase hex | no | The commit a **test** binding was last verified at. If omitted, defaults to `git rev-parse HEAD`. Only used by test bindings. |
| `--reject` | `"<option>: <why>"` | no | Opens a **road-not-taken** ground: `supports = rejected:<option>`, `claim = <why>`. Splits on the first `:`; both sides must be non-empty. |
| `--assume` | a claim | no | Opens a **chosen** ground (`supports = chosen`) with that claim. |
| `--revisit` | a reference | no | Attaches a **Person** re-check to the most recent ground (`reference` = when/where a human re-affirms it). |
| `--assume-test` | a test selector | no | Attaches a **Test** check to the most recent ground (`reference` = the selector). |
| `--counter-test` | a selector | no† | The test that must flip red if the claim breaks. †Required to complete a test binding. |
| `--on-platform` | a platform | no† | Adds one liveness platform to the most recent ground. Repeatable. †≥1 required to complete a test binding. |
| `--triggered-by` | a trigger | no† | Adds one liveness trigger. Repeatable. †≥1 required to complete a test binding. |
| `--surface` | a surface | no† | Adds one liveness surface. Repeatable. †≥1 required to complete a test binding. |

A decision with no grounds is allowed (it simply records the decision text and `observe`).

### The binding grammar (left-to-right)

The trailing flags are walked in order and bound to **the most recently opened ground**:

1. `--assume <claim>` opens a new **chosen** ground; `--reject "<opt>: <why>"` opens a new
   **rejected** road. These are the only flags that start a new ground.
2. After a ground is opened, `--revisit`, `--assume-test`, `--counter-test`,
   `--on-platform`, `--triggered-by`, and `--surface` all attach to **that** ground until
   the next `--assume` / `--reject`.
3. `--revisit <ref>` makes the ground's check a **Person** re-check.
4. `--assume-test <selector>` + `--counter-test <selector>` + at least one `--on-platform`
   + at least one `--triggered-by` + at least one `--surface` (and a `verified_at_sha`,
   resolved from `--verified-at-sha` or `git rev-parse HEAD`) make the ground's check a
   **Test**.
5. `--observe`, `--blame`, and `--verified-at-sha` are decision-level, not per-ground.

If a per-ground flag appears before any `--assume` / `--reject`, it is refused:
`<flag> has no preceding --assume/--reject ground`. A missing value is refused with
`<flag> requires a value`. An unrecognized flag is refused with `decide: unknown flag <x>`.

### The refusals it enforces

- **Empty decision** → `decision text is empty`.
- **R2 — revisit XOR test.** A single ground cannot be both a Person re-check and a Test:
  `a ground cannot be both --revisit and --assume-test (R2)`.
- **A rejected road carries no check.** Attaching `--revisit` or `--assume-test` to a
  `--reject` ground →
  `a road-not-taken (rejected) ground cannot carry a check in 0.1.0 — reserved for a future rejection-rationale liveness feature`.
- **No vacuous test binding.** `--assume-test` without `--counter-test` →
  `a test binding requires --counter-test (no vacuous binding)`.
- **A test binding needs full liveness.** A test binding missing a platform, trigger, or
  surface → `a test binding requires at least one --on-platform, --triggered-by, and --surface`.
- **Liveness flags without a test.** `--counter-test` / `--on-platform` / `--triggered-by`
  / `--surface` on a ground that is not a test binding →
  `--counter-test/--on-platform/--triggered-by/--surface require --assume-test`.
- **Unresolvable `verified_at_sha`.** No `--verified-at-sha` and no git HEAD →
  `cannot resolve verified_at_sha (not a git repo?) — pass --verified-at-sha`; a malformed
  value → `verified_at_sha must be 40 lowercase hex: <sha>`.
- **An author must be named (R5).** No `--blame` and no `git config user.name` →
  `no author: pass --blame, or set git config user.name`; an explicit empty `--blame` →
  `--blame must be non-empty`.
- **No store.** Running outside an initialized store →
  `no .evolving/ store here — run \`ev init\` first`.

A best-effort **R3 lint** is also run over the decision / observe / claim text: a
self-evolve verb (e.g. `self-improve`) emits a `warning:` on stderr but does **not** fail
the command — a re-wording evades it.

**Exit code:** `0` on success; `1` on any refusal above.

**Output (stdout / stderr):**

- success (stdout): `recorded <id> (<n> ground(s))`
- failure (stderr): `error: <message>`

**Example** — a chosen ground re-checked by a human, plus a road-not-taken:

```sh
ev decide "build our own retrieval; reject pgvector" \
  --observe "evaluating retrieval backend for v2" \
  --assume "team has bandwidth to maintain it long-term" \
  --revisit "Q3" \
  --reject "pgvector: would lock our schema" \
  --blame "You"
# → recorded <id> (2 ground(s))
```

**Example** — a chosen ground guarded by a **test** instead of a human:

```sh
ev decide "restore-safety counter DB-backed; reject Redis" \
  --observe "multi-pod restore-safety counter" \
  --assume "no Redis; multi-pod coordination via the existing DB" \
  --assume-test "pytest tests/test_redis_absent.py" \
  --counter-test "pytest tests/test_redis_absent.py::test_redis_injection_flips_red" \
  --on-platform linux-ci \
  --triggered-by pyproject.toml \
  --surface pyproject-deps \
  --verified-at-sha d308afac1b2c3d4e5f60718293a4b5c6d7e8f901 \
  --reject "Redis: a new infra dependency" \
  --blame "You"
```

---

## `ev guard`

**Synopsis:** attach a test to an **unbound** ground of the **current HEAD** decision after
the fact. Because the check is part of the hashed payload, this writes a **new child** tick
rather than mutating the existing one.

```
ev guard "<selector>" <id> [<target>] \
  --counter-test "<selector>" \
  --on-platform <p> [--on-platform …] \
  --triggered-by <t> [--triggered-by …] \
  --surface <s> [--surface …] \
  [--verified-at-sha <40-hex>] [--blame "<name>"]
```

**Positional arguments:**

| Position | Name | Required | Meaning |
| --- | --- | --- | --- |
| 1 | `selector` | yes | The test selector to bind as the ground's check `reference`. |
| 2 | `id` | yes | The tick to amend — **must be the current HEAD**. |
| 3 | `target` | conditional | Which ground to bind: a **claim text** or a numeric **index**. Required only when more than one ground is still unbound. |

**Flags:**

| Flag | Takes | Required | Effect |
| --- | --- | --- | --- |
| `--counter-test` | a selector | yes | The test that must flip red if the claim breaks. |
| `--on-platform` | a platform | yes (≥1) | Liveness platforms. Repeatable. |
| `--triggered-by` | a trigger | yes (≥1) | Liveness triggers. Repeatable. |
| `--surface` | a surface | yes (≥1) | Liveness surfaces. Repeatable. |
| `--verified-at-sha` | 40 lowercase hex | no | Commit the test was verified at; defaults to `git rev-parse HEAD`. |
| `--blame` | a name | no | Author; defaults to `git config user.name`. |

**Target resolution:** with one unbound ground, `target` may be omitted. With more than
one, it is required — `more than one unbound ground — name the target (claim or index)`.
A numeric target out of range → `ground index <i> out of range`; a claim that matches no
ground → `no ground with claim "<t>"`; a claim that matches several →
`ambiguous: multiple grounds with claim "<t>"`.

**The refusals it enforces:**

- **HEAD only.** Amending anything other than HEAD →
  `guard can only amend the current HEAD decision; <id> is not HEAD (<head>)`.
- **A human re-check stays human (R2).** A Person-checked ground cannot be force-bound to a
  test → `a human-rechecked ground cannot carry a test (R2 hard error)`.
- **A rejected road carries no check.** →
  `a road-not-taken (rejected) ground cannot carry a test in 0.1.0 — reserved for a future rejection-rationale liveness feature`.
- **Already bound.** A ground that already has a check → `ground already has a check`.
- **No vacuous binding.** Empty `--counter-test` →
  `a test binding requires a counter-test (no vacuous binding)`.
- **Full liveness required.** Missing platform / trigger / surface →
  `a test binding requires at least one platform, triggered-by, and surface`.
- Plus the same `verified_at_sha` and `--blame` resolution rules as `ev decide`.
- **Unknown tick.** An `id` not present in the store → `no tick with id <id>`.

**Exit code:** `0` on success; `1` on any refusal above.

**Output (stdout / stderr):**

- success (stdout): `bound; wrote child <child-id>`
- failure (stderr): `error: <message>`

**Example** — bind a test to the `schema stays frozen` ground of the HEAD decision:

```sh
ev guard "pytest tests/test_schema_frozen.py" <HEAD-id> "schema stays frozen" \
  --counter-test "pytest tests/test_schema_frozen.py::test_schema_change_flips_red" \
  --on-platform linux-ci \
  --triggered-by schema.sql \
  --surface schema-ddl
# → bound; wrote child <child-id>
```

`<HEAD-id>` is the id printed by the most recent `ev decide` / `ev guard`.

---

## `ev show`

**Synopsis:** print one tick in full, exactly as stored on disk (the pretty JSON: hashed
payload plus the `id` / `status` / `held_since` / `blame` bookkeeping).

```
ev show <id>
```

**Flags:** none. The single positional `id` is required.

**Exit code:** `0` if the tick exists and is readable; `1` otherwise.

**Output (stdout / stderr):**

- success (stdout): the on-disk JSON of the tick, printed as-is.
- not found (stderr): `error: no tick with id <id>`
- read error (stderr): `error: reading <id>: <io error>`

**Example:**

```sh
ev show 638c47b0c9dd
```

---

## `ev verify`

**Synopsis:** audit the whole chain and its refusals; or, with `--self-test`, reproduce the
two frozen golden vectors.

```
ev verify [--self-test]
```

**Flags:**

| Flag | Takes | Required | Effect |
| --- | --- | --- | --- |
| `--self-test` | — | no | Recompute the two frozen golden-vector ids and exit. |

**What `ev verify` checks:** every tick parses against the closed schema (R1) and check
shape (R2); every stored `id` equals the hash of its payload and matches its filename
(R4 / R6); every `parent_id` resolves and the lineage is forward-only and acyclic (R6);
every tick carries a non-empty `blame` (R5); and a best-effort lexical lint flags
self-evolve subject (R3) and forbidden-op (R5) language. It reports **all** violations, not
just the first. See [concepts.md](concepts.md) for the refusals in depth.

**Exit code:** `0` when the chain is clean (or all golden vectors match); `1` when any
violation is found (or any golden id drifts), or if the store cannot be read.

**Output (stdout / stderr) — plain `ev verify`:**

- clean (stdout, two lines):
  - `✓ chain intact: every id == hash(payload), lineage forward-only`
  - `✓ every tick validates against the closed schema (R1) and check shape (R2)`
- violations (stdout, one `✗ <line>` per violation), then stderr: `<n> violation(s)`
- store read error (stderr): `error: reading store: <io error>`

**Output — `ev verify --self-test`:** one line per vector, `✓` or `✗`, e.g.

```
✓ genesis: e2b337f53a1f (want e2b337f53a1f)
✓ case1: 638c47b0c9dd (want 638c47b0c9dd)
```

**Example:**

```sh
ev verify
# → ✓ chain intact: every id == hash(payload), lineage forward-only
# → ✓ every tick validates against the closed schema (R1) and check shape (R2)

ev verify --self-test
```

---

## `ev why`

**Synopsis:** reverse lookup — given a test selector, name the decision(s) it guards. Scans
every **live** tick and matches the selector against each Test-bound ground's `reference`.

```
ev why <selector>
```

**Flags:** none. The single positional `selector` is required (the test selector to look up,
exactly as it was bound — e.g. `pytest tests/test_redis_absent.py`).

**What it does:** for every live tick, for every ground whose check is a **Test** whose
`reference` equals `selector`, it prints one line naming the tick file (its id), the guarded
decision, the guarding claim, and what that claim supports (`chosen` or `rejected:<option>`).
A selector that guards nothing is an error. Person re-checks and unbound grounds are never
matched (only Test bindings carry a selector).

**Exit code:** `0` when at least one ground matches; `1` when none match, or when there is
no store.

**Output (stdout / stderr):**

- match (stdout, one line per matching ground): `<file>\t<decision>\tguards: <claim> (<supports>)`
  — `<file>` is the tick id (bare), `<decision>` and `<claim>` are quoted (Rust `{:?}` debug
  form), `<supports>` is bare: e.g.
  `638c47b0c9dd\t"restore-safety counter DB-backed; reject Redis"\tguards: "Argus introduces no Redis; multi-pod coord via existing DB" (chosen)`
- no match (stderr): `"<selector>" guards nothing`
- no store (stderr): `error: no .evolving/ store here — run \`ev init\` first`

**Example:**

```sh
ev why "pytest tests/test_redis_absent.py"
# → <file>	"<decision>"	guards: "<claim>" (chosen)
```

---

## `ev reopen`

**Synopsis:** read one decision **as it stands now** — its text, what it observed, and the
present verdict of every ground (for Test grounds, the frozen-at commit vs. the live check
state). Read-only; it never writes a tick.

```
ev reopen <id>
```

**Flags:** none. The single positional `id` is required (the tick to reopen).

**What it does:** loads the tick, resolves the live staleness reference (offline — no network
fetch), then prints the decision, the `observe` line (only when non-empty), and one line per
ground. A **Test** ground shows the commit it was frozen at (first 8 hex of
`verified_at_sha`) and its present verdict from the same evaluator `ev check` uses (`green`,
`red`, `stale`, `not-run`, …). A **Person** ground and an unbound ground print their claim
without a check line.

**Exit code:** `0` when the tick exists and is readable; `1` when the id is missing or
unreadable.

**Output (stdout / stderr):**

- decision (stdout): `decision <id>: <decision>` — `<decision>` is quoted (`{:?}`).
- observe, only if non-empty (stdout): `observe: <observe>` — quoted.
- per ground (stdout, one line each, indented two spaces):
  - Test: `  [<supports>] <claim> — test <reference> frozen@<sha8> now: <verdict>` — `<claim>`
    and `<reference>` are quoted; `<sha8>` is the first 8 chars of `verified_at_sha`;
    `<verdict>` is the live verdict label.
  - Person: `  [<supports>] <claim> — person <reference>` — `<claim>` and `<reference>` quoted.
  - unbound: `  [<supports>] <claim>` — `<claim>` quoted.
- missing id (stderr): `error: no tick with id <id>`
- read error (stderr): `error: reading <id>: <io error>`

**Example:**

```sh
ev reopen 638c47b0c9dd
# → decision 638c47b0c9dd: "restore-safety counter DB-backed; reject Redis"
# → observe: "multi-pod restore-safety counter — chat-room R2289→R2290"
# →   [chosen] "Argus introduces no Redis; multi-pod coord via existing DB" — test "pytest tests/test_redis_absent.py" frozen@d308afac now: not-run
```

---

## `ev list`

**Synopsis:** inventory the ledger — one line per recorded decision, sorted by id
(deterministic).

```
ev list
```

**Flags:** none.

**What it does:** reads every tick in the store, sorts by id, and prints one row per
decision: its id, status, and decision text. A tick that fails to parse still lists its id
with `?` for status and `<unparseable>` for the decision (so a corrupt file is never
silently dropped — `ev verify` owns the schema error). An empty ledger says so.

**Exit code:** `0` when the store exists (including when it is empty); `1` when there is no
store.

**Output (stdout / stderr):**

- per tick (stdout, one row each, sorted by id): `<id>\t<status>\t<decision>` — `<decision>`
  is quoted (`{:?}`); e.g. `638c47b0c9dd\tlive\t"restore-safety counter DB-backed; reject Redis"`
- empty ledger (stdout): `no decisions yet`
- no store (stderr): `error: no .evolving/ store here — run \`ev init\` first`

**Example:**

```sh
ev list
# → 638c47b0c9dd	live	"restore-safety counter DB-backed; reject Redis"
# → e2b337f53a1f	live	"freeze the retrieval schema for v2"
```

---

## `ev log`

**Synopsis:** walk the decision lineage from `HEAD` back to genesis, newest first.

```
ev log
```

**Flags:** none.

**What it does:** reads `HEAD`, then follows each tick's `parent_id` back to genesis,
printing the same row shape as `ev list` for each tick in the chain (newest first). It is the
lineage view — only the HEAD ancestry, not every tick in the store. A content-addressed chain
cannot cycle, but a cycle guard stops the walk if one ever appears, and a `parent_id` that
does not resolve emits a broken-lineage warning and stops. An empty ledger (no HEAD) says so.

**Exit code:** `0` when the store exists (including when it is empty); `1` when there is no
store, or when a tick in the lineage cannot be read.

**Output (stdout / stderr):**

- per tick (stdout, one row each, newest first): `<id>\t<status>\t<decision>` — same shape as
  `ev list`, `<decision>` quoted (`{:?}`).
- empty ledger (stdout): `no decisions yet`
- broken lineage (stderr): `warning: <id> not found (broken lineage)` (the walk stops)
- read error (stderr): `error: reading <id>: <io error>`
- no store (stderr): `error: no .evolving/ store here — run \`ev init\` first`

**Example:**

```sh
ev log
# → 638c47b0c9dd	live	"restore-safety counter DB-backed; reject Redis"
# → e2b337f53a1f	live	"freeze the retrieval schema for v2"
```

---

## Coming (see the project README Status)

The `ev check` liveness evaluator is still landing toward `0.1.0` and is **not** shipped in
`0.0.1`. See the **Status** section of the [project README](../README.md).
