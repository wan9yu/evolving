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
| `verify` | anchor resolution (commit / file / test / artifact), the status, drift, and the `cell` that joins them |
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
  `demand` · `ack` · `thought` · `indicator` · `retire` · `snapshot` · `pause` · `session`. The
  id-prefix table also reserves names for planned types; unknown types are ignored on read.
- Every disposition event — `close` · `hold` · `demand` · `prune` · `ack` — carries `at_verify`: a
  snapshot of what each of the claim's anchors read (status, drift, cell) at the instant of the
  decision. ev writes it and never reads it back; it exists so a later reader can ask whether the
  signal preceded the decision, not to drive any behaviour of ev's own.
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
- **A claim's state is a fact about the LEDGER, not about the world.** It is folded from the last
  status any `ev evidence` or `ev verify` recorded, and it is deliberately not re-derived live: it
  moves when a human runs `ev verify`, and not before. So a file can be anchored and then deleted,
  and the TEXT `ev brief` will still print `✓ the parser is fixed [anchored]` — while `ev brief
  --json` at that same instant reports `"status": "gone"`, `"cell": "file-gone"`, because the JSON
  surfaces annotate (they re-read every anchor there and then). The state word says what the ledger
  was told; the status and the cell say what ev just saw. Deriving state live would add a second
  state-machine site beside `derive_state`, let the two disagree, and leave `ev verify` with nothing
  to do. **`ev verify` and `ev brief --json` are what report the world.**

## Verbs

`init` · `think` (`--pin`) · `claim` (`--evidence <ref>`, `--by agent|human`, `--source-ref` as the
idempotency key, `--kind` to declare what kind of claim this is — e.g. defect, priority) ·
`evidence <claim> <ref>` (the demand-answer verb; agents permitted) ·
`verify [<claim>]` (`--json`; re-check anchors and report drift; each check appends a `verify` event,
so disagreeing re-checks sit beside their history; self-evident evidence — the `commit:` refs exhaust
files about itself — is not re-checked by default, since it is content-addressed and fails only if the
commit is absent from this clone; `--full` re-checks it anyway. By default, ev verify no longer polls
for that absence on old exhaust windows; `ev verify --full` is the path that still does.) ·
`close <claim>` (requires evidence, or the explicit exit
`--dead --reason <text>`; a bare close is refused) · `hold <claim> --reason` · `demand <claim>` ·
`ack <claim>` (human-only: the human looked, and the claim still stands — see below) ·
`pause` (`--boundary` on the snapshot day; `--script` for piped stdin) · `brief` (`--json`; ≤2KB text) ·
`line` (`--json [--stable]`) · `indicator declare|retire` (ceiling of four) ·
`hook install|uninstall|session-start|session-end` · `doctor` (ledger integrity, plus three
never-gating census lines: anchor liveness, ref types in use, and the movement census — see below) ·
`baseline [<sha>]` (record where the ledger began; default HEAD — `ev init` records it, an upgraded
0.2.1 ledger needs it once) · `exhaust --since <ref> --session <id>` (plumbing; `--since ROOT` starts
the window at the baseline, so a repo's pre-existing history is never filed as a session's output).

`ack <claim> --i-am-the-human` records that the human looked at a claim and it still stands. It
completes the disposition set — `close` / `hold` / `demand` / `prune` had no way to record the most
common outcome of a review — and it is what keeps `neighborhood-moved` (below) from becoming a flag
no human can ever clear. It is **not a re-base**: the evidence `base` a claim was filed against stays
pinned forever; `ack` records a second, human-relative reference point (the HEAD looked at), and once
a claim has been acked, drift is counted from that look forward — `last_ack` takes priority over the
filing `base` whenever both exist — see "Evidence, resolution, drift" below.

Closure verbs (`close`, `hold`, `demand`, `ack`, `pause`, `indicator`) refuse under the `CLAUDECODE`
environment variable unless `--i-am-the-human` is passed — a provenance courtesy, not security.
Exit codes: 0 done · 1 honest refusal · 2 error. State-reading output ends with
"as of event `<id>` · ev refreshes when invoked, not in the background."

