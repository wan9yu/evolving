# ev 0.2.0 · Plan #1 — The Core Loop (design)

> Package `evolving`, binary `ev`, version 0.2.0. The 0.1.x line was a different tool, honorably retired;
> 0.2.0 shares only the name and the crates.io position. This spec covers **Plan #1: the core closure loop,
> running end-to-end on one machine (macOS) against real coding-agent sessions.** Deeper background (the full
> converged spec and the fleet measurements that shaped it) lives in the maintainer's local notes; this
> document is self-contained for implementation.

## Essence

Agents claim; the loop makes claims pay. A **claim** ("done", "fixed", "verified") enters the ledger with a
typed **evidence** pointer — attached automatically from the session's own exhaust wherever possible. The
engine **verifies pointers deterministically** (existence and match — never goodness, never an LLM judge).
Only the **human closes**: close-with-evidence, hold-in-grey, or declare-dead. "Closed anyway" does not
exist. Nothing gates, nothing blocks — a daily ≤5-minute **pause** is where judgment happens, and a **line**
of boundary snapshots is what accumulates. The design guards *engagement*, not just presence: the known
failure mode of human-in-the-loop review is switching off (automation bias), and every affordance that could
become muscle memory carries a visible honesty mark instead of a silent pass.

## Scope

**In (Plan #1):** the ledger; 15 verbs; 2 deterministic verifiers; macOS Claude Code hook adapter
(session markers + brief injection); exhaust claims from real sessions (sweep as the primary path); the pause
(line-oriented); the brief; `ev line` (terminal + `--json`); the work indicator; basic doctor; goldens.

**Out (deferred, with reasons):** `ev rep` + the machine-global hooklog + the UserPromptSubmit hook (the
human-capability indicator ships with them — declaring it without its evidence type would be ceremony);
`ev line --html` + publishing (worth shipping when there is fleet data worth showing); fleet enrollment
(gated on two probes and a claim-grain measurement, below); external-ledger enrollment mode (needed by
public repos that forbid process references in-tree); the agents-md emitter; graduation detection; `--redact`;
`doctor --git`; LLM drafting of claim labels (rejected at v0; pre-registered trigger: label-legibility voted
"n" on two consecutive boundaries).

## Pre-registered success criteria (written before building)

1. A real Claude Code session on this machine files exhaust claims **automatically** (hook + sweep), including
   the sessions that build ev itself — the tool eats itself from day one.
2. **The catch:** ≥1 bare claim is caught open at a pause, bounced with a demand, and later closed with a real
   pointer. A self-evidencing exhaust commit-claim does not count; the catch must involve a bare or
   independently-evidenced claim.
3. The daily pause receipt reads **≤5 minutes** (median across the dogfood week).
4. `ev line` renders the work indicator from ≥1 boundary snapshot, every closed cell traceable to its proof.
5. `ev doctor` clean after the dogfood week; `line --json --stable` goldens byte-stable.

## Architecture

One Rust crate, seven modules, each one job:

| module | job |
|---|---|
| `ledger` | envelope, writer identity, atomic batch append, scan (torn-tail tolerant, ULID dedupe, sort) |
| `state` | the fold: event log → derived state (claim/thought/indicator machines, grey/starvation, snapshots, views) |
| `verify` | V1 commit-resolvable, V2 exists→hash→pass-line; four statuses + the `self_evident` fact |
| `exhaust` | git-window session discovery; one claim per session/chunk; the label rule |
| `hooks` | install/uninstall; SessionStart (brief + sweep) and SessionEnd (marker only) handlers |
| `pause` | the ritual: queue views, screens, receipts |
| `render` | terminal line + brief text + `--json` (stable ordering for goldens) |

Dependencies: `clap`, `serde`, `serde_json`, `ulid`, `sha2`, `fs4`, `time`. Git via subprocess (no git
library). No TUI crate — the pause is a line-oriented prompt loop.

## Ledger

- Layout: `.evolving/` at repo root — `version` ("2") · `config.toml` (non-historical prefs) ·
  `ledger/<writer-id>.jsonl` (one append-only file **per writer**; writer id = hostname-slug + 4 hex, kept in
  uncommitted `local/writer.toml`) · `artifacts/` (committed evidence archives) · `local/`+`cache/` gitignored
  via a committed `.evolving/.gitignore`. `ev init` adds `merge=union` for `ledger/*.jsonl` to
  `.gitattributes` (backstop; the primary merge guarantee is that no two machines write the same file) and
  registers the repo in `~/.config/evolving/repos`.
- Envelope: `{v, id, ts, writer, seq, actor{kind: human|agent|engine, id?, via?}, type, body}`. Ids are
  type-prefixed ULIDs; the CLI accepts unique prefixes and prints the shortest unique form.
- Event types (the full table is frozen in code from day one; two are unused until Plan #2): `thought` ·
  `pull` · `promote` · `claim` · `evidence` · `verify` · `close` · `hold` · `renew` · `prune` · `demand` ·
  `indicator` · `retire` · `repwindow`* · `repclose`* · `snapshot` · `pause` · `cadence` · `session`
  (*reserved for Plan #2).
- Write primitive: **one** — a multi-event batch serialized to a single buffer, one `O_APPEND` write + fsync;
  a killed process can never leave dangling intra-batch references. `flock` on `local/writer.toml` guards the
  seq counter. Reads skip a torn trailing line with a warning; the writer truncates a provably partial tail
  before its next append.
- Fold: no database. Every invocation scans `ledger/*.jsonl`, dedupes by ULID, sorts by `(ts, writer, seq)`,
  folds in two passes (index, then transitions). Derived claim states: `open{bare → evidenced → verified}` ·
  `grey` · `closed` · `dead`, plus derived `expired-bare` (a bare claim past its second boundary — countable,
  revivable by evidence) and the `standing` flag (for the tool's own enrolled features). Grey = explicit hold
  or starvation (no events across ≥2 boundaries); three grey boundaries force the renew / hold-for-help /
  prune fork — in presentation only, never automatically.
- Snapshots: **counted-set semantics** — a boundary snapshot counts matching events not present in any prior
  snapshot's counted set, so late-syncing events land in the next boundary instead of vanishing. Snapshots are
  immutable; the line renders snapshots only.
- Indicators are ledger events (declare/retire with `supersedes` lineage; ceiling of 4 enforced at declare).

## Verbs (15)

`init` · `think` · `claim` (`--evidence <ref>`, `--by agent`, `--round` sugar; `source_ref` is the idempotency
key; long interactive sessions chunk at 10 commits / 4 hours) · `evidence <claim-id> <ref>` (attach; the
demand-answer verb; agents permitted, creation-only) · `close <id>` (requires evidence or an explicit exit:
`--dead --reason <text>`; a bare close is refused with the sting text) · `hold <id> --reason` · `demand <id>`
(human-only bounce; the brief leads with returned demands) · `verify [<id>]` · `pause` (`--boundary` on the
weekly snapshot day; `--all` folds every enrolled repo in one sitting) · `brief` (≤2KB, `--json`) · `line`
(terminal; `--json [--stable]`) · `indicator declare/retire` · `hook install/uninstall/session-start/
session-end` · `doctor` (torn lines, dangling refs, duplicate transitions, clock drift) · `exhaust` (plumbing,
callable directly).

Closure verbs (`close`/`hold`/`demand`/`pause`/`indicator`/prune) refuse under the `CLAUDECODE` env var with
an `--i-am-the-human` override — documented as a provenance courtesy, not security. Read verbs and
`hook`/`exhaust` are exempt. Every verb: `--json`, TTY/NO_COLOR-aware, exit 0 done / 1 honest refusal /
2 error. Every state-reading output ends with "as of event `<id>` · ev refreshes when invoked, not in the
background."

## Evidence and verification

- Typed refs (canonical JSON in the ledger; string sugar in the CLI): `commit:` · `test:` · `file:` ·
  `artifact:` · `metric:` · `url:`. `metric:`/`url:` are **recorded-only** in Plan #1 (no verifier, no network
  — ever, in verification).
- Verifiers: **V1** `git rev-parse --verify <sha>^{commit}` in the named repo; **V2** exists → sha256 → named
  pass-line scan, one code path for `test:`/`file:`/`artifact:`; stages report rather than silently
  short-circuit. Statuses: `verified` · `failed` · `unreachable` (not a failure — a pointer that cannot
  resolve *here*) · `recorded`. Verification runs at filing, at the pause, and on `ev verify`; each run
  appends a `verify` event, so disagreeing re-verifications sit beside their history.
- **`self_evident: true`** marks evidence auto-derived by exhaust from the same repo it verifies against.
  Renderers show **⊙** for self-evident and **✓** for independent verification — never the same mark. This is
  the honesty seam against evidence that merely restates the work: a pointer's existence is a fact; whether
  the evidence covers the promise is the human's judgment at the pause. (Field name for the failure this
  guards: reward hacking at the evidence layer — evidence can never self-certify.)
- Display grades: runnable refs (`test:`) render above static refs (`file:`/`url:`) in the queue. The
  "remove-the-fix → RED" counter-proof discipline is documented as recommended practice, not mandated.
- Transcript-region archival is the default at filing: the matched pass-line region (±20 lines) is copied into
  `.evolving/artifacts/` and the ref points there (session transcripts are mutable and eventually deleted;
  refs into them rot by construction). Reads are streamed (never slurp a transcript).

## Hooks, exhaust, sweep

- Hook events: **SessionStart** (inject `ev brief` as context; run the sweep) and **SessionEnd** (append a
  session marker — one write, nothing else; it must survive being killed). Hooks call the `ev` binary; any
  internal error prints nothing and exits 0. `ev hook install` merges idempotently into the Claude Code
  settings file; the `compact` source is excluded from the SessionStart matcher.
- **The sweep is the primary exhaust path**: at every SessionStart and any `ev` invocation, file exhaust for
  any session whose close-marker (or orphaned open-marker past a threshold) is not yet processed. Writer-scoped:
  a machine only sweeps its own session markers.
- Exhaust: discover the session's git window (commits between markers), file **one claim per session/chunk**
  with all shas as evidence (`self_evident`), idempotent on `source_ref`. Label rule: **the commit subject when
  the session carries exactly one commit; otherwise the first content line of the session's closing summary,
  skipping boilerplate** ("Round N complete." and kin); fallback `"session <short-id>: N commits on <branch>"`.
  Zero commits and zero explicit claims → file nothing.
- The brief (≤2KB): returned demands first, then open claims, pinned thoughts, grey list, and — when a pause
  is overdue — one pull line ("pause overdue: N claims wait"). A ten-line stanza for AGENTS.md (pasted
  manually in Plan #1) tells agents how to file claims and answer demands.

## The pause

Line-oriented prompt loop, screens in order: **0** the day's shape → **1** returned demands (the payoff
moment) → **2** the exhaust batch — ⊙ badge, subject lines, files-touched count, **per-item recommended
action and a cost label** ("N boundaries old · referenced by M"), batch-acknowledge worded honestly
("records that work happened; it does not verify assertions"), per-day group-acknowledge in catch-up mode →
**3** bare claims one at a time (demand / attach / hold / dead / carry) — the sting budget lives here →
**4** grey forks and pins → **5** the receipt: duration + one-key "labels legible? y/n". Batch-acknowledge
staying under judgment (not becoming muscle memory) is guarded by the ⊙/✓ split and the legibility receipt —
the pause is an engagement device, not a checkbox.

## Indicators at birth

One: the work line — `closed-with-evidence` vs `expired-bare`, two raw counts, snapshotted at the weekly
boundary. No percentage, no composite, no smoothing. (The human-capability line ships with `ev rep` in
Plan #2, born with a two-boundary observation period.)

## Testing and discipline

TDD with BDD-named tests; goldens from day one (`ev line --json --stable` and `ev brief --json` byte-stable);
the fold is pure and golden-tested; verifiers get fixture repos. Comments carry no version numbers or
process labels. Commit messages and authorship stay clean of tool names. `/simplify` before any tag. The
repo enrolls itself (`ev init` on evolving2) the day the ledger works.

## Parallel measurement (not gating this plan)

On the fleet host, two probes for Plan #2: tee the hook stdin/stdout under the real `agent-runner serve`
invocation (confirm SessionStart/SessionEnd fire headless and hook output does not pollute the stream-json);
confirm the hook PATH resolves the static `ev` binary. Plus one measurement: the would-be claim count at
issue-card grain on the daily fleet repo.

## Design laws (restated for this plan)

Facts, not verdicts (deterministic verification; `self_evident` is a fact) · nothing gates (hooks always exit
0; refusals are product behavior on the human's own verbs) · pause ≤5 minutes · no daemon, and every surface
says so · TTY/NO_COLOR/`--json` sacred · calm, no red, no alarm · append-only, one write primitive · the
beauty budget is spent on the pause and the line · the tool is its own subject within stated limits.
