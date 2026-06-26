#!/usr/bin/env sh
# Session-start hook — auto-inject the settled ev decisions at the start of an agent session.
#
# WHY A HOOK, NOT A SKILL: heeding the settled decisions must NOT depend on the agent remembering to
# run `ev brief`. A skill is invoked when the model judges it relevant — and a fresh agent can miss
# that judgement and act before loading the ledger (the exact drift ev exists to prevent). A hook is
# deterministic: the brief is in context before the agent does anything.
#
# Wire this into your agent runtime's session-start mechanism (a hook that runs a command at session
# start and adds its stdout to the session context — see settings.snippet.json for the config shape).
# Run it from the repo whose .evolving/ ledger you want loaded (co-located with the code — see
# ../scaffold/ev-colocate.sh).
set -eu

# No ledger here, or ev not installed → contribute nothing (silent, non-fatal).
[ -d .evolving ] || exit 0
command -v ev >/dev/null 2>&1 || exit 0

brief=$(ev brief 2>/dev/null) || exit 0
[ -n "$brief" ] || exit 0

printf 'Settled decisions in this repo (ev brief) — heed these; a rejected road is a hard do-not, and an\n'
printf 'agent does not re-litigate a ratified ruling:\n\n%s\n' "$brief"
