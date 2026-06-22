# `ev` command reference

The authoritative reference for the `ev` command surface: the write side (`init`, `decide`,
`guard`, `migrate`, `correct`) and the read side (`check`, `show`, `verify`, `why`, `reopen`,
`brief`, `list`, `log`). The package is named `evolving`; the command is `ev`.

Every command returns a process exit code: **`0`** on success, **`1`** (failure) otherwise.
Throughout, errors are written to **stderr** as `error: <message>`; the per-command
sections quote the real strings.

For the model behind these commands — Ticks, Grounds, Checks, identity, and the refusals —
see [concepts.md](concepts.md).

- [Global flags — output rendering](#global-flags--output-rendering)
- [`ev init`](#ev-init)
- [`ev decide`](#ev-decide)
- [`ev propose`](#ev-propose)
- [`ev ratify`](#ev-ratify)
- [`ev pending`](#ev-pending)
- [`ev guard`](#ev-guard)
- [`ev migrate`](#ev-migrate)
- [`ev correct`](#ev-correct)
- [`ev check`](#ev-check)
- [`ev show`](#ev-show)
- [`ev verify`](#ev-verify)
- [`ev why`](#ev-why)
- [`ev reopen`](#ev-reopen)
- [`ev brief`](#ev-brief)
- [`ev list`](#ev-list)
- [`ev log`](#ev-log)

---

## Global flags — output rendering

Two flags apply to every command (placeable anywhere on the line):

| Flag | Value | Effect |
| --- | --- | --- |
| `--color` | `auto` (default) \| `always` \| `never` | When to render the rich human view (colour, glyphs, the unified line grammar). `auto` colours only a TTY; `always` forces it (e.g. for `\| less -R`); `never` is plain. |
| `--plain` / `-p` | — | Force the plain output — no colour, glyphs, or aligned layout (the same bytes a pipe gets). Wins over `--color=always`. |

**The two-channel contract:** the rich view appears **only** on a colour TTY (or `--color=always`). A
pipe, a redirect, CI, `NO_COLOR`, `--color=never`, and `--plain` all emit the **plain tab-separated
bytes** — so `| grep`, `> file`, and a CI text-scraper get stable, escape-free output. The machine path
(`--json`, `events.jsonl`, `state.json`) is never styled. Glyphs degrade to ASCII under `EV_ASCII`;
truecolor (vs the named-ANSI default, which adapts to the terminal theme) is opt-in via `EV_TRUECOLOR`.

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
| `--jurisdiction` | `A` \| `B` \| `C` \| `D` | no | A declared (non-hashed) jurisdiction tag. `A`/`B` may gate; `C`/`D` are **detect-only** — structurally ungateable (see [concepts.md](concepts.md)). Surfaced by `show` / `list` / `reopen`. An out-of-vocabulary value is refused. |
| `--source-ref` | a key | no | A declared (non-hashed), **opaque source identity** ev never interprets — a non-empty string (e.g. `R2289`, `#555`, an issue ref) carried verbatim. ev derives only a dedup/reconcile key from it; used by `ev migrate` to dedup + reconcile a backfill. Surfaced by `show` / `list` / `reopen`. Non-empty if given. (The canonical intake also accepts a structured object — see [`ev migrate`](#ev-migrate); on this interactive path it is a plain string.) |
| `--observe` | a string | no | Sets the decision's `observe` field (the situation being observed). |
| `--dry-run` | — | no | Assemble + validate the decision and compute its real id, but **write nothing** (no tick, no event, no `HEAD` move) — a safe preview before the immutable append. Prints `would record <id> (<n> ground(s)) — dry run, nothing written`; the id matches what a real `ev decide` then records. |
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
5. `--observe`, `--blame`, `--verified-at-sha`, `--authority`, `--jurisdiction`,
   `--source-ref`, and `--dry-run` are decision-level, not per-ground. (`--dry-run` may sit
   anywhere in the stream; a literal `--dry-run` in value position — e.g. `--observe "--dry-run"`
   — is kept as the value, not read as the flag.)

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
- **Rejected roads carry no check by default; a user-ruled rejected road may carry a falsifiable
  tripwire.** A human re-check (`--revisit`) on a `--reject` ground is **always** refused. A test
  binding (`--assume-test`) on a `--reject` ground is refused *unless* the decision carries
  `--authority user-ruled` (capability A — the tripwire lift), and a `--counter-test` is still
  required (no harvested rejected-road tripwire) →
  `a rejected road can carry a tripwire test only when the decision is --authority user-ruled`
  (or `a road-not-taken (rejected) ground cannot carry a human re-check` for `--revisit`).
  The tripwire binds only a **structural** token; a prose re-walk with no token (e.g. #1194's
  milestone re-assignment) has nothing to bind and stays surface-only — it is **not** caught.
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
- **A declared jurisdiction must be in vocabulary.** A `--jurisdiction` value outside
  `{A, B, C, D}` → `jurisdiction must be one of A, B, C, D (got <v>)`.
- **A source-ref must be non-empty.** An empty `--source-ref` → `--source-ref needs a non-empty value`.
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

## `ev propose`

**Synopsis:** record an **agent proposal** — a decision an agent suggests. It reuses `ev decide`'s
grammar for the decision + its grounds, but is **always `agent-proposed` / `agent-disposable`**,
**unbound**, and **inert** until a human ratifies it. The trust fields can never be flag-set — that is
the whole point of a separate agent door.

```
ev propose "<decision>" [--observe …] [--assume …]… [--reject …]… [--jurisdiction …] [--source-ref …] [--blame <agent-id>] [--json]
ev propose --from-git <commit> [trailing flags…]
```

**What it does:** assembles the decision exactly as `ev decide` does, then **forces**
`provenance = agent-proposed` and `authority = agent-disposable` (no flag overrides them). The proposal
is **inert**: it never gates (LOCK 3 maps any not-green verdict to the non-gating `memo`) and never
reaches [`ev brief`](#ev-brief) (the boot-read excludes `agent-proposed`) until a human ratifies it.

**Blame never reads git.** Because an agent typically runs under the human's git identity, `ev propose`
resolves its author **without** the `git config` fallback: `--blame <id>` if given, else the
`EV_AGENT_ID` environment variable (the runner's declared agent), else the literal `agent`. A proposal's
author is therefore never silently a human.

**Unbound — refused flags.** A check and `authority` attach only when a human ratifies, so `ev propose`
refuses `--assume-test`, `--counter-test`, `--on-platform`, `--triggered-by`, `--surface`,
`--verified-at-sha`, `--revisit`, and `--authority` with
`ev propose records an UNBOUND proposal — <flag> is not allowed here …`.

**Flags:** the decision-level + ground flags of [`ev decide`](#ev-decide) *except* the refused ones
above, plus:

| Flag | Takes | Effect |
| --- | --- | --- |
| `--blame` | an agent id | The proposing agent's identity. If omitted: `EV_AGENT_ID`, else `agent`. Never git config. |
| `--json` | flag | Emit `{"kind":"ev-proposed","id":…,"provenance":"agent-proposed","authority":"agent-disposable","blame":…}` — the citable envelope a runner records to cite when a human ratifies it. |

**Output (stdout):** `proposed <id> (<n> ground(s)) — agent-proposed, awaiting ratification` (or the
`--json` envelope). **Exit:** `0` ok · `1` on a write/validation failure · refused flags fail with the
message above.

---

## `ev ratify`

**Synopsis:** a human ratifies an agent proposal — the **only** bridge from `agent-proposed` to a
user-ruled ruling.

```
ev ratify <proposal-id> --blame <human>
```

**What it does:** mints a **child** tick that copies the proposal's **hashed payload** (decision /
observe / grounds) verbatim, flips `provenance → human-now` and `authority → user-ruled`, and attaches
the `ratifies:<proposal-id>` edge. The proposal itself is **never rewritten** — it stays immutable,
thereafter shown "ratified by `<child>`". Same mint-a-child mechanics as [`ev correct`](#ev-correct);
the child's id is content-addressed over the copied payload, so the proposal and its ratified child are
recognizably the same decision (and the proposal collapses under the child in `brief` / `list` /
`pending`).

**`--blame` is REQUIRED and never auto-filled.** Ratification is the one op where a `git config` fallback
would forge a human, so the ratifying human must be named explicitly.

| Flag | Takes | Required | Effect |
| --- | --- | --- | --- |
| `--blame` | a human id | **yes** | The ratifying human. Never auto-filled from git. |

**Refusals:** ratifying a tick that is not `agent-proposed` →
`ev ratify only ratifies an agent proposal; tick <id> is <provenance> (nothing to ratify)`. A missing
`--blame` is refused by the arg parser (exit `2`).

**Output (stdout):** `ratified <proposal-id> → <child-id> (now user-ruled, human-now)`. **Exit:** `0` ok ·
`1` on a write failure / no such tick / not-a-proposal.

---

## `ev pending`

**Synopsis:** list the agent proposals awaiting ratification. A **pull-only** view — a query a human
runs, **never** a notifier (no push, no unread, no badge).

```
ev pending
```

**What it does:** shows every live `agent-proposed` decision that has **not** yet been ratified. (A
ratified proposal collapses under its user-ruled child, so it drops out automatically.) Decision-led,
newest first; the `○` provenance glyph leads each row on a TTY, with a `… awaiting ratification —
ev ratify <id> --blame <you>` footer; a pipe gets today's tab-separated bytes (id, status, decision,
blame). Empty → `no proposals awaiting ratification`.

*Sunset:* aging long-stale proposals out of the default view (to an `--all`) is a stated future
refinement; for now every un-ratified proposal is shown.

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
| `--authority` | `user-ruled` \| `agent-disposable` | no | A declared (non-hashed) authority tag set on the child tick, surfaced by `reopen` / `show` / `list` / `brief`. **Required (`user-ruled`) when binding a test to a rejected road** (capability A — the tripwire must be user-ruled). An out-of-vocabulary value is refused. |

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
- **Rejected roads carry no test by default; a user-ruled rejected road may carry a tripwire.**
  Binding a test to a `--reject` ground is refused *unless* the child carries `--authority
  user-ruled` (capability A); the counter-test stays required (no harvested rejected-road tripwire) →
  `a rejected road can carry a tripwire test only when guarded with --authority user-ruled`.
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

## `ev migrate`

**Synopsis:** ingest an *existing* decision history into the ledger from one or more sources —
idempotently. The **primary, format-neutral intake** is the **Canonical Decision Intake
Contract** (`--source canonical:<path.jsonl>`): a producer-owned adapter (or a live runner)
emits one canonical JSON line per decision, and `ev` re-validates every line through its own
read-path validators on the way in. Four built-in extractors (`gitlog`, `to-human`,
`decisions-immutable`, `escalation`) are a **peripheral convenience** for simple substrates,
HARVESTING the rulings and structured roads-not-taken those sources already record. `ev migrate`
also reconciles a source against the store (the capture-gap report), and harvests an existing
test as a check shape (`--bind-check`). For the format, the trust boundary, and writing an
adapter, see [migrating.md](migrating.md).

```
ev migrate --source canonical:<path.jsonl> [--source …] [--jurisdiction-map <path>] [--dry-run] [--blame <fallback>]
ev migrate --source <kind>:<path> [--source …] [--jurisdiction-map <path>] [--dry-run] [--blame <fallback>]
ev migrate --reconcile --against <kind>:<path>
ev migrate --bind-check <selector> --on-platform <p> --triggered-by <t> --surface <s> [--verified-at-sha <40-hex>]
```

**Flags:**

| Flag | Takes | Required | Effect |
| --- | --- | --- | --- |
| `--source` | `<kind>:<path>` | for a backfill | A source to import. `<kind>` ∈ `{canonical, gitlog, to-human, decisions-immutable, escalation}` — `canonical` is the primary intake (JSONL); the other four are convenience extractors. `<path>` is read from disk. Repeatable; sources import in deterministic `source_key` order. |
| `--jurisdiction-map` | a `<path>` | no | A `source_key → A/B/C/D` bucket map (see below). It is **how an imported decision gets its jurisdiction**: a record whose `source_key` is in the map carries that bucket; a record **absent** from the map imports **untagged**. Purely additive — omitting it imports every record untagged. |
| `--dry-run` | — | no | Parse + report what **would** import; write no tick. |
| `--blame` | a name | no | Fallback author for any source record carrying none. **R5 stays intact** — a record with neither its own author nor this fallback is *not* imported; it is reported as a source-only gap (an author is never invented). |
| `--reconcile` | — | no | Reconcile mode: join `--against` against the store and report the buckets instead of importing. |
| `--against` | `<kind>:<path>` | with `--reconcile` | The source to reconcile against. |
| `--bind-check` | a selector | no | Harvest an existing test as a bound check **shape** (counter-test absent, full liveness still required) and print it. Does not write a tick by itself. |
| `--on-platform` / `--triggered-by` / `--surface` | a value (repeatable) | with `--bind-check` (≥1 each) | The liveness the harvested check declares. A harvest with an empty set is refused (no half-harvest). |
| `--verified-at-sha` | 40 lowercase hex | no | The sha the `--bind-check` harvest was verified at; defaults to `git rev-parse HEAD`. |

### `canonical:` — the Canonical Decision Intake Contract (the primary intake)

`--source canonical:<path.jsonl>` reads the **Canonical Decision Intake Contract**: one JSON
object per line (JSONL); blank lines and `#`-comment lines are skipped. Each line is independent
and idempotent on its dedup key. This is the format-neutral seam both a legacy adapter and a
future live runner emit. The full spec — the closed envelope, the trust boundary, the ingest
gates, and writing an adapter — is in [migrating.md](migrating.md); the essentials:

- **The closed envelope.** Each line's key set is exactly
  `{kind, decision, observe?, grounds, blame?, authority?, jurisdiction?, source_ref?, provenance?}`.
  `kind` MUST be the fixed string `"ev-decision-intake"`. An unknown `kind`, or **any** unknown
  envelope key, is a **hard loud failure** — the wire envelope is strict and does **not** get the
  on-disk forward-compat tolerance, so a mis-piped file cannot smuggle a field past ingest.
- **`ev` owns identity.** The contract carries **no `id`, `parent_id`, `held_since`, or
  `status`** — `ev` computes/stamps those (`parent_id = HEAD`; `held_since` = write-time;
  `status = "live"`; `id =` the content-addressed hash). The producer never supplies identity;
  that is the whole trust boundary.
- **`ev` re-validates every ground.** The `grounds[]` / `check` sub-shape is byte-identical to
  the on-disk one, and every element is re-parsed through `ev`'s own read-path validators at
  ingest (claim non-empty; `supports ∈ {chosen, rejected:<non-empty>}`; full check shape). A
  malformed ground is rejected at the door.
- **`source_ref` is opaque.** Taken verbatim (a string) or carried whole (an object); `ev`
  derives only a dedup key from it and never re-sniffs `observe` for a token when `source_ref`
  is present.
- **`provenance` defaults to `imported`.** On this import path a record that declares no
  `provenance` is stamped `imported`; an explicit value (`agent-proposed` / `human-now`) wins.

A worked record (one JSONL line, shown pretty):

```json
{
  "kind": "ev-decision-intake",
  "decision": "rate-limit lives at the edge proxy",
  "observe": "round R1043",
  "grounds": [
    { "claim": "the edge sees every request first", "supports": "chosen" },
    { "claim": "the app tier double-counts", "supports": "rejected:app-tier" }
  ],
  "blame": "Wang Yu",
  "authority": "user-ruled",
  "jurisdiction": "C",
  "source_ref": "R1043",
  "provenance": "imported"
}
```

#### Ingest-boundary gates

The same refusals `ev verify` enforces at rest are applied at the door, so a malformed record
never lands:

- a `C` / `D` (detect-only) decision may carry **no** runnable Test check;
- a **rejected-road Test check** (a tripwire) is allowed **only** when `authority=user-ruled`
  **and** a `counter_test` is present — the same rule as `ev decide` / `ev guard` (capability A),
  so the user-ruled-only rule is structural across every producer;
- a **harvested** check (a Test with no counter-test) is allowed **only** for
  `provenance=imported` — a fresh `agent-proposed` Test binding must carry a counter-test and
  full liveness, exactly like `ev decide` / `ev guard`;
- **jurisdiction precedence:** an inline `jurisdiction` on a canonical record **wins** over
  `--jurisdiction-map`; the map fills only a record that declares none; a record declaring a
  **different** bucket than the map is a hard error.

### The four convenience extractors (rulings + structured roads only — never NLP'd)

The `gitlog` / `to-human` / `decisions-immutable` / `escalation` kinds are pure, format-aware
extractors for **simple substrates** — a peripheral path beside the canonical contract. An
adopter with a bespoke history writes a small adapter that emits canonical JSONL instead (see
[migrating.md](migrating.md)); these built-ins are never widened to swallow one adopter's
grammar. They parse **rulings and *structured* rejected-roads only** — a road becomes a ground
iff the source declares it with an explicit `rejected: <option>: <why>` (or `reject …`) token. A
free-text prose reason is **never** mined into a ground: a block with no structured road yields
a record with **zero grounds** (an honest capture), never a synthesized one.

- **`gitlog`** — a chat-room / git log; each `## R<N> …` header is one decision, keyed by its
  `R<N>` / `#<n>` round token.
- **`to-human`** — the `RESOLVED` / `FLAG` markdown blocks (the authority substrate); a
  `### RESOLVED <key>: <decision>` is a user-ruled decision, a `### FLAG` an open one — both
  captured.
- **`decisions-immutable`** — a document split on numbered `## N.` / `## §N` sections, one
  decision per section, keyed `§N`.
- **`escalation`** — the *same* `RESOLVED` / `FLAG` reader as `to-human`, path-parameterized
  (no layout of its own).

Each extracted record's `source_key` (e.g. `R2289`, `#555`, `§3`) is carried into the hashed
`observe` **and** written to the non-hashed `source_ref`, so the backfill can dedup and
reconcile durably — from the record's own payload, never from the events log.

### The idempotent backfill

A backfill sorts records by `source_key`, then for each computes the content-addressed id it
*would* take and **skips it if that key is already in the store** — so running `ev migrate`
twice writes nothing the second time. The chain is **kept** (`keep-chain`): a back-dated
mid-chain insert that re-parents an existing tick is counted and reported as **re-linked**,
never rewritten. Every record — canonical or extractor-built — is appended through the **same**
single hashing path as `ev decide` (one `compute_id`, one write, one R3 lint).

### Tag discrepancies on re-import (resolved with `ev correct`)

Idempotency is keyed on the durable `source_key`, **not** on the non-hashed tags. So when a
re-imported record carries the same key as a stored tick but its **resolved** non-hashed tags
(`authority` / `jurisdiction` / `provenance`) **differ** from what is already stored, `ev
migrate` does **not** silently skip it. A tick is immutable, so the difference is never applied —
but it is **surfaced loudly**, never dropped, because a silently-discarded corrected authority is
exactly the false-green `ev` exists to refuse (e.g. a ruling first imported as an open item, later
corrected upstream, would otherwise never reach `ev brief`).

Per differing record, one line on **stderr**:

```
discrepancy: source <key> (tick <id>): authority stored=<a> incoming=<b> — NOT applied (ticks are immutable; resolve with `ev correct <id>`)
```

(the `<…>` clause lists every differing tag — `authority` / `jurisdiction` / `provenance` —
each as `stored=<v> incoming=<v>`, `; `-joined), and the run's summary line gains a trailing
`, N discrepancy(ies) — see above` count. This runs under `--dry-run` too (it compares against
the stored tick without writing). The record is still **skipped** (counted in `skipped`), and the
exit code stays `0` — a discrepancy is a *standing report*, not a failure.

The remedy is **[`ev correct`](#ev-correct)**: it appends a corrective child carrying the
corrected tag, so the right value surfaces while the stale tick stays as honest history. This
means **`ev migrate` is no longer a clean "all-zeros = done" signal**: a non-zero discrepancy
count is a *correction pending*, and re-running the import will report it again until you resolve
it with `ev correct`.

On this import path a record's **`provenance` defaults to `imported`** (history) when it
declares none; an explicit value on a canonical record wins. Fresh authorship can never reach
here: `ev decide` / `ev guard` always stamp `human-now`, so a forbidden op can never be
laundered as `imported` (see the provenance partition in [concepts.md](concepts.md)).

### Jurisdiction on import (`--jurisdiction-map <path>`)

An imported decision is **untagged by default** — and an untagged decision can gate. `--jurisdiction-map
<path>` is how a backfilled record gets its A/B/C/D jurisdiction tag, so a `C`/`D` import becomes
**structurally detect-only** rather than landing as a gating record.

The file is a plain text `source_key → bucket` map, one pair per line:

```
# source_ref -> bucket   (a `#` line is a comment; blank lines are skipped)
R2289 C
#1194 C
§3   A
```

- each non-blank, non-`#` line is **exactly two whitespace-separated tokens**: `<source_key> <bucket>`;
- `<source_key>` is the record's durable key (the dedup key derived from its `source_ref`, e.g. an
  `R<N>` / `#<n>` / `§N` token — the same key the extractors carry into `source_ref` and used by
  reconcile);
- `<bucket>` is one of `{A, B, C, D}`. An out-of-vocabulary or malformed line is a **hard error that
  names the offending line** and writes nothing (`jurisdiction-map line "<line>": …`).

A record whose key is in the map imports carrying that jurisdiction; a record **absent** from the map
imports **untagged** (the map is purely additive — an omitted `--jurisdiction-map` tags nothing). For a
**canonical** record, an inline `jurisdiction` **wins** over the map; the map fills only a record that
declares none; a record declaring a **different** bucket than the map is a hard error
(`source <key>: inline jurisdiction <inline> conflicts with the --jurisdiction-map entry <mapped>`).
Because jurisdiction is **non-hashed**, tagging never moves a tick id: a re-run is still idempotent (a
tagged record already in the store is skipped, not rewritten), and the golden vectors do not move.

This is how a detect-only import is made detect-only **structurally**, not by convention. A
`C`/`D`-tagged decision can **never gate**: any not-green verdict on it is mapped to the non-gating
`memo` label (so `ev check --exit-on-red` can never trip on it), and `ev verify` forbids a `C`/`D` tick
from carrying a runnable `Test` check at all. So a gateway record like `#1194` mapped to bucket `C`
imports as a **permanent detect-only MISS** — surfaced forever, gating never (see [concepts.md](concepts.md)).

### Reconcile (`--reconcile --against <src>`)

Reconcile does not import. It reads the source's `source_key`s and the store's durable keys
(the dedup key of each tick's `source_ref`, else the first round token in its hashed `observe`)
and reports four buckets: **in-both**, **source-only** (the *capture gap* — a ruling the source
has that the ledger never captured), **store-only**, and **un-keyable** (store ticks with no
derivable key, counted separately). `--against` accepts the same kinds as `--source`, including
`canonical:<path.jsonl>`.

**The refusals it enforces:**

- **A backfill needs a source.** No `--source` (and not `--reconcile` / `--bind-check`) →
  `ev migrate needs at least one --source <kind>:<path> (or --reconcile / --bind-check)`.
- **Reconcile needs a target.** `--reconcile` without `--against` →
  `--reconcile requires --against <kind>:<path>`.
- **A known source kind.** A `<kind>` outside the five →
  `unknown source kind <k> (expected canonical | gitlog | to-human | decisions-immutable | escalation)`.
- **A `<kind>:<path>` shape.** A `--source` / `--against` missing the colon →
  `--source expects <kind>:<path>, got <spec>`; an unreadable path → `reading <path>: <io error>`.
- **A strict canonical envelope.** A `canonical:` line that is not JSON, not an object, carries
  an unknown envelope key, or whose `kind` is not `ev-decision-intake` is a hard failure naming
  the line, e.g. `canonical line <n>: field outside closed schema: <k>` or
  `canonical line <n>: not an ev-decision-intake record (kind=<v>)`; a malformed ground fails
  through the same read-path validator a stored tick uses.
- **A canonical record needs a durable key.** A `canonical:` record that yields **no** dedup key
  — **no** `source_ref` **and no** round/`#issue` token in `observe` — is rejected at the door:
  `canonical line <n>: a record needs a source_ref (or a round/#issue token in observe) for idempotent re-import`.
  Without a durable key, distinct records would collide on an empty key and re-import every run, so
  the producer must emit a stable `source_ref` (see [migrating.md](migrating.md)).
- **An ingest gate.** A `C` / `D` canonical record carrying a Test check →
  `source <key>: a <C|D> jurisdiction (detect-only) decision cannot carry a runnable test check`;
  a harvested check on a non-`imported` record →
  `source <key>: a harvested test check (no counter-test) is allowed only for imported history, not <provenance>`;
  an inline jurisdiction conflicting with the map →
  `source <key>: inline jurisdiction <inline> conflicts with the --jurisdiction-map entry <mapped>`.
- **No half-harvest (`--bind-check`).** An empty platform / trigger / surface →
  `a harvested binding requires at least one platform, triggered-by, and surface (no half-harvest)`;
  an empty selector → `a harvested binding requires a non-empty test reference`; a malformed
  sha → `verified_at_sha must be 40 lowercase hex: <sha>`.
- **R5 is never bypassed.** A source record with no author and no `--blame` fallback is not
  imported — it is surfaced as a source-only gap (an author is never fabricated).
- **No store.** → `no .evolving/ store here — run \`ev init\` first`.

**Exit code:** `0` on success; `1` on any refusal above.

**Output (stdout / stderr):**

- backfill (stdout): `<(dry-run) >imported N, skipped M, re-linked K, J source-only gap(s)`
  (the `(dry-run) ` prefix only under `--dry-run`); a trailing `, D discrepancy(ies) — see above`
  is appended **only when** `D > 0` tag discrepancies were surfaced (see [above](#tag-discrepancies-on-re-import-resolved-with-ev-correct)).
- per discrepancy (stderr, one line each): `discrepancy: source <key> (tick <id>): <tag stored=… incoming=…; …> — NOT applied (ticks are immutable; resolve with \`ev correct <id>\`)`.
- reconcile (stdout): `reconcile: in-both N, source-only M (the capture gap), store-only K, un-keyable J`.
- `--bind-check` (stdout):
  `harvested check (falsifiability not proven; no counter-test): "<selector>" on [<platforms>] triggered-by [<triggers>] surface [<surfaces>]`.
- failure (stderr): `error: <message>`.

**Example** — ingest a canonical decision-intake stream emitted by an adapter (one line per
decision; `provenance` defaults to `imported`), then re-run to confirm idempotency:

```sh
ev migrate --source canonical:decisions.jsonl --blame "Wang Yu"
# → imported 12, skipped 0, re-linked 0, 1 source-only gap(s)

ev migrate --source canonical:decisions.jsonl --blame "Wang Yu"
# → imported 0, skipped 12, re-linked 0, 0 source-only gap(s)   (idempotent on the dedup key)
```

**Example** — backfill a chat-room log and a decisions doc in one idempotent pass (with a
blame fallback for un-attributed records), then reconcile the authority substrate against the
store to find the capture gap:

```sh
ev migrate \
  --source gitlog:chat-room.md \
  --source decisions-immutable:DECISIONS.md \
  --blame "Wang Yu"
# → imported 7, skipped 0, re-linked 0, 2 source-only gap(s)

ev migrate --reconcile --against to-human:to-human.md
# → reconcile: in-both 5, source-only 3 (the capture gap), store-only 1, un-keyable 0
```

**Example** — import another team's rulings tagged detect-only via a `--jurisdiction-map`, so the
gateway record `#1194` lands as a permanent detect-only MISS (bucket `C`) instead of a gating record:

```sh
cat gateway.map
# # source_ref -> bucket
# #1194 C
# R2289 C

ev migrate --source escalation:escalation.md --jurisdiction-map gateway.map --blame "Wang Yu"
# → imported 2, skipped 0, re-linked 0, 0 source-only gap(s)
# `#1194` now carries jurisdiction C: surfaced forever (memo), gating never; a record absent
# from the map imports untagged.
```

**Example** — harvest an existing test as a check shape (a counter-test-less binding;
falsifiability is *not* proven, so add one later with `ev guard`):

```sh
ev migrate --bind-check "pytest tests/test_redis_absent.py" \
  --on-platform linux-ci --triggered-by pyproject.toml --surface pyproject-deps
# → harvested check (falsifiability not proven; no counter-test): "pytest tests/test_redis_absent.py" …
```

---

## `ev correct`

**Synopsis:** fix a **stale non-hashed tag** (`authority` / `jurisdiction` / `provenance`) on an
existing decision under `ev`'s append-only law. It does **not** rewrite the target tick: it
appends a corrective **child** that copies the target's hashed payload (`decision` / `observe` /
`grounds`) verbatim — so it is recognizably the same decision — carries the corrected tag, and
records an explicit **`corrects:<target-id>` relation-overlay edge** (non-hashed, so the child's `id`
is unaffected). `ev brief` / `ev list` then collapse the corrective lineage to its current state —
reading that edge to supersede the corrected tick precisely — so the corrected child surfaces and the
stale parent stays as honest history (`ev log` still shows the full lineage; `ev show` / `ev reopen`
print the `corrects:` edge so the correction is traceable). The `corrects` edge is `ev`'s **first and
only** relation overlay; the general case-law graph is deliberately not built (a machine-fence test
pins this). A corrective child that carries no `corrects` edge still collapses via content-equality —
the fallback that keeps edge-less corrections working. Limit: two genuinely-independent decisions with byte-identical
`decision`/`observe`/`grounds` would also collapse under that content-equality fallback — the explicit
edge is the precise going-forward signal.

```
ev correct <id> [--authority <v>] [--jurisdiction <v>] [--provenance <v>] [--blame "<name>"]
```

**Positional argument:** the single `id` (required) is the tick whose tag to correct.

**Flags:**

| Flag | Takes | Required | Effect |
| --- | --- | --- | --- |
| `--authority` | `user-ruled` \| `agent-disposable` | at least one tag† | The corrected authority. Out-of-vocabulary is refused. |
| `--jurisdiction` | `A` \| `B` \| `C` \| `D` | at least one tag† | The corrected jurisdiction. Out-of-vocabulary is refused. |
| `--provenance` | `imported` \| `agent-proposed` \| `human-now` | at least one tag† | The corrected provenance. Out-of-vocabulary is refused. |
| `--blame` | a name | no* | The author of the correction. *If omitted, falls back to `git config user.name`; one of the two must resolve to a non-empty name (R5). A correction is a human-authored act.* |

†At least **one** of `--authority` / `--jurisdiction` / `--provenance` must be given.

**What it does:** reads the target tick, resolves the corrected tags (an **override wins**; an
**unspecified tag inherits** the target's), and appends a new child through the **same** hashing
path as `ev decide` — a new `id` at HEAD whose `parent_id` is the prior HEAD. The hashed payload
(`decision` / `observe` / `grounds`) and the `source_ref` are copied from the target verbatim; only
the corrected non-hashed tags differ. The target is **never** rewritten — immutability intact.

It is a **human-authored** act (blame required) and is **unreachable** from `ev migrate` /
canonical intake, so an adapter can never launder a tag through it — the only way a tag changes is
a named human appending a correction.

**The refusals it enforces:**

- **At least one tag.** No `--authority` / `--jurisdiction` / `--provenance` →
  `ev correct needs at least one of --authority / --jurisdiction / --provenance`.
- **No-op.** Every supplied tag already holds that value (nothing actually changes) →
  `tick <id> already carries those tags — nothing to correct`.
- **Unknown id.** An `id` not in the store → `no such tick: <id>`.
- **Vocabulary.** An out-of-vocabulary `--authority` / `--jurisdiction` / `--provenance` is
  refused with the same string `ev decide` uses (e.g. `authority must be user-ruled or
  agent-disposable`).
- **Detect-only structural lock.** Setting a `C` / `D` jurisdiction on a decision that carries a
  runnable Test check → `cannot set jurisdiction <v> on a decision that carries a test check
  (detect-only)`.
- **No store.** → `no .evolving/ store here — run \`ev init\` first`.

**Exit code:** `0` on success; `1` on any refusal above.

**Output (stdout / stderr):**

- success (stdout): `corrected <id> (<n> ground(s))` — `<id>` is the **new child** id.
- failure (stderr): `error: <message>`.

**Example** — a ruling imported with `authority` omitted (so it never reached `ev brief`), then
corrected so it surfaces:

```sh
ev brief
# → no user-ruled decisions          (the ruling was imported as an open item)

ev correct 638c47b0c9dd --authority user-ruled --blame "You"
# → corrected <new-child-id> (1 ground(s))

ev brief
# → <the ruling>  [user-ruled]       (the corrected child now surfaces; the stale parent stays in `ev log`)
```

A migrate **discrepancy** (a re-import whose resolved tags differ from the stored tick — see
[`ev migrate`](#tag-discrepancies-on-re-import-resolved-with-ev-correct)) is resolved exactly this
way.

---

## `ev check`

**Synopsis:** evaluate every live Test-bound ground against its cached receipts and print one
flat verdict per ground — facts, never a score or a rank. Optionally run the bound tests first
(`--run`), and gate the exit code (`--exit-on-red`).

```
ev check [--run] [--platform <p>] [--exit-on-red] [--offline] [--attest <p1,p2,…>]
```

Like [`ev brief`](#ev-brief) / [`ev list`](#ev-list), check first **collapses each corrective
lineage to its current state** (an [`ev correct`](#ev-correct) child supersedes the stale tick it
re-tags) and evaluates only the current live decisions. So a correction that **demotes** a
decision — to `agent-proposed`, or away from `user-ruled` — takes effect at the gate, and a
superseded tick neither prints a duplicate row nor gates. The superseded tick stays reachable
via [`ev log`](#ev-log) / [`ev show`](#ev-show).

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
`green`, `red`, `gray->red`, `not-run`, `stale`, `unproven`, `silently-unbound`, `exempt`,
`memo`. Each is a fact; none outranks another (`unproven` = `ev check --run` ran the
counter-test and it did not flip — a vacuous check). **`memo`** is the non-gating label a
not-green verdict takes in two structural cases: a **`C`/`D`-jurisdiction** (detect-only)
decision, *or* an **`agent-proposed`** tick (capability B — LOCK 3: an agent cannot author a
gating rule; only a named human ratifies one). In both cases the row still prints, naming the
decision, but it can never trip `--exit-on-red` — a structural guarantee (see
[concepts.md](concepts.md)), the sibling of `exempt`. The agent-proposed case also protects the
tripwire: an agent-authored tripwire cannot gate.

**Harvested rows.** A Test binding whose `counter_test` is **absent** (a *harvested* binding
from `ev migrate`) is evaluated exactly as any other — a passing harvested test still reads
`green`, a failing one still `red` — but its row's `<detail>` is prefixed
`harvested — falsifiability not proven; …`, and after the rows a trailing
`harvested-unproven: N of M test bindings have no counter-test (run ev guard to add one)` line
counts the debt. Run `ev guard` to add a counter-test and prove falsifiability.

**Exit code:** `0` normally; `1` only under `--exit-on-red` when any ground is not green
(`n/a`, `exempt`, and `memo` do not count — including any agent-proposed ground, which is mapped
to `memo`), or when there is no store / the store cannot be read.

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
- declared tags, each only when the tick carries one (stdout, after the JSON):
  `authority: <value>`, then `jurisdiction: <value>`, then `source_ref: <value>` (a string
  verbatim, or an object as its deterministic compact JSON), then `corrects: <id>` (the
  relation-overlay edge, when the tick is a correction).
- not found (stderr): `error: no tick with id <id>`
- read error (stderr): `error: reading <id>: <io error>`

**Example:**

```sh
ev show 638c47b0c9dd
```

---

## `ev verify`

**Synopsis:** audit the whole chain and its refusals; or, with `--self-test`, reproduce the
three frozen golden vectors.

```
ev verify [--self-test]
```

**Flags:**

| Flag | Takes | Required | Effect |
| --- | --- | --- | --- |
| `--self-test` | — | no | Recompute the three frozen golden-vector ids and exit. |

**What `ev verify` checks:** every tick parses against the closed *hashed*-schema (R1) and
check shape (R2); a `C`/`D`-jurisdiction (detect-only) tick carries no test check; every
stored `id` equals the hash of its payload and matches its filename (R4 / R6); every
`parent_id` resolves and the lineage is forward-only and acyclic (R6); every tick carries a
non-empty `blame` (R5); and a best-effort lexical lint flags self-evolve subject (R3) and
forbidden-op (R5) language. It reports **all** violations, not just the first. A *tolerated*
unknown top-level (non-hashed) key is not a violation — it is surfaced as a `warning:` on
stderr (see the two-tier schema in [concepts.md](concepts.md)). See concepts.md for the
refusals in depth.

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
✓ harvested: 0cf784b51331 (want 0cf784b51331)
```

The `harvested` vector pins that omitting an absent `counter_test` keeps a harvested binding's
id byte-stable (see [concepts.md](concepts.md)).

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
- declared tags, each only when present (stdout): `authority: <value>`, then
  `jurisdiction: <value>`, then `source_ref: <value>`, then `corrects: <id>` (the
  relation-overlay edge, when the decision is a correction).
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
the options it must not re-propose. Drawn **only from human-ratified rulings** — an
`agent-proposed` decision is excluded, so a proposal an agent recorded never governs a fresh
agent until a human vouches for it. **Load-bearing rulings** — user-ruled decisions that
closed a road via `--reject` — are **pinned above the cap** so recency never buries them; the
rest follow **most-recent-first**, **capped**, with an honest remainder footer so nothing is
silently hidden (and a hidden closed-road ruling is counted, never silent). `--json` emits the
same set as one machine-readable object an agent can parse.

```
ev brief [--limit N] [--json]
```

**Flags:**

| Flag | Value | Required | Meaning |
| --- | --- | --- | --- |
| `--limit` | non-negative integer | no | Cap the number of decisions shown. Overrides the config default `brief_limit` (which itself defaults to `10`). `--limit 0` shows **all** decisions (no cap, no footer). |
| `--json` | flag | no | Emit the frozen `ev-brief` JSON contract (one object) instead of the human text — for an agent to parse. Honors `--limit`; the elision counts make any capped-off ruling visible. |

**What it does:** reads every tick, collapses each **corrective lineage** to its current state
(an [`ev correct`](#ev-correct) child supersedes the stale tick it re-tags — so a ruling whose
`authority` was corrected to `user-ruled` surfaces, and one corrected away from it drops out),
keeps the **live**, `authority == "user-ruled"`, **non-`agent-proposed`** ones (the provenance
exclusion is the ratification line: a `provenance == "agent-proposed"` record never reaches the
boot-read, in either output form, until a human re-authors it), then orders them so that
**load-bearing rulings come first**. A ruling is *load-bearing* iff
any of its grounds closes a road (its `supports` starts with `rejected:`) — those are the
decisions a fresh agent must not re-walk, so they sort ahead of every non-load-bearing ruling
**regardless of recency** and are pinned above the cap. Within each of those two groups the
order is **most-recent-first** (by `held_since`, tie-broken by id descending so output is
deterministic). It then caps to the effective limit. The effective limit is `--limit N` when
given, else the config `brief_limit` (default `10`); a limit of `0` from either source means
"show all". For each shown decision it prints the decision marked `[user-ruled]`, then one
indented line per road-not-taken (each ground whose `supports` is `rejected:<option>`). When
the cap drops decisions, a remainder footer is printed pointing at `ev list` (see below), and
when any of the hidden decisions are themselves load-bearing the footer **counts them** — so a
capped brief never hides a closed-road ruling without saying so. Person re-checks and chosen
grounds are not listed — `brief` is the *what was ruled and what was rejected* view, not the
full reopen. A store with no user-ruled decisions says so. It never touches the network.

**Exit code:** `0` when the store exists (including when there are no user-ruled decisions);
`1` when there is no store.

**Output (stdout / stderr):**

- per user-ruled decision (stdout, load-bearing first then most-recent-first):
  `<decision>  [user-ruled]` (two spaces before the tag), then one indented line per rejected
  road: `  rejected <option>: <claim>`.
- remainder footer (stdout, only when the cap drops decisions): `… <N> more user-ruled
  decision(s)<, M with rejected roads> — \`ev list\` for all` — where `<N>` is the number of
  user-ruled decisions beyond the cap, and the conditional `, <M> with rejected roads` clause
  is appended **only when** `M > 0` of those hidden decisions are load-bearing (carry a
  rejected road); when `M` is `0` the clause is omitted entirely. Not printed when nothing is
  dropped (including `--limit 0`).
- none (stdout): `no user-ruled decisions`
- `--json` (stdout): one object, always valid JSON even when empty (never the `no user-ruled
  decisions` text), on the frozen `ev-brief` contract:
  `{"kind":"ev-brief", "decisions":[{"id", "decision", "load_bearing", "rejected_roads":[{"option",
  "claim"}], "source_ref"?}], "shown", "total", "elided", "elided_load_bearing"}`. Each decision
  carries its **citable `id`**; `source_ref` is present only when the producer supplied one; the
  `elided` / `elided_load_bearing` counts make a capped-off ruling visible (re-pull with a higher
  `--limit` rather than act on a partial view).
- no store (stderr): `error: no .evolving/ store here — run \`ev init\` first`

**Example:**

```sh
ev brief
# → restore-safety counter DB-backed; reject Redis  [user-ruled]
# →   rejected Redis: a new infra dependency

ev brief --limit 2
# → <a load-bearing ruling — pinned above the cap>  [user-ruled]
# →   rejected …
# → <next ruling, load-bearing first then most-recent-first>  [user-ruled]
# → … 3 more user-ruled decision(s), 1 with rejected roads — `ev list` for all

ev brief --limit 0   # show every user-ruled decision, no cap, no footer

ev brief --json
# → {"kind":"ev-brief","decisions":[{"id":"…","decision":"restore-safety counter DB-backed; reject Redis","load_bearing":true,"rejected_roads":[{"option":"Redis","claim":"a new infra dependency"}],"source_ref":"R2289"}],"shown":1,"total":1,"elided":0,"elided_load_bearing":0}
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
decision: its id, status, and decision text. A **corrective lineage** is collapsed to its
**current** state — an [`ev correct`](#ev-correct) child supersedes the stale tick it re-tags, so
only the latest tag of a decision is listed (the full lineage stays in `ev log`). A tick that
fails to parse still lists its id with `?` for status and `<unparseable>` for the decision (so a
corrupt file is never silently dropped — `ev verify` owns the schema error). An empty ledger says
so.

**Exit code:** `0` when the store exists (including when it is empty); `1` when there is no
store.

**Output (stdout / stderr):**

- per tick (stdout, one row each, sorted by id): `<id>\t<status>\t<decision>` — `<decision>`
  is quoted (`{:?}`); e.g. `638c47b0c9dd\tlive\t"restore-safety counter DB-backed; reject Redis"`.
  When the tick carries a declared tag, the row gains a trailing field for each set, in order:
  `\tauthority=<value>`, `\tjurisdiction=<value>`, `\tsource_ref=<value>`.
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
