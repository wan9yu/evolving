# Migrating an existing decision history into `ev`

You do not start from an empty ledger. A team usually already has its decisions written down
somewhere — a chat-room or git log, a `RESOLVED` / `FLAG` doc, a numbered decisions document, an
escalation log — and a pile of tests that already guard those decisions. `ev migrate` brings that
history into the ledger and adopts those tests, without re-typing anything.

The **primary, format-neutral intake** is the **Canonical Decision Intake Contract**. An adopter
with a bespoke history writes a small adapter — in any language — that parses *their* format at
the edge and emits canonical JSONL; `ev` never sees the bespoke markdown. The four built-in
extractors (`gitlog`, `to-human`, `decisions-immutable`, `escalation`) are a peripheral
convenience for simple substrates. For the exact flags, exit codes, and printed strings of
`ev migrate`, see [commands.md](commands.md#ev-migrate); for the Tick model the records become,
see [concepts.md](concepts.md).

## The Canonical Decision Intake Contract

One JSON object per line (JSONL), UTF-8, **one decision per line**. Blank lines and `#`-comment
lines are skipped (the same convention as the `--jurisdiction-map` reader). Each line is
independent and idempotent on its dedup key.

```sh
ev migrate --source canonical:decisions.jsonl --blame "Wang Yu"
# → imported 12, skipped 0, re-linked 0, 1 source-only gap(s)
```

### The worked record

A real human ruling, on the wire as one line (shown pretty for readability — on disk it is a
single line):

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

### The closed envelope

Each line's key set is **exactly**:

```
{ kind, decision, observe?, grounds, blame?, authority?, jurisdiction?, source_ref?, provenance? }
```

- `kind` — **required**, the fixed string `"ev-decision-intake"`. An unknown `kind`, or **any**
  unknown envelope key, is a **hard loud failure** that names the line. The wire envelope is
  strict: unlike a stored tick (which tolerates an unknown non-hashed key as forward-compat), an
  external producer's line gets **no** tolerance, so a mis-piped file cannot smuggle a field past
  ingest.
- `decision` — **required**, non-empty. Maps to the hashed `decision` field.
- `observe` — optional (defaults to `""`). Maps to the hashed `observe`. Keep it to a round/source
  token plus human context — **not** raw source markdown — so format never bleeds into the
  content-addressed identity.
- `grounds` — **required**, an array that **may be empty** (the honest zero-grounds capture — a
  falsified-premise open item, or any decision with no structurally-declared reasons). Maps to the
  hashed `grounds`. Each element is re-validated (see below).
- `blame` — optional on the wire; falls back to `--blame`, then `git config user.name`. A record
  with **none** of the three is a **source-only gap** — reported, never imported with an invented
  author.
- `authority` — optional, `{user-ruled, agent-disposable}`. A `user-ruled` ruling surfaces in
  `ev brief`; an open item that is **not** a ruling should **omit** `authority` (the producer that
  knows the difference sets it at the edge).
- `jurisdiction` — optional, `{A, B, C, D}`. `A`/`B` may gate; `C`/`D` are detect-only.
- `source_ref` — optional; see [source_ref](#source_ref-the-dedup-key) below.
- `provenance` — optional, `{imported, agent-proposed, human-now}`; see
  [provenance](#provenance) below. Default on this import path is `imported`.

The contract carries **no `id`, no `parent_id`, no `held_since`, no `status`**. `ev` computes and
stamps those itself at ingest (`parent_id = HEAD`; `held_since` = write-time; `status = "live"`;
`id` = the content-addressed hash). **The producer never supplies identity** — that is the whole
trust boundary in one sentence. Because the producer cannot supply lineage, it can never forge it.

### `source_ref` — the dedup key

`source_ref` is an **opaque, producer-supplied source identity** `ev` never interprets: a
non-empty **string** (an issue ref, a commit, a round token), or a non-empty structured
**object** (JSON). `ev` derives exactly **one** thing from it — a stable dedup/reconcile key (the
string verbatim, or the deterministic sorted-compact JSON of an object) — and compares only that
key, never the contents. It is taken verbatim; `ev` does not re-sniff `observe` for a token when
`source_ref` is present. It is the adopter's concept (a "round", a ticket, a sprint) carried
opaquely; `ev` has no notion of "rounds". Keep your source identity stable and re-imports stay
idempotent.

### `provenance`

`provenance` records **how** a decision entered the ledger, from the closed vocabulary
`{imported, agent-proposed, human-now}`. On the migrate / canonical-intake path a record that
declares none is stamped **`imported`** (history); an explicit value wins (a live runner may emit
`agent-proposed` for a machine draft awaiting a human, or `human-now` for a human ruling captured
live). Fresh authorship can never reach this path: `ev decide` / `ev guard` always stamp
`human-now`, so a forbidden op can never be laundered as `imported`. The only effect of
`imported` is to downgrade the R5 lexical forbidden-op lint to a non-gating warning on faithfully-
transcribed text — **every** hard refusal stays hard (see the provenance partition in
[concepts.md](concepts.md)).

### What `ev` re-validates at ingest (the trust boundary)

The producer supplies **structure**; `ev` re-validates it. This is what keeps the trust boundary
in `ev` no matter what the adapter does:

- **Every `grounds[]` / `check` is re-parsed through `ev`'s own read-path validators** — the same
  ones that guard an on-disk tick. The `grounds` / `check` sub-shape is byte-identical to the
  on-disk one, so there is exactly **one** grounds grammar in the system: claim non-empty;
  `supports ∈ {chosen, rejected:<non-empty>}`; a Test check needs a 40-hex `verified_at_sha` and
  full liveness. A malformed ground is rejected at the door.
- **`ev` computes `id` and `parent_id` itself** — the producer cannot forge identity or lineage.
- **`blame` is required or reported as a source-only gap** — never invented.
- **The ingest-boundary gates** below run at the door, not only at a later `ev verify`.

The honest caveat, stated plainly: **`ev` validates that grounds are well-formed, never that they
are faithful to the adopter's source.** A buggy or hostile adapter that mis-parses prose into
structurally-valid-but-wrong grounds is a **producer bug `ev` cannot catch** — the honest-capture
law protects against `ev` *synthesizing* grounds, not against an edge adapter *fabricating* them.
Format at the edge means the trust boundary (identity, the refusals) stays in `ev`; faithfulness
of the parse stays with the adapter you own.

### Ingest-boundary gates

The same refusals `ev verify` enforces at rest are applied at the door, so a malformed record
never lands:

- a **`C` / `D` (detect-only)** decision may carry **no** runnable Test check
  (`source <key>: a <C|D> jurisdiction (detect-only) decision cannot carry a runnable test check`);
- a **harvested check** (a Test with no counter-test) is allowed **only** for
  `provenance=imported` — a fresh `agent-proposed` Test binding must carry a counter-test **and**
  full liveness, exactly like `ev decide` / `ev guard`
  (`source <key>: a harvested test check (no counter-test) is allowed only for imported history, not <provenance>`);
- **jurisdiction precedence:** an inline `jurisdiction` on a canonical record **wins** over
  `--jurisdiction-map`; the map fills only a record that declares **none**; a record declaring a
  **different** bucket than the map is a hard error
  (`source <key>: inline jurisdiction <inline> conflicts with the --jurisdiction-map entry <mapped>`).

### Idempotency, the chain, and reconcile

A migrate sorts records by their dedup key, computes the content-addressed id each *would* take,
and **skips any key already in the store** — so running it twice writes nothing the second time.
The chain is **kept**: a back-dated mid-chain insert that re-parents an existing tick is reported
as **re-linked**, never rewritten. Every record funnels through the **same** single hashing path
as `ev decide` (one `compute_id`, one write, one R3 lint) — there is no second hashing path.

```sh
ev migrate --source canonical:decisions.jsonl --blame "Wang Yu"
# → imported 0, skipped 12, re-linked 0, 0 source-only gap(s)   (idempotent on the dedup key)
```

**Reconcile** joins a source against the store and reports the **capture gap** — a ruling the
source has that the ledger never captured — without importing:

```sh
ev migrate --reconcile --against canonical:decisions.jsonl
# → reconcile: in-both 5, source-only 3 (the capture gap), store-only 1, un-keyable 0
```

### Harvesting a test you already have (`--bind-check`)

`ev migrate --bind-check <selector>` adopts an existing test as a **harvested** check — a real
test with full liveness but **no counter-test**, so its falsifiability is not yet proven:

```sh
ev migrate --bind-check "pytest tests/test_redis_absent.py" \
  --on-platform linux-ci --triggered-by pyproject.toml --surface pyproject-deps
```

`ev check` evaluates a harvested binding exactly like any other (a passing harvested test reads
`green`, a failing one `red`) but tags the row `harvested — falsifiability not proven; …` and
prints a `harvested-unproven: N of M …` debt line. The way out is `ev guard`: add a
`--counter-test` and the binding becomes a proven, authored check (a new child tick, since the
check is hashed). A canonical record may also carry a harvested check inline — but only when its
`provenance` is `imported` (see the ingest gates above).

### No invented authors

A source record with no author of its own and no `--blame` fallback is **not** imported. It is
surfaced as a **source-only gap** (R5 stays intact — an author is never fabricated). Supply
`--blame "<name>"` as the fallback author for un-attributed records, or fix the source.

## Writing an adapter

The adopter owns the format. Your bespoke decision history is an **intermediate artifact** —
scaffolding you built because `ev` did not exist yet. `ev` core does not grow to absorb it;
instead you write a small adapter (any language) that parses your format and emits **one canonical
line per real decision**, then pipe that JSONL into `ev migrate --source canonical:<path>`.

The rules of a good adapter:

- **One line per real decision.** A decision your source actually settled becomes one
  `ev-decision-intake` line.
- **Declare grounds structurally — never NLP prose into a ground.** A road becomes a
  `rejected:<option>` ground only if your source declares it as one; if a block has no structured
  reason, emit `"grounds": []` (the honest zero-grounds capture). `ev` re-validates the structure
  but cannot check that your parse was faithful — keep the adapter honest.
- **Keep `observe` to a token + human context**, not raw markdown, so format never bleeds into the
  hashed identity.
- **Set `authority` only on a real ruling**; omit it on an open item so the open item never
  surfaces in `ev brief` as if it were settled.
- **Use a stable `source_ref`** (your own work-unit id) so re-imports dedup idempotently.
- **Never set `id` / `parent_id`** — those keys are not in the envelope; `ev` owns identity.

**Same contract, two producers.** The exact JSONL a one-shot adapter emits is what a future live
agent-runner emits as a native side-effect of doing work — no migration, no markdown. A runner
appends one canonical line when it (or the human it serves) settles a decision, with
`provenance = "agent-proposed"` for a machine draft awaiting a human (authority omitted) or
`provenance = "human-now"` for a human ruling captured live. Because the contract carries no
`id` / `parent_id`, the runner is stateless about ledger position: it proposes content, `ev`
assigns identity and lineage. Adopting the canonical contract for a one-time migration also lays
the runner integration — it is the same file format and the same ingest code.

## The built-in convenience extractors

For **simple substrates** that do not warrant a custom adapter, four built-in extractors parse a
source's text into records directly. They are the peripheral path: `ev` never widens them to
swallow one adopter's grammar — that is what the canonical contract is for.

```sh
ev migrate \
  --source gitlog:chat-room.md \
  --source decisions-immutable:DECISIONS.md \
  --blame "Wang Yu"          # fallback author for any un-attributed record
```

- **`gitlog`** — a chat-room / git log; each `## R<N> …` header is one decision, keyed by its
  `R<N>` / `#<n>` round token.
- **`to-human`** — the `RESOLVED` / `FLAG` markdown blocks; a `### RESOLVED <key>: <decision>` is a
  user-ruled decision, a `### FLAG` an open one — both captured.
- **`decisions-immutable`** — a document split on numbered `## N.` / `## §N` sections, one decision
  per section, keyed `§N`.
- **`escalation`** — the *same* `RESOLVED` / `FLAG` reader as `to-human`, path-parameterized (no
  layout of its own).

All four parse **structured rulings + structured rejected-roads only** — a road becomes a ground
iff the source declares it with an explicit `rejected: <option>: <why>` (or `reject …`) token. A
free-text prose reason is **never** mined into a ground; a block with no structured road imports as
an honest zero-grounds capture. Each extracted record's key (e.g. `R2289`, `#555`, `§3`) is carried
into the hashed `observe` **and** written to the non-hashed `source_ref`, so the backfill dedups
and reconciles durably — from the record's own payload, never from the events log.

### Tagging imports detect-only as you backfill

`--jurisdiction-map <path>` tags backfilled records by their dedup key, so another team's rulings
land as detect-only (`C` / `D`) rather than untagged-and-gateable. The map is a plain text file,
one `<source_key> <bucket>` pair per line (`#` comments and blank lines skipped; bucket ∈
`{A, B, C, D}`):

```sh
cat gateway.map
# # source_ref -> bucket
# #1194 C
# R2289 C

ev migrate --source escalation:escalation.md --jurisdiction-map gateway.map --blame "Wang Yu"
# `#1194` now carries jurisdiction C: surfaced forever (memo), gating never.
```

A record whose key is in the map carries that bucket; a record absent from the map imports
untagged. For a **canonical** record an inline `jurisdiction` wins over the map (a conflict is a
hard error). Because jurisdiction is non-hashed, tagging never moves a tick id, so the backfill
stays idempotent. An out-of-vocabulary or malformed map line is a hard error that names the line.
For the detect-only guarantee in depth, see [concepts.md](concepts.md).
