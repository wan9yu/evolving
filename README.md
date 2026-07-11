# ev — a closure engine

[![CI](https://github.com/wan9yu/evolving/actions/workflows/ci.yml/badge.svg)](https://github.com/wan9yu/evolving/actions/workflows/ci.yml)
[![coverage](https://codecov.io/gh/wan9yu/evolving/branch/main/graph/badge.svg)](https://codecov.io/gh/wan9yu/evolving)
[![crates.io](https://img.shields.io/crates/v/evolving.svg)](https://crates.io/crates/evolving)

Claims are cheap. Agents report "done", "fixed", "verified"; dashboards report green.
Evidence is not cheap, and almost nothing forces a claim through it before the claim is believed.

`ev` is a small command-line closure engine for one person and the agents working alongside them.
An agent files a **claim** with a typed **evidence pointer**. The engine **checks that the pointer
resolves** — deterministically, never judging whether the work is good — and counts how far the
world has since moved under each anchor (**drift**). Only the
**human closes** a claim: with evidence, on hold, or declared dead. Nothing gates, nothing blocks —
a short daily **pause** is where the judgment happens, and a **line** of what closed with evidence is
what accumulates.

```
agent claims ─▶ evidence pointer ─▶ anchor resolves? (exists · matches — a fact, not a verdict)
                                          │
                                    drift: how far the world has
                                    moved under the anchor since
                                          │
                                    the human closes  ─▶  close with evidence
   (nothing gates; a claim with no                    │   hold (grey)
    evidence simply waits at the pause)               └─  declare dead
                                          │
                                    the line: closed-with-evidence vs let-go
```

## Install

```sh
cargo install evolving   # installs the `ev` binary
```

Prebuilt static binaries (including aarch64 musl for hosts without a toolchain) are attached to
each GitHub Release.

## The loop

```sh
ev init                                   # enroll a repo
ev hook install                           # wire the session hooks: auto-capture + the brief
ev claim "fixed the parser" \             # an agent files a claim with a pointer
    --by agent --evidence commit:<sha>
ev claim "the boundary is safe"           # a bare claim — no pointer yet
ev verify                                 # re-check anchors; report drift
ev pause                                  # the human's daily ritual: demand, attach, hold, let go
ev line                                   # the work line: what closed with evidence
ev doctor                                 # check the ledger's integrity
```

Sessions also leave exhaust: your commits are captured automatically as self-evident claims, so you
don't file one for every commit — you file one when you want to assert something a bare commit doesn't
say, and back it with a pointer.

Evidence pointer types: `commit:<sha>` · `test:<path>[::<pass-line>]` · `file:<path>[::<line>]` ·
`artifact:<name>[::<line>]` · `metric:<text>` and `url:<text>` (recorded, not checked).

## How it works

Everything is an event in an append-only ledger (`.evolving/ledger/`, one JSONL file per machine,
committed with the repo). No database, no daemon: every invocation re-reads the events and folds
them into the current state — a claim moves `bare → evidenced → anchored`, or sits grey, or ends
closed or dead. History is never rewritten; corrections are new events beside the old ones.

An anchor is checked for exactly one thing: does the pointer **resolve** — the commit exists, the
file contains the named line. `resolves` is a fact about the pointer, never a verdict on the claim.
Evidence auto-derived from your own commits is marked **⊙** (self-evident); independently filed
anchors get **✓** — never the same mark, because evidence must not self-certify.

A filed anchor records the repo state it was filed against (its `base`). For path-bearing anchors
(`test:`, `file:`, `artifact:`), ev can then count **drift**: how many commits have touched the
cited path beyond that base. (Auto-captured commit exhaust carries no base — a commit is its own
fixed point.) Drift is measured in world movement, not clocks — an anchor can still resolve while
the claim it supported has gone stale underneath. The engine counts; what the count means is the
human's judgment.

That judgment happens at the pause: demand evidence, attach it, hold in grey, or let a claim die.
Closing is its own deliberate act — `ev close <id>`, on a claim that has earned it. What
accumulates is the line — two raw counts, never a score.

## What it refuses to do

- **Facts, not verdicts.** The engine checks whether a pointer resolves and how far the world has
  drifted under it — never whether the work behind it is good, never by asking a model, never over
  the network. Whether the evidence covers the promise is the human's call at the pause.
- **Nothing gates.** Session hooks always succeed; the only refusals are on your own verbs — a claim
  closed without evidence is refused, because *closed-anyway* should not exist.
- **Only the human closes.** Agents may file claims and attach evidence. Closing is yours.
- **No daemon.** State refreshes when you invoke `ev`, never in the background.

## For agents

A repo running `ev` carries an [`AGENTS.md`](AGENTS.md) that tells any coding agent how to file
evidence-backed claims and answer a demand.

## Design

The internals — the append-only ledger, the fold, anchor resolution and drift, the sweep, and the
pause — are described in [`docs/design.md`](docs/design.md).

## License

Apache-2.0.
