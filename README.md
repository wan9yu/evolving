# evolving

**English** | [中文](README.zh-CN.md)

[![CI](https://github.com/wan9yu/evolving/actions/workflows/ci.yml/badge.svg)](https://github.com/wan9yu/evolving/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/wan9yu/evolving/branch/main/graph/badge.svg)](https://codecov.io/gh/wan9yu/evolving)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE)

> **Decisions don't stay right.** `ev` watches the reason a decision rests on and resurfaces it when that reason breaks — at your next `ev check`, no watcher to run.

## The problem

Your agents make decisions faster than anyone can track — *build our own retrieval; keep the schema frozen; no Redis* — each one resting on a reason that was real at the time. Then it scrolls out of view: the thread is archived, the run ends, the agent that made the call is gone by the next one.

Months later the reason quietly moves — a dependency changes behavior, a constraint that made the call correct lifts, the test that proved the claim stops running. The decision is still in force, still shaping the codebase, but the ground beneath it is gone. So a fresh agent re-derives a call you already settled, or builds on a premise that quietly died — and you, the human on the hook, find out late, if at all.

`ev` is the layer that closes that gap. It records a human-authored decision *and the grounds it rests on* as an immutable, content-addressed chain, binds a falsifiable check to a ground, and **resurfaces the decision when that check goes red — at your next `ev check`, no watcher to run.** It deals in facts, not verdicts: no scores, no ranks, no auto-judgements — just an honest record of what was decided, why, who is on the hook, and whether the check guarding each reason is still alive. Under the hood it's *git for decisions*; agents propose, a human ratifies and stays on the hook.

A single self-contained Rust binary. No network, no daemon. The store is a local `.evolving/` directory — content-addressed and append-only.

## What `ev` is — and is not

The question `ev` gets most often is *isn't this just …?* It is not:

- **An ADR.** An Architecture Decision Record captures *why* a decision was made — as prose, written once, then left to rot. Nobody re-reads it, and nothing tells you when the premise it rested on stops holding. `ev` records the *why* too, but binds it to a falsifiable check and **brings the decision back when that check goes red**. An ADR is a tombstone; an `ev` decision is alive.
- **A comment on a test.** A note like *"this test guards the no-Redis decision"* is unstructured prose no one reads at decision time. When the test fails you get a red test — not *the no-Redis decision broke; here is the alternative that was rejected and who is on the hook* — and a comment can't tell you the guard itself quietly stopped running. `ev` makes the link structured and content-addressed, resurfaces the whole decision, and tracks whether the check is even still alive.
- **git.** git versions the *code* — the *what*. It has no notion of a decision, the grounds it rests on, or whether a past call's assumption still holds; `git log` is findable but it never comes *to* you. `ev` borrows git's spine — immutable, content-addressed, append-only — and adds the one verb git lacks: **resurface a decision when the ground beneath it moves.**
- **An agent-memory store.** Mem0, Zep, Letta and the like *remember* — they recall facts and context from past sessions when asked, and surface a contradiction only reactively. `ev` is not memory; it is **active and narrow.** It holds human-authored *decisions and the checks bound to them* — not arbitrary recall — and it does not wait to be queried: when a decision's bound check goes red, the decision comes back on its own. Memory tells you *what you decided*; `ev` tells you *when what you decided stopped being true*.

And it is not a task tracker, a CI system, or an environment monitor: it manages no work items, it does not own your test suite (it only reads whether a bound check passed), and it fires on git-recorded change — never on a UI click or a config drift that leaves no commit. `ev` detects and resurfaces; it does not prevent.

For the full landscape — ADR tools, decision ledgers, agent memory, signed-provenance protocols, architecture-fitness functions, governance frameworks — and where `ev` sits among them, see [`docs/neighbors.md`](docs/neighbors.md).

## Install

```sh
cargo install evolving
```

This installs an `ev` binary on your `PATH`. The package is named `evolving`; the command is `ev`.

No Rust toolchain? Each release attaches **prebuilt static binaries** — download the one for your platform from the [latest release](https://github.com/wan9yu/evolving/releases/latest) and drop it on your `PATH`. The Linux builds are static (`musl`), so the `aarch64-unknown-linux-musl` binary runs on any ARM-Linux host regardless of its glibc:

```sh
curl -L <asset-url> | tar xz
install ev ~/.local/bin/ev
ev --version
```

Build from source:

```sh
git clone https://github.com/wan9yu/evolving
cd evolving
cargo build --release
# binary at target/release/ev
```

## Quickstart

The core loop: **decide → bind a check → keep working → resurface on red.**

Create the store:

```sh
ev init
```

Record a decision with a chosen ground (re-checked by a human at a named time) and a road-not-taken:

```sh
ev decide "build our own retrieval; reject pgvector" \
  --observe "evaluating retrieval backend for v2" \
  --assume "team has bandwidth to maintain it long-term" \
  --revisit "Q3" \
  --reject "pgvector: would lock our schema" \
  --blame "You"
```

Record a decision whose chosen ground is guarded by a **test** rather than a human. A test binding must carry a counter-test (the test that should flip red if the claim breaks), at least one platform / trigger / surface for liveness, and the commit it was last verified at:

```sh
ev decide "restore-safety counter DB-backed; reject Redis" \
  --observe "multi-pod restore-safety counter" \
  --assume "no Redis; multi-pod coordination via the existing DB" \
  --assume-test "pytest tests/test_redis_absent.py" \
  --counter-test "pytest tests/test_redis_absent.py::test_redis_injection_flips_red" \
  --on-platform linux-ci \
  --triggered-by pyproject.toml \
  --surface pyproject-deps \
  --verified-at-sha d308afac1b2c3d4e5f60718293a4b5c6d7e8f901 \
  --reject "Redis: a new infra dependency" \
  --blame "You"
```

Bind a test to an unbound ground of the current HEAD decision *after the fact* with `ev guard`. Because the check is part of the hashed payload, this writes a **new child** rather than mutating the existing tick:

```sh
ev guard "pytest tests/test_schema_frozen.py" <HEAD-id> "schema stays frozen" \
  --counter-test "pytest tests/test_schema_frozen.py::test_schema_change_flips_red" \
  --on-platform linux-ci \
  --triggered-by schema.sql \
  --surface schema-ddl
```

`<HEAD-id>` is the id printed by the most recent `ev decide` / `ev guard`. The third positional argument names which ground to bind (by claim text or by index); it is required only when more than one ground is still unbound.

Audit the chain and its refusals, then read a decision in full:

```sh
ev verify
ev show <id>
```

`ev verify` confirms every id equals the hash of its payload, that lineage is forward-only, and that every tick validates against the closed schema and check shape.

Evaluate the bound checks and resurface any decision whose check has gone red. With a test-check bound to a runnable command, `ev check --run` runs it, records a receipt, and runs its counter-test to prove the binding can actually flip; `--exit-on-red` gates the exit code. `ev why` maps a check back to the decision it guards, and `ev reopen` shows the full decision object (frozen-vs-current, with the road-not-taken):

```sh
ev check --run --platform linux-ci --exit-on-red
ev why "pytest tests/test_schema_frozen.py"
ev reopen <id>
```

## The honesty boundary

`ev` completes one specific picture — *does a human-vetted decision stay live, and is the check guarding it itself alive?* — and is honest about the edges of that picture:

- **It does not claim tamper-resistance of offline test outcomes.** `ev` records that a test was bound and the commit it was verified at, but it cannot prove an offline test result was honest. That is a documented boundary, not a guarantee.
- **It fires on changes recorded in git** — a bound check going red, or a commit touching a declared trigger. It does **not** detect external-state drift: a UI click, an org or config change, or an upstream-API behavior change that leaves no git commit will not trigger `ev`. A check that can only fail on external state belongs on a timer, not bound to a trigger.
- **It detects; it does not prevent.** `ev` is decision memory that resurfaces a broken assumption, not an environment sentinel that stops one from happening.
- **It assumes a single writer per store.** `ev` takes no lock when it appends a tick and advances `HEAD`, so two `ev` processes writing the same store concurrently can fork the chain or race the pointer. This is bounded and recoverable — content-addressing prevents corruption — but serialize your writers if you script `ev`.

## Documentation

Usage docs live in [`docs/`](docs/):

- [`docs/concepts.md`](docs/concepts.md) — the model in depth: the Tick schema, Grounds, Checks, content-addressed identity, append-only immutability, jurisdiction, provenance, the forward-compatible schema, and the refusals `ev verify` enforces.
- [`docs/neighbors.md`](docs/neighbors.md) — where `ev` sits in the landscape: its real neighbours (ADR tools, Lore, decision ledgers, agent memory, signed-provenance, fitness functions, governance frameworks), each with its approach and `ev`'s different path — plus the shared ground and `ev`'s own gaps.
- [`docs/commands.md`](docs/commands.md) — the authoritative command reference: every flag, exit code, the exact strings each command prints, and a worked example per command.
- [`docs/migrating.md`](docs/migrating.md) — bringing an existing decision history into `ev`: the canonical decision-intake format, writing a small adapter that emits it, and the built-in convenience extractors.
- [`docs/philosophy.md`](docs/philosophy.md) — the design philosophy: the tenets behind `ev`, and why it makes the choices it does.
- [`docs/measuring-drift-defense.md`](docs/measuring-drift-defense.md) — how to honestly measure whether `ev` is catching real re-derivations: never quote a catch-rate without its denominator, the blind external denominator, the never-broke control, the uncatchable co-population, and leading with a MISS.

**Using `ev` from an AI agent?** [`skills/ev/SKILL.md`](skills/ev/SKILL.md) is a tool-agnostic agent skill — drop it into your agent's skills directory so it uses `ev` correctly without reading the manual.

## License

Apache-2.0.
