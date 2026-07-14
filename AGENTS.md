# Working with ev

This repo runs `ev`, a closure engine: claims are recorded with typed evidence pointers, the
engine checks deterministically that pointers resolve, and only the human closes a claim. Nothing
here gates or blocks your work — it records what was claimed and whether the evidence resolves.
A resolving anchor is a fact about the pointer, never proof the claim is right — that judgment
stays with the human.

When you finish a unit of work:

- **File a claim with a typed evidence pointer:**
  `ev claim "what you did" --by agent --evidence commit:<sha>`
  (or `--evidence test:<path>::<a line that must appear in it>`).
- **If the human demands evidence for a prior claim, answer it:**
  `ev evidence <claim-id> <ref>`.
- **Never run `ev close`** — closing a claim is the human's move. Filing evidence is yours.
- A claim with no evidence stays open and is surfaced at the human's next pause.

Filing discipline — match the pointer to the kind of claim (declare it with `--kind`):

- A **defect** claim ("X is broken / leaks / is unreachable") should carry a runnable pointer —
  `test:<path>::<text from the reproducing line>` naming a test or reproduction, not just the file
  where the bug lives. ev anchors by **content, not by line number**: `::<text>` goes red when the
  cited line changes, while a bare `file:<path>` goes red only if the file is deleted.
- A **priority** claim ("the next version should X") cannot be proven by a resolving anchor: the
  anchor only shows the gap's neighborhood exists. Before filing one, search the target for X
  already shipped or already rejected (code, docs, design notes), and attach what you searched and
  found as additional evidence. If X already exists, do not file the claim.

Evidence pointer types: `commit:<sha>` · `test:<path>[::<text on the cited line>]` ·
`file:<path>[::<text on the cited line>]` · `artifact:<name>` · `metric:<text>` (recorded, not
verified) · `url:<text>` (recorded, not verified). The `::` payload is **text to match, never a line
number** — `file:src/x.rs:56` is refused, `file:src/x.rs::fn parse(` is the anchor that goes red when
that line changes.

A content anchor (the `::<text>` form) must quote text that exists in the target **right now** — ev
refuses to file one otherwise, since an anchor on absent text is born red and can never carry a
signal. A bare `file:<path>` (no `::`) is refused too, if the trailing segment after a `:` is a line
number — that shape almost always means a line was meant, and ev anchors by content, not by line.
The statuses an agent will see, on `ev evidence`, `ev verify`, or a claim's evidence read back, are
`resolves` · `changed` · `gone` · `unreachable` · `recorded`. `changed` means **the cited line
changed — re-read what is there now**, never "fixed": ev has no way to tell whether the code that
replaced it addresses what the claim described.

On machines where the session hooks are wired (`ev hook install`, once per machine), your session's
commits are captured automatically as self-evident claims — so you do not have to file a claim for
every commit. File one when you want to assert something a bare commit does not say (fixed,
verified, safe), and back it with a pointer.
