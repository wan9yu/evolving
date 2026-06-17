# `ev` documentation

`ev` is **git for decisions**: it records human-authored decisions and the grounds they
rest on as an immutable, content-addressed *tick chain*, binds a test-check or a human
re-check to each ground, and audits that chain against a fixed set of refusals.

These docs cover the **shipped `0.0.1` surface** — the commands that exist today
(`init`, `decide`, `guard`, `show`, `verify`). For what is still landing toward `0.1.0`,
see the **Status** section of the [project README](../README.md).

- **[commands.md](commands.md)** — the authoritative command reference: every flag, exit
  code, the exact strings each command prints, and a worked example per command.
- **[concepts.md](concepts.md)** — the model in depth: the Tick schema, Grounds, Checks,
  content-addressed identity and the frozen golden vectors, append-only immutability, the
  refusals `ev verify` enforces, and the honesty / trust boundary.

Back to the [project README](../README.md).
