# ev — design

> How the tool is built: the ledger, the fold, the verifiers, the session hooks, and the pause.
> The [README](../README.md) covers what it is for; this document describes the internals as they
> actually behave.

## The loop, in one paragraph

A **claim** enters the ledger with (or without) typed **evidence** pointers. The engine **verifies
pointers deterministically** — existence and match only, never goodness, never a model, never the
network. Only the **human closes**: with evidence, on hold (grey), or declared dead. Nothing gates;
a claim with no evidence simply waits, visibly, at the next pause.

## Architecture

One crate, each module one job:

| module | job |
|---|---|
| `ledger` | envelope, writer identity, the single atomic batch-append, torn-tolerant scan |
| `state` | the fold: event log → derived state (claim machine, grey, snapshots, views) |
| `verify` | anchor resolution (commit / file / test / artifact), the four statuses, drift |
| `exhaust` | git-window discovery; one claim per session; the label rule |
| `hooks` | session hook install and handlers; the sweep |
| `pause` | the ritual: screens, decisions, receipts |
| `render` | the brief, the line, and their `--json` forms |

plus `cmd` (one thin handler per verb), `main` (dispatch), `lib` (error type, shared helpers).
Git is invoked as a subprocess; there is no git library, no TUI crate, no network client.

## The ledger

- Layout: `.evolving/` at the repo root — `version` (schema `2`) · `config.toml` ·
  `ledger/<writer-id>.jsonl` (one append-only file **per writer**; writer id = hostname slug + 4 hex,
  kept in the uncommitted `local/writer.toml`) · `artifacts/` (committed evidence archives; `artifact:`
  refs resolve here) · `local/` and `cache/` are ignored via a committed `.evolving/.gitignore`.
  `ev init` adds `merge=union` for `ledger/*.jsonl` to `.gitattributes` (a backstop — the primary
  merge guarantee is that no two machines write the same file) and registers the repo in
  `~/.config/evolving/repos`.
- Envelope: `{v, id, ts, writer, seq, actor{kind: human|agent|engine, id?, via?}, type, body}`.
  Ids are type-prefixed ULIDs (`clm_…`, `evd_…`, `vfy_…`); the CLI accepts any unique prefix.
- Event types read by the fold: `claim` · `evidence` · `verify` · `close` · `hold` · `prune` ·
  `demand` · `thought` · `indicator` · `retire` · `snapshot` · `pause` · `session`. The id-prefix
  table also reserves names for planned types; unknown types are ignored on read.
- **One write primitive:** a batch is serialized to a single buffer and lands in one append write +
  fsync, so a killed process can never leave a dangling intra-batch reference. An exclusive flock on
  `local/writer.toml` guards the per-writer `seq` counter. A provably partial trailing line (torn by
  a kill) is truncated before the next append; reads skip any unparseable line with a warning.
- The fold: no database. Every invocation scans `ledger/*.jsonl`, dedupes by id, sorts by
  `(ts, writer, seq)`, and folds. Derived claim states: `bare → evidenced → anchored` (open) ·
  `grey` (an explicit hold; evidence revives it) · `closed` · `dead` · `expired-bare` (a bare claim
  that has survived two boundary pauses — countable, revivable by evidence). A demanded claim that
  later gains evidence surfaces as a **returned demand**. Snapshots are immutable events; the fold
  surfaces their recorded deltas as history.

## Verbs

`init` · `think` (`--pin`) · `claim` (`--evidence <ref>`, `--by agent|human`, `--source-ref` as the
idempotency key, `--kind` to declare what kind of claim this is — e.g. defect, priority) ·
`evidence <claim> <ref>` (the demand-answer verb; agents permitted) ·
`verify [<claim>]` (`--json`; re-check anchors and report drift; each check appends a `verify` event,
so disagreeing re-checks sit beside their history; self-evident evidence — the `commit:` refs exhaust
files about itself — is not re-checked, since it cannot fail; `--full` re-checks it anyway) ·
`close <claim>` (requires evidence, or the explicit exit
`--dead --reason <text>`; a bare close is refused) · `hold <claim> --reason` · `demand <claim>` ·
`pause` (`--boundary` on the snapshot day; `--script` for piped stdin) · `brief` (`--json`; ≤2KB text) ·
`line` (`--json [--stable]`) · `indicator declare|retire` (ceiling of four) ·
`hook install|uninstall|session-start|session-end` · `doctor` · `exhaust --since <ref> --session <id>`
(plumbing).

Closure verbs (`close`, `hold`, `demand`, `pause`, `indicator`) refuse under the `CLAUDECODE`
environment variable unless `--i-am-the-human` is passed — a provenance courtesy, not security.
Exit codes: 0 done · 1 honest refusal · 2 error. State-reading output ends with
"as of event `<id>` · ev refreshes when invoked, not in the background."

