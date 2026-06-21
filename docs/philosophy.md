# `ev` — design philosophy

The principles behind `ev` — why it surfaces facts, not verdicts, and what a long-running system taught us.

Nine tenets. Each was held as a stance, then pressure-tested against a long-running, high-volume,
fresh-actor-every-round system. The wording below is the *post-pressure-test* form: what survived,
with the lessons folded in.

## 1. Facts, not verdicts
ev surfaces flat, unscored facts (green / red / not-run / stale / gray->red / unproven / silently-unbound /
exempt). Never a score, rank, or health number — precision past the real noise floor is a lie, and a
score invites chasing the number instead of the action. **Necessary but not sufficient:** a flat fact
list still rots if it has no discipline — cap the item count, lead each item with the action, and
hold a noise floor (don't let a one-bit jitter become a phantom to chase). Delete the score; keep
triage-by-action, a cognitive cap, and actionability.

## 2. Detect, don't prevent — but detection without teeth is wallpaper
ev makes a broken assumption / a dead guard / an overridden ruling **visible**; it does not block the
underlying mistake. But a signal that only reports gets ignored — it becomes wallpaper, and a team
will even turn its own warnings down to fight the spam. The teeth are the **gate**: `--exit-on-red`
turns a fact into a stop. ev offers detection plus an optional gate; used without the gate, expect to
be seen and ignored. What earns heeding is enforcement, not eloquence.

## 3. Resurface on a verified not-green — and absence is not-green
Fire exactly when there is a **verified not-green fact**: a real red, OR a verified liveness gap —
not-run (a required run is missing), stale (the run is behind live origin / a triggering change / the
window), ran-on-the-wrong-platform, or silently-unbound. **The absence of a required run is
not-green, never silent-green.** What ev refuses is a *speculative* alarm — a guess with no evidence.
A verified absence is evidence, not speculation. (The dominant real-world failure is the silent break
that never produces a red; a philosophy that only fires "on a verified red, never on absence" is
defenseless against exactly that — so the liveness meta-guard, which treats absence as red, is the
heart of this tenet, not an exception to it.)

## 4. Immutable chain — but hot/cold by read path, and retractable in principle
A decision is never edited in place; a re-judgment is a new child; the chain is append-only and
content-addressed. But immutability has a cost: records pushed into a cold, rarely-read archive
**rot** (they go stale unnoticed — legible but dead). So split by read path: keep the live decisions
on the **hot** path (what `brief`/`list` surface every round); let the chain be the **cold** archive.
And immutability is a property of the *chain*, not a ban on retraction: regretted or sensitive content
(e.g. anything that shouldn't live forever in history) must become **tombstone-able** — id preserved,
content retracted — or the ledger becomes a permanent liability. (tombstoning is a stated
requirement for the chain, not a shipped capability.)

## 5. State the honesty boundary — and cover the painful half
Say plainly what ev does not do (it does not prevent the mistake, does not fire on external-state
drift, covers only the machine-bindable share). Stated limits earn
trust **on one condition: the half ev covers must be the painful half.** "Only half" is dismissed if
it is the easy half; it is adopted when the covered half (durable capture of the human-judged
decisions that otherwise scroll away) is the one that actually hurts.

**Detection is structural-token only.** A rejected-road tripwire — a falsifiable check bound to a road
a `user-ruled` decision closed — catches a re-walk only when the re-walk touches a **structural,
grep-able artifact** (a file token, a commit, a schema). It reads green while the road stays closed and
red when re-walked, and the required counter-test proves it can flip; but a **prose** act with no token
is outside its reach. The canonical miss is **#1194** — a milestone re-assignment with no git commit of
the act and no structural marker. ev's record can hold that decision, but the tripwire cannot *catch* a
prose re-walk of it. We state this as scope, not apology, and **never** claim the tripwire "fixes" or
"would have caught" such a case.

**Agent-authored rules never gate.** A tick with `provenance=agent-proposed` can never flip
`--exit-on-red`: any not-green verdict maps to the non-gating `memo` (surfaced, never a false-green,
never a block). An agent cannot author a gating rule; only a named human ratifies one. This mirrors
`ev brief` excluding agent-proposed from the boot-read, and it protects the tripwire — an agent-authored
tripwire cannot gate. The enforcement is at **gate time** (the provenance check, LOCK 3), the one
chokepoint every tick passes through regardless of how it entered — not a signature, and not the
`--authority` tag (which is declared, not cryptographically verified: an agent could even claim
`user-ruled`, and the `provenance=agent-proposed` exclusion would still deny it the gate). The signing
boundary is banked; until then, the gate-time provenance check is what holds the line.

## 6. Capture beats auto-resurface for the human-judged half
The most painful decisions are pure judgment — no falsifiable check can auto-surface them. There, the
value is durable **capture** + a legible boot-time object + a re-check reminder, not cleverness. But
capture only happens if someone captures — and a fresh actor does not capture spontaneously. So
capture-discipline lives or dies on the read path (see tenet 9).

## 7. Seam, not merge — but the seam may be a merge signal
ev reads external signals (git, a selected-list); it never absorbs the other system's engine
(affinity, CI). The seam between declared intent and observed reality **is** the gap ev surfaces.
But a surfaced seam is a **diagnosis**, not a permanent architecture: sometimes the right fix is for
the team to *merge* the two underlying systems and eliminate the gap. ev stays the thin reader; the
diagnosis it provides can legitimately drive a merge ev itself does not perform.

## 8. Human-legible — and on the read path
The resurfaced object must be understandable by a person at the moment of re-decision; not agent-only.
But legibility is necessary, not sufficient: a record nobody passes by is written-and-ignored. The
decisions that matter must sit where the reader — human or agent — is **forced to pass** (the boot
read). Legible + off-path = dead.

## 9. Boot-path or dark code; sunset by default
In a fresh-actor-every-round system this decides life or death. A mechanism that is not on the path
the actor is **forced to re-read each round** runs zero times — it is dark code, doomed regardless of
how good it is. So every enforcement mechanism declares, at birth, three things:
- **(a) read path** — is its trigger on the must-read-every-time context? If not, it does not exist.
- **(b) sunset** — what condition retires it? (Prune-observe-judge-codify with an observation window,
  not prune-then-forget — Chesterton's fence with eyes open.)
- **(c) premise** — is it pinned to a **snapshot** or to **live truth**? A snapshot premise silently
  inverts into a lie as the guarded thing changes underneath it. (ev's sha-pin + live-origin staleness
  is exactly this defense: a binding pinned to an old sha reads stale, not green.)

And the read path must be **near-zero-cost** (local, no network) or budget pressure will drop it.

---

## How the tenets relate
3 + 9 are the load-bearing pair: tenet 3 says *absence is not-green* (so silent breaks get caught),
and tenet 9 says *the catch must be on the read path* (so the catch actually runs). 1/2/8 are about
keeping the signal cheap, heeded, and seen. 4/7 are the long-run corrections (immutability rots
off-path; a seam can warrant a merge). 5/6 are the scope: cover the painful, human-judged half, and
state the rest honestly.
