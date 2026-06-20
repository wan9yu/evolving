# `ev` documentation

`ev` is **git for decisions**: it records human-authored decisions and the grounds they
rest on as an immutable, content-addressed *tick chain*, binds a test-check or a human
re-check to each ground, and audits that chain against a fixed set of refusals.

These docs cover the `ev` command surface: `init`, `decide`, `guard`, `migrate`, `check`,
`why`, `reopen`, `show`, `brief`, `list`, `log`, `verify`.

- **[usage.md](usage.md)** — a task-oriented guide: the common workflows ("I just made a
  decision", "what's already ruled?", "is any assumption broken?") with a short example each.
- **[commands.md](commands.md)** — the authoritative command reference: every flag, exit
  code, the exact strings each command prints, and a worked example per command.
- **[concepts.md](concepts.md)** — the model in depth: the Tick schema, Grounds, Checks,
  content-addressed identity and the frozen golden vectors, append-only immutability, the
  refusals `ev verify` enforces, and the honesty / trust boundary.
- **[philosophy.md](philosophy.md)** — the design principles behind `ev`: the nine tenets
  explaining why it makes the choices it does (facts not verdicts, detect not prevent, boot-path
  or dark code).

## Usage

New to `ev`? Start with the workflow guide: **[usage.md](usage.md)**.

Back to the [project README](../README.md).