## Evidence, resolution, drift

- Typed refs: `commit:<sha>` · `test:<path>[::<pass-line>]` · `file:<path>[::<line>]` ·
  `artifact:<name>[::<pass-line>]` · `metric:<text>` · `url:<text>`. Metric and url are
  **recorded-only** — no verifier, and never any network.
- Anchor resolution: commits resolve via `git rev-parse --verify <sha>^{commit}`; files, tests, and
  artifacts resolve as exists → readable (hashed) → named pass-line found. Statuses: `resolves` ·
  `failed` · `unreachable` (a pointer that cannot resolve *here* — not a failure) · `recorded`.
  **Resolution is a fact about the pointer, never a verdict on the claim** — the status word is
  chosen so a resolve-check cannot be read as "the claim is verified." (Ledgers are append-only;
  events written before this word existed carry `verified` and are normalized on read, never
  rewritten.) Resolution runs when evidence is filed and on `ev verify`.
- **Drift:** every filed anchor records its `base` — the repo state (HEAD sha) it was filed against.
  For path-bearing anchors, the number of commits between base and HEAD touching the cited path is
  reported wherever evidence is read: `ev verify` (text and `--json`), the brief's `--json` evidence
  entries, and the pause's returned-demands screen. A structural fact, measured in world
  movement, not clocks — zero means the cited path is exactly as the anchor saw it; a drifted anchor
  can still resolve while the recommendation it supported has gone stale. The human judges what
  drift means; the engine only counts it.
- **`self_evident: true`** marks evidence auto-derived from the same repo it resolves against (a
  session's own commits). Renderers show **⊙** for self-evident and **✓** for independently filed
  anchors — never the same mark. A pointer's existence is a fact; whether the evidence covers
  the promise is the human's judgment at the pause. Evidence never self-certifies.

## Hooks, exhaust, the sweep

- `ev hook install` merges two hooks into the repo's `.claude/settings.json`, idempotently:
  **SessionStart** (matcher `startup|resume`, which excludes compaction) prints the brief — injected
  as session context — then runs the sweep; **SessionEnd** appends one session marker recording the
  current HEAD. Hooks always exit 0; an internal error prints nothing.
- **The sweep** runs at session start and is writer-scoped: for each of this machine's session
  markers not yet swept, it files the commits in that session's window — from the previous swept
  marker's head (a resolved sha watermark) to this marker's head — as **one claim per session**,
  every commit attached as `self_evident` evidence, idempotent on the session id. A claim orphaned
  bare by a kill between the claim write and the evidence write is repaired on the next pass — the
  evidence it never got is attached to it, never a second claim. Windows never overlap; an empty
  window files nothing.
- The label rule: the commit subject when the window holds exactly one commit; otherwise
  `"session <short-id>: N commits on <branch>"`.

## The pause

A line-oriented prompt loop, screens in order: **0** the day's shape → **1** returned demands (the
payoff moment) → **2** the exhaust batch — ⊙ badge, labels, age (`N boundaries old`), acknowledged
with honest wording (*acknowledging records that work happened; it does not verify the assertions*) →
**3** bare claims one at a time — demand (`d`) / attach (`a <ref>`) / hold (`h`) / dead (`x`) /
carry (`c`) → **4** the grey list → **5** the receipt: duration and a one-key "labels legible? y/n".
A `--boundary` pause first writes the counted-set snapshot (the delta since prior snapshots), then
records the pause itself.

## The line

`ev line` renders the work indicator: **closed-with-evidence** and **expired-bare**, two raw counts —
no percentage, no composite, no smoothing. Snapshot rows carry the boundary history; the "now" line
is the live fold. `--json --stable` normalizes volatile fields and is byte-stable (golden-tested).

## Doctor

`ev doctor` checks the ledger: dangling references (an event pointing at an unknown claim),
duplicate closes, and per-writer clock drift. Clean exits 0; problems print and exit 2.

## Design laws

Facts, not verdicts · nothing gates (hooks exit 0; the only refusals are on the human's own verbs) ·
only the human closes · append-only, one write primitive · no daemon, and every surface says so ·
two raw counts, never a score · calm, plain output.

## Not yet built

The human-capability indicator and its `rep` verb · `line --html` and publishing · fleet and
external-ledger enrollment · automatic transcript-region archival · rendering for custom-declared
indicators · a pause-overdue pull line in the brief · multi-repo `pause --all` · session chunking ·
torn-line reporting in doctor (scan already tolerates and heals torn tails).
