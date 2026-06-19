# evolving

**English** | [中文](README.zh-CN.md)

[![CI](https://github.com/wan9yu/evolving/actions/workflows/ci.yml/badge.svg)](https://github.com/wan9yu/evolving/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/badge/crates.io-v0.0.1-orange)](https://crates.io/crates/evolving)
[![codecov](https://codecov.io/gh/wan9yu/evolving/branch/main/graph/badge.svg)](https://codecov.io/gh/wan9yu/evolving)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE)

`ev` is **git for decisions**. It records human-authored decisions and the grounds they
rest on as an immutable, content-addressed *tick chain*, binds either a test-check or a
human re-check to each ground, and resurfaces a decision when a bound check goes red. It
deals in **facts, not verdicts** — there are no scores, no ranks, no auto-judgements;
just an honest record of what was decided, why, who is on the hook, and whether the check
guarding each reason is still alive.

## Status

`0.0.1` — an early, honest cut on the way to the **`0.1.0` honest-resurface slice**. A
single self-contained Rust binary, no network, no daemon; the store lives in a local
`.evolving/` directory. The `0.1.0` slice is feature-complete in the source tree; the
published crate is still `0.0.1`, not yet tagged.

**Shipped:** the full capture→resurface loop — recording decisions and their grounds
(`ev decide`), binding a test or human re-check after the fact (`ev guard`), evaluating a
bound check and resurfacing a decision when it goes red (`ev check [--run] [--exit-on-red]`,
the flat verdict states), the **liveness meta-guard** (`ev check` flags a check that never
ran on a declared platform as not-run, with event-driven freshness and per-runner `--attest`
scoping), naming the decision a check guards (`ev why`), reading a decision in full
(`ev reopen` / `ev show`), browsing the ledger (`ev list` / `ev log`), and auditing the chain
and its refusals (`ev verify`). `ev check --run` runs the bound check for you, records a receipt,
and runs its counter-test to prove the binding can actually flip — a check that cannot flip is
reported `unproven`. The **authority tag** (`--authority user-ruled` / `agent-disposable`, surfaced
by `ev brief` so a fresh agent reads a human's ruling before re-deciding) and seeding a decision from
a commit (`ev decide --from-git`) are shipped too — the `0.1.0` slice is feature-complete; only the
release cut and tag remain.

## Install

```sh
cargo install evolving
```

This installs an `ev` binary on your `PATH` (the package is named `evolving`; the command
is `ev`).

Build from source:

```sh
git clone https://github.com/wan9yu/evolving
cd evolving
cargo build --release
# binary at target/release/ev
```

## Quickstart

Create the store:

```sh
ev init
```

Record a decision with a chosen ground (re-checked by a human at a named time) and a
road-not-taken:

```sh
ev decide "build our own retrieval; reject pgvector" \
  --observe "evaluating retrieval backend for v2" \
  --assume "team has bandwidth to maintain it long-term" \
  --revisit "Q3" \
  --reject "pgvector: would lock our schema" \
  --blame "You"
```

Record a decision whose chosen ground is guarded by a **test** rather than a human. A test
binding must carry a counter-test (the test that should flip red if the claim breaks), at
least one platform / trigger / surface for liveness, and the commit it was last verified at:

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

Attach a test to an unbound ground of the current HEAD decision *after the fact* with
`ev guard`. Because the check is part of the hashed payload, this writes a **new child**
rather than mutating the existing tick:

```sh
ev guard "pytest tests/test_schema_frozen.py" <HEAD-id> "schema stays frozen" \
  --counter-test "pytest tests/test_schema_frozen.py::test_schema_change_flips_red" \
  --on-platform linux-ci \
  --triggered-by schema.sql \
  --surface schema-ddl
```

`<HEAD-id>` is the id printed by the most recent `ev decide`/`ev guard`. The third
positional argument names which ground to bind (by claim text or by index); it is required
only when more than one ground is still unbound.

Audit the chain and the refusals, then read a decision in full:

```sh
ev verify
ev show <id>
```

`ev verify` confirms every id equals the hash of its payload, that lineage is
forward-only, and that every tick validates against the closed schema and check shape.

Evaluate the bound checks and resurface any decision whose check has gone red. With a
test-check bound to a runnable command, `ev check --run` runs it, records a receipt, and
gates the exit code under `--exit-on-red`; `ev why` maps a check back to the decision it
guards, and `ev reopen` shows the full decision object (frozen-vs-current, with the
road-not-taken):

```sh
ev check --run --platform linux-ci --exit-on-red
ev why "pytest tests/test_schema_frozen.py"
ev reopen <id>
```

## The model

- **Tick** — one decision in the chain. Its hashed payload is `{decision, observe,
  grounds, parent_id}`; `id`, `status`, `held_since`, and `blame` are bookkeeping kept
  outside the hash.
- **Ground** — a reason a decision rests on. A ground is either **chosen** (a reason for
  the decision taken) or a **road-not-taken** (`rejected:<option>`, a reason an
  alternative was declined).
- **Check** — what keeps a chosen ground honest over time. Either a **Test** (a test
  selector plus its counter-test, the platforms/triggers/surfaces that keep it live, and
  the `verified_at_sha` it last passed at) or a human **Person** re-check (a reference to
  when/where a person re-affirms the ground).
- **Identity** — `id = first 12 hex of SHA-256` over the canonical-JSON of `{decision,
  observe, grounds, parent_id}`.
- **Append-only** — the chain is never edited in place. A change is a **new child** whose
  `parent_id` points at its predecessor.

## The refusals it enforces (the red lines)

- **Closed schema.** A tick with any field outside the fixed schema is rejected.
- **A human re-check stays human.** A ground re-checked by a person can never be
  force-bound to a test.
- **A rejected road carries no check.** A road-not-taken cannot take a check in `0.1.0`
  (reserved for a future rejection-rationale liveness feature).
- **The system is never the subject of self-evolve language.** Self-evolve / self-improve
  verbs must take a human subject, not the system (best-effort lexical lint).
- **Every mutating op names a human.** A decision or a guard must carry a `--blame` (or a
  resolvable `git config user.name`).
- **No auto-close.** Nothing closes, prunes, or stops a decision on its own; a human
  authors every change.

## Honesty / trust boundary

`ev` completes one specific picture: *does a human-vetted decision stay live, and is the
check guarding it itself alive?* It does that by content-addressing the decision record and
by demanding that every test binding name a counter-test and the surfaces that keep it
live, so a check that has quietly died is visible.

It does **not** claim tamper-resistance of offline test outcomes — `ev` records that a
test was bound and the commit it was verified at, but it cannot prove an offline test
result was honest. That is a documented boundary, not a guarantee.

`ev` fires on changes recorded in **git** — a bound check going red, or a commit touching a
declared trigger. It does **not** detect **external-state drift**: a UI click, an org/config
change, or an upstream-API behavior change that leaves no git commit will not trigger `ev`.
`ev` is decision memory, not a replacement for an environment sentinel; a check that can only
fail on external state should be run on a timer (a 0.1.x capability), not bound to `triggered_by`.

## Documentation

Usage docs live in [`docs/`](docs/):

- [`docs/commands.md`](docs/commands.md) — the authoritative command reference: every flag,
  exit code, the exact strings each command prints, and a worked example per command.
- [`docs/concepts.md`](docs/concepts.md) — the model in depth: the Tick schema, Grounds,
  Checks, content-addressed identity, append-only immutability, and the refusals
  `ev verify` enforces.
- [`docs/philosophy.md`](docs/philosophy.md) — the design philosophy: the nine tenets behind
  `ev`, and why it makes the choices it does.

**Using `ev` from an AI agent?** [`skills/ev/SKILL.md`](skills/ev/SKILL.md) is a
tool-agnostic agent skill — drop it into your agent's skills directory so it uses `ev`
correctly without reading the manual.

## License

Apache-2.0.