## Evidence, resolution, drift

- Typed refs: `commit:<sha>` · `test:<path>[::<text on the cited line>]` ·
  `file:<path>[::<text on the cited line>]` · `artifact:<name>[::<text on the cited line>]` ·
  `metric:<text>` · `url:<text>`. Metric and url are
  **recorded-only** — no verifier, and never any network.
- The `::` payload is text to match, **never a line number**: ev anchors by content, so a line number
  would stay green after the code moved. `file:<path>:<N>` and `test:<path>:<N>` are refused at filing
  (0.2.2; before that the `:<N>` was silently folded into the path and the anchor resolved to nothing).
  `::<text>` fails when the cited line changes; a bare `file:<path>` fails only if the path disappears.
- Anchor resolution: commits resolve via `git rev-parse --verify <sha>^{commit}`; files, tests, and
  artifacts resolve as exists → readable → the named text found on some line. The status is a typed
  class (`verify::Status`), so a reader that buckets it cannot silently fold a value it does not know
  into one it does. Statuses: `resolves` · `changed` (the file is there, the cited text is not — the
  line the anchor pointed at moved) · `gone` (the path is absent, or the commit is absent from this
  clone — the container is gone) · `unreachable` (the path exists but ev could not read it — a fact
  about ev's reach, not about the code) · `recorded` (self-asserted; cannot fail by construction).
  **Resolution is a fact about the pointer, never a verdict on the claim** — the status word is
  chosen so a resolve-check cannot be read as "the claim is verified." Resolution runs when evidence
  is filed and on `ev verify`.
- **Attach-time guard (0.2.3):** `ev evidence` / `ev claim --evidence` refuse three anchors that
  cannot carry a signal: an id that is not a claim's; a content anchor (`::<text>`) whose text is
  absent from the target at filing time (an anchor on absent text is born red and stays red
  forever — it carries no signal and never will); and an empty pass-line (`file:<path>::`), which
  matches every line and can never go red. The guard runs once, at filing; it does not touch anchors
  already on the ledger.
- Ledgers are append-only, so the read path carries what older versions wrote and never rewrites it:
  0.1.x's `verified` normalizes to `resolves`, and `failed` — the pre-0.2.3 value that conflated
  `changed`, `gone` and a never-valid anchor behind one word — stays in the ledger as `failed`,
  forever. 0.2.3 never produces it. An unrecognised status also reads as `failed`: ev does not guess.
  A read, however, is a **measurement, not an echo**: where the pointer still parses, the read path
  re-reads the anchor and reports what it finds there and then, so a legacy `failed` is superseded by
  a fresh reading rather than repeated. Where ev cannot re-read the pointer at all (a ref no current
  grammar accepts), `failed` stands and its cell is `legacy` — ev does not guess. The event is never
  rewritten in either case.
- **Drift:** every filed anchor records its `base` — the repo state (HEAD sha) it was filed against.
  For path-bearing anchors, the number of commits between base and HEAD touching the cited path is
  reported wherever evidence is read: `ev verify` (text and `--json`), the brief's `--json` evidence
  entries, and the pause's returned-demands screen. A structural fact, measured in world
  movement, not clocks — zero means the cited path is exactly as the anchor saw it; a drifted anchor
  can still resolve while the recommendation it supported has gone stale. The human judges what
  drift means; the engine only counts it. Once a claim has been `ack`'d, drift is counted from the
  HEAD that `ack` recorded rather than the filing `base` — from the human's **last look**, not from
  filing — so a fresh look resets the count without touching the pinned `base`. The ack is preferred
  only where the count **can be taken against it**: the ledger travels between clones, and an ack
  taken on a branch that was later squash-merged and deleted names a sha that resolves nowhere. There
  the count falls back to the pinned `base` — the original pin, and the more conservative (larger)
  count. Falling back is not a re-base, and it is not dropping the ack. If neither reference resolves,
  ev reports no drift at all rather than reporting zero.
- **`cell`:** the join of `status` and drift-since-the-last-look, derived in exactly one place
  (`Cell::of`) so no second site can drift from it. **Both halves are read at the same instant.**
  Every surface that shows a cell (`ev verify`, `ev brief --json`, the pause, `ev doctor`, the
  `at_verify` snapshot) re-reads the anchor there and then, rather than joining a status the ledger
  recorded at filing time with a drift counted now: `ev verify` is a manual verb, so the recorded
  status can be months old, and the join of an old status with a fresh count describes no world that
  ever existed. The read path re-reads and appends nothing; `ev verify` remains the verb that records. Five values: `still` (drift was measured, and it
  is zero — nothing this anchor can see has moved), `neighborhood-moved` (the cited line stands;
  code moved beside it — the content anchor's blind spot), `anchor-changed` (the cited line itself
  changed), `file-gone` (the container is gone), `legacy` (an UNPARSEABLE pointer from an older
  ledger — ev cannot read it and does not guess). Because the read path re-reads every pointer it
  CAN parse, `legacy` now means exactly one thing, and **`ev verify` does not clear it**: verify
  re-reads anchors, and this is the pointer it cannot read. The way out is to re-file the anchor
  with `ev evidence` under a grammar ev accepts; the old entry stays, because the ledger is
  append-only. `ev verify` prints an `unparseable` line for it rather than dropping it — a silent
  drop in the verb whose job is to report what it read is the false-green ev exists to refuse.
  **No cell is emitted when drift could not be measured** — a
  `commit:` ref, a `recorded` (`metric:`/`url:`) anchor, and an `unreachable` one all carry no cell,
  by the same convention: an absent cell means ev asserts nothing, not that nothing moved. `still`
  is the only value that means zero movement, and it means that only because drift was actually
  counted. `ev verify --json` and `ev brief --json` carry `cell` on every check that has one.
  **ev never reports whether a claim was resolved.** A content anchor sees a changed *line*, not a
  restored *behaviour*, and most caller-visible defects are fixed by adding code beside the buggy
  line — the anchor stays green, and the fix shows up only as `neighborhood-moved`: the ground under
  the claim shifted. `neighborhood-moved` is a prompt to re-read, and nothing more; ev does not
  infer a fix from it.
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
payoff moment) → **1.5** claims whose code moved since the last look (`cell` ∈
`neighborhood-moved` / `anchor-changed` / `file-gone`) — one line of *why*, then `h`old / `d`emand /
enter to skip, and `k` (still stands, i.e. `ack`) **only on `neighborhood-moved`**: an ack clears a
cell by moving the human's reference point, and `Cell::of` does not read drift for a changed or gone
anchor, so no ack can ever clear one. A broken anchor is named as broken and must be re-filed with
`ev evidence` — ev does not offer a key that cannot work → **2** the exhaust batch — ⊙ badge, labels, age (`N boundaries old`), acknowledged
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

