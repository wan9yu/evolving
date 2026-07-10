# ev — a closure engine

Claims are cheap. Agents report "done", "fixed", "verified"; dashboards report green.
Evidence is not cheap, and almost nothing forces a claim through it before the claim is believed.

`ev` is a small command-line closure engine for one person and the agents working alongside them.
An agent files a **claim** with a typed **evidence pointer**. The engine **verifies the pointer**
deterministically — it checks that the pointer resolves, never whether the work is good. Only the
**human closes** a claim: with evidence, on hold, or declared dead. Nothing gates, nothing blocks —
a short daily **pause** is where the judgment happens, and a **line** of what closed with evidence is
what accumulates.

```
agent claims ─▶ evidence pointer ─▶ engine verifies (exists? matches?)
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

## The loop

```sh
ev init                                   # enroll a repo
ev claim "fixed the parser" \             # an agent files a claim with a pointer
    --by agent --evidence commit:<sha>
ev claim "the boundary is safe"           # a bare claim — no pointer yet
ev pause                                  # the human's daily ritual: demand, close, hold, let go
ev line                                   # the work line: what closed with evidence
ev doctor                                 # check the ledger's integrity
```

Sessions also leave exhaust: your commits are captured automatically as self-evident claims, so you
don't file one for every commit — you file one when you want to assert something a bare commit doesn't
say, and back it with a pointer.

Evidence pointer types: `commit:<sha>` · `test:<path>[::<pass-line>]` · `file:<path>[::<line>]` ·
`artifact:<name>` · `metric:<text>` and `url:<text>` (recorded, not verified).

## What it refuses to do

- **Facts, not verdicts.** Verification checks whether a pointer resolves and matches — never whether
  the work behind it is good, never by asking a model, never over the network. Whether the evidence
  covers the promise is the human's call at the pause.
- **Nothing gates.** Session hooks always succeed; the only refusals are on your own verbs — a claim
  closed without evidence is refused, because *closed-anyway* should not exist.
- **Only the human closes.** Agents may file claims and attach evidence. Closing is yours.
- **No daemon.** State refreshes when you invoke `ev`, never in the background.

## For agents

A repo running `ev` carries an [`AGENTS.md`](AGENTS.md) that tells any coding agent how to file
evidence-backed claims and answer a demand.

## License

MIT.
