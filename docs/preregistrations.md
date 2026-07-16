# Pre-registrations and standing commitments

This file is the durable, tracked home of the ev project's load-bearing commitments. Until
2026-07-16 these lived only in gitignored `internal/` letters and an uncommitted ledger — a
drift condition the project's own thesis warns about, found by the 2026-07-16 step-back audit.
The ledger claim ids below are the machine-checkable anchors; this file is the human-readable
mirror. A commitment changed here without a corresponding ledger event is drift.

## The strategic lock

**The bottleneck is verification, not features.** Every release must answer "what did this
make checkable that was not checkable before," not "what can ev now do." This lock ranks all
work.

## Run-16 earn pre-registration (recorded before Run-16 data; ledger: `clm_01KXGHZ5TY62JJKDPQK9T17AMX`)

The question: does `neighborhood-moved` have unique earn — does it call a human back to a
claim the human would not otherwise have re-read, changing the disposition?

- **Outcome 1** (earn > 0, acceptable ack:earn ratio): close the two structural false-green
  classes; add no new screens/counts/colors on the strength of one success.
- **Outcome 2** (earn = 0 with movement opportunities >= 10): DELETE the moved screen from
  `ev pause`; `neighborhood-moved` stops being a human-facing signal; `drift` demotes to a
  neutral `--json` number; `ev ack`'s fate is decided with it; the negative result goes in the
  README, not an internal file.
- **Outcome 3** (lead count = 0 — the pair only reddens at or after refutation): strip every
  early-warning phrasing from README, doctor, release notes. A thing that only says "that spot
  moved" after the fact is a log, not an alarm.
- **Outcome 4** (movement opportunities < 10): under-powered — not falsified, not confirmed.
  The pair's code is frozen: no strengthening, no deletion, until a run accrues opportunities.

**Meta-rule:** these outcomes were written before any Run-16 number was read. A post-hoc
explanation ("the scenario was atypical / the anchors were badly aimed / the human had an off
day") for a negative result is itself the drift this project exists to catch.

**Current state: Outcome 4 fired for Run-16 (movement = 0). The pair is frozen as of 0.2.4.**

## The B2 curtain protocol (for whichever run finally accrues movement >= 10)

1. The human first receives an **unannotated** list of open claims — no cell, no drift shown.
2. The human writes down, durably, which claims the human intends to re-read this round and
   the expected disposition for each.
3. Only then is the moved screen revealed.
4. **earn** = a claim the pre-commitment marked "would not re-read" that the signal pulled
   back, whose final disposition differs from the pre-committed one. A confirm-then-`ack` is
   not earn. The ack:earn ratio is reported even when ugly.

## Run-17 core-stability probes (the Gateway gate)

Purpose: the owner is considering hosting Gateway (a 24/7, ~75-commits/day real repository)
on ev 0.2.4. Before that bet, Run-17 probes whether the ENGINE holds up in a long unattended
run. Pinned falsification conditions, written before any Run-17 data:

1. **Consistency:** `ev doctor` verbatim every round. Falsified by a doctor integrity crash /
   nonzero exit (dangling ref, duplicate close) or a census number contradicting the raw ledger.
2. **Deterministic replay:** the same ledger folds identically every time. Falsified by two
   same-instant `ev brief --json` (or `ev doctor`) runs disagreeing structurally.
3. **Backward compatibility in real use:** Run-17 starts from (or upgrades over) a 0.2.3-written
   trial ledger; old events read back with unchanged semantics. Falsified by any 0.2.3 event
   reading differently under 0.2.4.
4. **No-false-green hunt:** adversarially constructed inputs (the Run-14 D2 method) hunting for
   any anchor that reads green while the thing it cites is gone or changed. Falsified by one
   confirmed example.

**The gate:** all four probes pass → the engine is judged real and the Gateway step proceeds.
Any probe falsified → caught cheaply in dogfood; the Gateway bet does NOT proceed until fixed
and openly re-registered. **This gate is decided by the pinned conditions above, not adjudicated
after the data arrives.**

**Sequencing:** Run-17 probes first, then the Gateway decision. Not the reverse.

## 0.2.5 is decided by data, not intuition

The 0.2.4 instruments (`reading_snapshot` on every disposition; per-round `reading_census`)
exist to answer, from Run-17's ledger: D1 which depth/language axes are actually used; D2
whether a filled reading unlocks dispositions beyond `demand`; D3 whether `demand` correlates
with hitting an empty slot (unreadability) or with weak evidence (honest demand); D4 whether
"ev names the empty slots" actually drives agents to fill them across rounds. 0.2.5's scope
follows those answers.

## Honest adjudication state, as of 2026-07-16 (pre-data)

Recorded so this file cannot itself false-green:

- **Probe 4 already has two constructed counterexamples**, reproduced in minutes on 0.2.4:
  a substring passline that stays `resolves` after the cited line is deleted (matches the
  falsifier as written; ledger: `clm_01KXNKA88H2BQ71MKAQ5QXDBC9`), and a git-invisible-path
  anchor whose drift is structurally zero (a cell-level permanent `still`; ledger:
  `clm_01KXGFYNHF89KCZ2MH51FC2C1V`). Owner adjudication pending: strict no-bet vs
  fix-and-openly-re-register. Either way, the record stands that the gate fired before the bet.
- **The curtain's pre-committed prerequisite did not ship:** the brief exhaust-flood fix
  (ledger: `clm_01KXK2FMFBQD4TPWCRGK7EWVV2`, pledged "前置于 Run 17") is not in 0.2.4. The
  earn curtain is not administrable until it ships or the harness mutes `auto_commit`.
- **Known instrument gaps as specified:** `reading_census` is emitted only by
  `ev pause --boundary`, which no Run-17 round-protocol step invokes (D4 under-powered as
  written); probe 3 names no comparison surface; the blind list in curtain step 1 has no
  adequate surface (text brief caps at 12 claims; `--json` leaks cell/drift).
- **Engine defects found and ledgered by the same audit:** doctor reads "ledger clean" over
  silently skipped corrupt lines (`clm_01KXNKA89H7DNM8GZ4VMQRPFC5`); cross-writer clock skew
  can silently drop events (`clm_01KXNKA89ZBREM8R6G47G85B13`); the human gate is a single
  environment variable (`clm_01KXNKA8AEP5GXW7BDGRNN1HVZ`); `ev think` does not echo the note
  id its own workflow requires (`clm_01KXNKDBVZM7BG3X0GJ7FGAHX1`).
- **All gates so far are self-graded.** No independent adjudicator is named for any gate; one
  pre-committed prerequisite already drifted silently (the flood fix). Naming an adjudication
  mechanism is an open owner decision.

## The endpoint (pending owner ratification)

Proposed, not yet ratified: the Gateway earn adjudication (the first run with movement
opportunities >= 10 on an organically moving repository) is the LAST narrowing. If earn = 0
there under the curtain protocol, Outcome 2 executes, the product frame retires, the negative
publishes, and what remains is the verification methodology and the instrument. No further
smaller hypothesis is re-registered after that point.