It also prints three census lines that never gate and never change the exit code: anchor liveness
(what it would take for each recorded anchor to go red), ref types in use, and the movement
census — a count of **every claim that carries evidence**, open, held or closed, by its most severe
`cell` (`still` / `neighborhood-moved` / `anchor-changed` / `file-gone` / `legacy`), plus
`unmeasured` for a claim ev could place on no cell at all (its anchors are `commit:`/`metric:`/`url:`,
or git could not count against them). The unmeasured claims stay in the denominator and are named:
a census that dropped them would print a smaller total and call the remainder "claims" — the silent
undercount doctor exists to expose. Where claims sit on code that moved, doctor says
**re-read** — never "resolved"; it has no way to know whether the movement was the fix.

## Design laws

Facts, not verdicts · nothing gates (hooks exit 0; the only refusals are on the human's own verbs) ·
only the human closes · append-only, one write primitive · no daemon, and every surface says so ·
two raw counts, never a score · calm, plain output.

## Not yet built

The human-capability indicator and its `rep` verb · `line --html` and publishing · fleet and
external-ledger enrollment · automatic transcript-region archival · rendering for custom-declared
indicators · a pause-overdue pull line in the brief · multi-repo `pause --all` · session chunking ·
torn-line reporting in doctor (scan already tolerates and heals torn tails).
