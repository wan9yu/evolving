# Neighbors — where `ev` sits

Decisions deserve infrastructure, and a growing set of tools is built on that premise. This page places `ev` among them honestly: what each neighbour is for, how it works, and where `ev` takes a different path.

`ev` combines three properties:

1. an **immutable, content-addressed chain** of decisions,
2. the **grounds** a decision rests on as first-class structured data, and
3. a **falsifiable check** bound to a ground that, when it goes red, **actively resurfaces** the decision.

Properties (1) and (2) are shared with several neighbours below. The one `ev` has not found elsewhere is (3) — *re-evaluating a recorded decision against a changing world and bringing it back when its premise breaks.* That is where the differences below concentrate.

## The neighbours

| Neighbour | What it is for | How it works | `ev`'s different path |
|---|---|---|---|
| **ADR tools** — log4brains, npryce/adr-tools, MADR | Write and publish architecture decisions | Static markdown; supersession by a "superseded" status, by convention | The record is content-addressed and carries a runnable check — a broken premise brings the decision back, rather than relying on someone re-reading a file |
| **Lore Protocol** ([arXiv:2603.15566](https://arxiv.org/abs/2603.15566)) | Structured knowledge for AI coding agents, in git commit trailers | Grounds-style trailers (constraints, rejected alternatives); staleness via five passive heuristics (age, code drift, confidence, expiry hint, orphaned deps), surfaced on demand with `lore stale` | Binds a *falsifiable* check that is re-run and goes red on a real failure — an executed verdict, not a heuristic staleness flag |
| **AgDR** — [me2resh/agent-decision-record](https://github.com/me2resh/agent-decision-record) | Record what an agent decided | A record format for agent decisions | `ev` watches whether the decision is *still true*, and inverts the authorship line — a human ratifies and stays on the hook |
| **Decision ledgers** — agentic-decision-ledger, ElixirData | Govern decisions at decision time | Pre-execution admissibility gates; ElixirData adds a live decision stream and a manual "replay" of a past decision | `ev` does not gate at write time; it re-checks a *recorded* decision later, automatically, when the world it rested on moves |
| **Agent memory** — Mem0, Zep/Graphiti, Letta/MemGPT | Remember facts and context across sessions | Passive recall; contradictions surfaced reactively | `ev` is active: it binds a check and resurfaces — judgment and governance, not recall |
| **Signed-provenance / agent identity** — content-addressed + signed memory protocols (BLAKE3 + Ed25519), agent-identity schemes | Cryptographic provenance and capability boundaries for agent memory | Signed chains; identity and authorization | `ev` is honest-by-construction — provenance is *declared, not cryptographic*; integrity is *tamper-evident* (`ev verify` detects edits), not write-enforced. None of these watch a decision's premise |
| **Architecture-fitness / policy-as-code** — ArchUnit, OPA, fitness functions | Enforce architectural rules in CI | Block the build on a violation | `ev` is advisory — it *resurfaces*, it does not block — and it ties the check back to the *past decision and its grounds*, not just "a rule was violated" |
| **Governance frameworks** — NIST AI RMF (MEASURE 2.4 / 3.1), EU AI Act Art. 12 | Standards and regulation for AI monitoring and logging | Frameworks and mandates | `ev` is a single-binary substrate that can help satisfy a continuous-monitoring outcome (e.g. NIST MEASURE 2.4/3.1) — advisory alignment, not a compliance product |

## Honest edges

- **Shared ground.** An immutable chain and first-class grounds are not unique to `ev` — Lore, for one, has both. The property `ev` has not found elsewhere is the *active resurface when a bound, falsifiable check goes red.*
- **`ev`'s own gaps.** Provenance is **declared, not cryptographic** — signing is a deliberate non-goal, not a shipped feature. Immutability is **tamper-evident** (`ev verify` flags any edit), **not** write-enforced; `ev` is advisory, never a gate beyond a git hook. It fires on git-recorded change, never on external-state drift that leaves no commit.
- **No universal claim.** "Not found elsewhere" means exactly that — a survey, not a proof of a vacancy. If a tool already watches a decision's premise and resurfaces it, that is a neighbour we have not yet met, and we would want to know.
