# Measuring drift-defense honestly

`ev` binds a falsifiable check to a settled decision so that a later change which **re-walks** the
settled road flips the check red and resurfaces the decision (and, under `--exit-on-red`, gates it).
A natural question follows: *how often does it actually catch a real re-derivation?* That number — a
**catch-rate** — is easy to quote and easy to fool yourself with. This page is the discipline for
quoting it honestly. It is the same anti-false-green stance `ev` applies to a single check, applied to
the evaluation of `ev` itself.

## The one rule: never quote a catch-rate without its denominator

"`ev` caught 9 of the last 10 regressions" is meaningless until you say **10 of what**. A catch-rate is
a fraction, and the fraction is only as honest as its denominator. State the denominator every time, in
the same breath as the rate.

## Four things every honest report carries

1. **A blind external denominator.** The set of "re-derivations that mattered" must be enumerated by
   someone **other** than the person who bound the checks, **without** seeing which ones `ev` would
   catch. If the author of the bindings also picks the denominator, they will (unconsciously) pick the
   population the bindings already cover — and drive the rate to 100% by construction. A rate computed
   over a self-selected population is a number about the selection, not about `ev`.

2. **A seeded never-broke control.** Replaying only the things that *did* break measures sensitivity but
   says nothing about false alarms — a check that fires on everything would score perfectly. Include
   decisions that were bound but **never** re-walked, and confirm `ev` stays green on them. A
   drift-defense that cries wolf is worse than none; the control is how you prove it does not.

3. **The uncatchable co-population, reported beside the rate.** `ev`'s enforcement is git-jurisdiction
   bound and structural-token bound (see the honesty boundary in [philosophy.md](philosophy.md)). Some
   re-derivations leave **no** structural token and **no** git commit of the act — a prose milestone
   re-assignment, a board-field change, a verbal reversal. Those are **uncatchable by construction**.
   Count them and report the count next to the catch-rate, so a high rate over the *catchable* slice is
   never mistaken for coverage of the *whole* problem. A rate you can drive to exactly 0 (or 100) is a
   tell that you are measuring a controlled sub-population.

4. **Lead with a MISS.** Open the report with a concrete re-derivation `ev` did **not** catch, and say
   why. Leading with the limit is what makes the rest of the number trustworthy. (For the no-free-tier
   flagship, the lead MISS is the prose milestone re-assignment archetype: a settled policy reversed
   with no code change for any test to grep.)

## What `ev` reports — and what it never reports

`ev` emits **facts**: a check is green / red / not-run / stale / unproven / silently-unbound / memo,
each naming its decision. It never emits a **score** or a **rank**, and a drift-defense report built on
it must not invent one either. In particular, **never roll the above into a single composite "health"
number** — a composite hides exactly the denominator and the uncatchable co-population this page exists
to surface. Report the slices; let the reader weigh them.

## The shape of a replay

A retrospective ("shadow") evaluation is zero-deploy: it runs over history you already have, changing
nothing in production.

1. Enumerate the candidate decisions and bind each to its existing invariant test (harvest with
   `ev migrate` — adopt the test the team already wrote; no new tests required). A harvested binding
   gates on its own red but is flagged *falsifiability not proven* until a counter-test is added.
2. Have a second party produce the blind denominator (the re-derivations that mattered) and the
   never-broke control set.
3. For each, replay `ev check` at the relevant commit (check out the commit, run the bound check). A red
   on a real re-derivation is a catch; a red on a control is a false alarm; a re-derivation with no
   structural fingerprint is an uncatchable.
4. Report: catch-rate **with** denominator, false-alarm count over the control, the uncatchable count,
   and the lead MISS.

Only after a non-author team has run at least this much is any number an `ev` *property* rather than a
tool-author's self-report.
