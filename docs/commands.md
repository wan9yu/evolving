# `ev` command reference

The authoritative reference for the `ev` command surface: the write side (`init`, `decide`,
`guard`) and the read side (`check`, `show`, `verify`, `why`, `reopen`, `brief`, `list`,
`log`). The package is named `evolving`; the command is `ev`.

Every command returns a process exit code: **`0`** on success, **`1`** (failure) otherwise.
Throughout, errors are written to **stderr** as `error: <message>`; the per-command
sections quote the real strings.

For the model behind these commands — Ticks, Grounds, Checks, identity, and the refusals —
see [concepts.md](concepts.md).

- [`ev init`](#ev-init)
- [`ev decide`](#ev-decide)
- [`ev guard`](#ev-guard)
- [`ev check`](#ev-check)
- [`ev show`](#ev-show)
- [`ev verify`](#ev-verify)
- [`ev why`](#ev-why)
- [`ev reopen`](#ev-reopen)
- [`ev brief`](#ev-brief)
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
ev decide --from-git <commit> [trailing flags…]
```

The first positional argument is the **decision text** (required, non-empty) — *unless*
`--from-git` is given, in which case the decision text comes from the commit (see
[seeding from a commit](#seeding-from-a-commit-from-git) below). Everything after the source
is a stream of trailing flags parsed **left-to-right** (see the grammar below).

**Flags:**

| Flag | Takes | Required | Effect |
| --- | --- | --- | --- |
| `--from-git` | a commit | no* | Seed the decision text (and blame — the commit author, or a leading `Role:` subject prefix when present — and the subject's `#<n>` / `R<n>` plus body `Refs #<n>` provenance) from a commit instead of the positional argument. *Exactly one of `{positional decision, --from-git}` must be given.* |
| `--authority` | `user-ruled` \| `agent-disposable` | no | A declared (non-hashed) authority tag, human-set, surfaced by `reopen` / `show` / `list` / `brief`. An out-of-vocabulary value is refused. |
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

### Seeding from a commit (`--from-git`)

`--from-git <commit>` seeds the decision **envelope** from a commit rather than the positional
argument — the thinking is already written in the commit, so it is not re-typed:

- the **decision text** is the commit **subject** (`git show -s --format=%s <commit>`);
- the default **blame** is the commit **author** (`%an`) — *unless* the subject starts with a `Role:` prefix from the closed set {Dev, QA, Product, Mac, User} (case-insensitive), in which case the blame is that role; an explicit `--blame` overrides either;
- provenance is appended to `observe`: any `#<n>` / `R<n>` tokens in the commit **subject**, then any `Refs #<n>` lines in the commit **body**.

**Grounds are NEVER inferred from the commit.** The subject is scanned only for `#<n>` / `R<n>`
provenance tokens and a leading `Role:` prefix; the body only for `Refs #<n>` lines — never parsed
for reasons. The chosen reasons and roads-not-taken stay human-authored:
add them by hand with `--assume` / `--reject` (and their bindings) exactly as for a positional
decision. A `--from-git` decision with no `--assume` / `--reject` records just the subject and
the provenance.

Exactly **one** of `{positional decision, --from-git}` is allowed:

- both given → `decide: decision given twice (positional and --from-git)`;
- neither given → `decide: needs a decision (positional) or --from-git`;
- a commit git cannot resolve → `decide: cannot read commit <commit>`.

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
- **A declared authority must be in vocabulary.** An `--authority` value other than
  `user-ruled` or `agent-disposable` → `authority must be user-ruled or agent-disposable`.
- **Exactly one decision source.** Positional decision *and* `--from-git` →
  `decide: decision given twice (positional and --from-git)`; neither →
  `decide: needs a decision (positional) or --from-git`; an unresolvable commit →
  `decide: cannot read commit <commit>`.
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

**Example** — seed the decision text + blame + provenance from a commit, then add a
human-authored ground and a marked authority by hand:

```sh
ev decide --from-git HEAD \
  --authority user-ruled \
  --assume "team still wants this posture" \
  --reject "the alternative: it would lock us in"
# decision text = the commit subject; blame = a leading `Role:` subject prefix when present,
# else the commit author; provenance into observe = the subject's `#<n>` / `R<n>` tokens then
# the body `Refs #<n>` lines; grounds stay hand-authored.
# → recorded <id> (2 ground(s))
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
  [--verified-at-sha <40-hex>] [--blame "<name>"] [--authority <value>]
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
| `--authority` | `user-ruled` \| `agent-disposable` | no | A declared (non-hashed) authority tag set on the child tick, surfaced by `reopen` / `show` / `list` / `brief`. An out-of-vocabulary value is refused. |

**Target resolution:** with one unbound ground, `target` may be omitted. With more than
one, it is required — `more than one unbound ground — name the target (claim or index)`.
A numeric target out of range → `ground index <i> out of range`; a claim that matches no
ground → `no ground with claim "<t>"`; a claim that matches several →
`ambiguous: multiple grounds with claim "<t>"`. With **no** unbound ground and no `target`,
it is refused — `no unbound ground to guard`.

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
- **A declared authority must be in vocabulary.** An `--authority` value other than
  `user-ruled` or `agent-disposable` → `authority must be user-ruled or agent-disposable`.
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

## `ev check`

**Synopsis:** evaluate every live Test-bound ground against its cached receipts and print one
flat verdict per ground — facts, never a score or a rank. Optionally run the bound tests first
(`--run`), and gate the exit code (`--exit-on-red`).

```
ev check [--run] [--platform <p>] [--exit-on-red] [--offline] [--attest <p1,p2,…>]
```

**Flags:**

| Flag | Takes | Required | Effect |
| --- | --- | --- | --- |
| `--run` | — | no | For each live Test-bound ground that declares `--platform`, run its bound ref locally and append one receipt before evaluating. |
| `--platform` | a platform | no | Which declared platform a `--run` satisfies (the receipt's platform). Defaults to `local`. |
| `--exit-on-red` | — | no | Exit `1` if any ground is not green (and not `n/a` / `exempt`). |
| `--offline` | — | no | Use only the cached staleness reference; never resolve it fresh (non-blocking). |
| `--attest` | a comma-list of platforms | no | The platforms **this runner speaks for**. A declared platform **not** in this set becomes a non-gating `exempt` fact instead of `not-run`. Omit `--attest` to attest **all** declared platforms (the default). |

**Per-runner attestation (`--attest`).** A test binding declares the platforms it must be live
on (its `--on-platform` set). A single runner usually speaks for only some of them. `--attest
linux-ci,mac` tells `ev check` that this runner attests `linux-ci` and `mac`: any *other*
declared platform on a binding is reported as **`exempt`** — a co-equal, **non-gating** fact —
rather than counted as a missing **`not-run`**. With `--attest` omitted, every declared
platform is attested (the cross-platform / audit default), so a platform with no receipt is
`not-run`. `exempt`, like `n/a` and `green`, never trips `--exit-on-red`.

**The flat verdict labels** (one per Test-bound ground; non-Test grounds never print):
`green`, `red`, `gray->red`, `not-run`, `stale`, `unproven`, `silently-unbound`, `exempt`. Each is
a fact; none outranks another (`unproven` = `ev check --run` ran the counter-test and it did not
flip — a vacuous check).

**Exit code:** `0` normally; `1` only under `--exit-on-red` when any ground is not green
(`n/a` and `exempt` do not count), or when there is no store / the store cannot be read.

**Output (stdout / stderr):**

- per Test-bound ground (stdout, one row each): `<label>\t<file>\t<claim>\t(<detail>)` —
  `<claim>` is quoted (`{:?}`); `<detail>` is `missing: <platforms>` for `not-run`, the stale
  reason for `stale`, else `ran <ts>` or `no receipt`.
- after the rows, only when `--run` was **not** passed (stdout): a note pointing the reader to run `ev check --run` to execute each counter-test and prove its falsifiability (under `--run` the verdict itself carries it — an `unproven` row — so no note prints)
- no Test-bound grounds (stdout): `no test-bound grounds to check`
- no store (stderr): `error: no .evolving/ store here — run \`ev init\` first`
- store read error (stderr): `error: reading store: <io error>`

**Example** — a mac runner attests only the platforms it speaks for, so a `linux-ci`-only
binding is `exempt` here, not `not-run`:

```sh
ev check --attest mac
# → exempt	<file>	"<claim>"	(no receipt)
# → note: run `ev check --run` to execute each counter-test and prove its falsifiability

ev check --run --platform linux-ci --exit-on-red
```

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
- declared authority, only when the tick carries one (stdout, after the JSON): `authority: <value>`
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
- declared authority, only when present (stdout): `authority: <value>`.
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

## `ev brief`

**Synopsis:** the boot-read — print the **live** decisions whose declared authority is
`user-ruled`, and the roads each of them rejected. A near-zero-cost, **0-network** read (the
store only — no git, no receipts) for a fresh agent to load the decisions it must respect and
the options it must not re-propose. Ordered **most-recent-first** and **capped**, with an
honest remainder footer so nothing is silently hidden.

```
ev brief [--limit N]
```

**Flags:**

| Flag | Value | Required | Meaning |
| --- | --- | --- | --- |
| `--limit` | non-negative integer | no | Cap the number of decisions shown. Overrides the config default `brief_limit` (which itself defaults to `10`). `--limit 0` shows **all** decisions (no cap, no footer). |

**What it does:** reads every tick, keeps the **live**, `authority == "user-ruled"` ones,
orders them **most-recent-first** (by `held_since`, tie-broken by id descending so output is
deterministic), then caps to the effective limit. The effective limit is `--limit N` when
given, else the config `brief_limit` (default `10`); a limit of `0` from either source means
"show all". For each shown decision it prints the decision marked `[user-ruled]`, then one
indented line per road-not-taken (each ground whose `supports` is `rejected:<option>`). When
the cap drops decisions, a remainder footer is printed pointing at `ev list` (see below), so a
capped brief never hides a ruling without saying so. Person re-checks and chosen grounds are
not listed — `brief` is the *what was ruled and what was rejected* view, not the full reopen.
A store with no user-ruled decisions says so. It never touches the network.

**Exit code:** `0` when the store exists (including when there are no user-ruled decisions);
`1` when there is no store.

**Output (stdout / stderr):**

- per user-ruled decision (stdout, most-recent-first): `<decision>  [user-ruled]` (two spaces
  before the tag), then one indented line per rejected road: `  rejected <option>: <claim>`.
- remainder footer (stdout, only when the cap drops decisions): `… <N> more user-ruled
  decision(s) — \`ev list\` for all` — where `<N>` is the number of user-ruled decisions beyond
  the cap. Not printed when nothing is dropped (including `--limit 0`).
- none (stdout): `no user-ruled decisions`
- no store (stderr): `error: no .evolving/ store here — run \`ev init\` first`

**Example:**

```sh
ev brief
# → restore-safety counter DB-backed; reject Redis  [user-ruled]
# →   rejected Redis: a new infra dependency

ev brief --limit 2
# → <newest user-ruled decision>  [user-ruled]
# →   rejected …
# → <second-newest user-ruled decision>  [user-ruled]
# → … 3 more user-ruled decision(s) — `ev list` for all

ev brief --limit 0   # show every user-ruled decision, no cap, no footer
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
  is quoted (`{:?}`); e.g. `638c47b0c9dd\tlive\t"restore-safety counter DB-backed; reject Redis"`.
  When the tick carries a declared authority, the row gains a trailing
  `\tauthority=<value>` field.
- empty ledger (stdout): `no decisions yet`
- no store (stderr): `error: no .evolving/ store here — run \`ev init\` first`

**Example:**

```sh
ev list
# → 638c47b0c9dd	live	"restore-safety counter DB-backed; reject Redis"
# → e2b337f53a1f	live	"freeze the retrieval schema for v2"	authority=user-ruled
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

## Release status

Every command above — including the `ev check` liveness evaluator with per-runner `--attest`
scoping, the `--from-git` decision source, the declared `--authority` tag, and `ev brief` — is
present and documented in the source tree for the **`0.1.0` honest-resurface slice**. For the
gap between the source tree and the **published** crate, see the **Status** section of the
[project README](../README.md).
