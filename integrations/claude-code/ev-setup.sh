#!/usr/bin/env sh
# ev-setup — one-step setup of the ev usage loop for Claude Code.
#
# In a guarded working tree, sets up everything an agent needs to USE ev:
#   1. a CO-LOCATED ledger          — so bound checks see the code's real git state
#   2. the ev SKILL                 — where Claude Code discovers it (.claude/skills/ev/SKILL.md)
#   3. the session-start brief + pre-commit gate, wired locally
#
# Idempotent · non-destructive (backs up / refuses to clobber) · --dry-run · prints every change.
# Everything it writes is LOCAL setup on your machine (.claude/, .git/hooks/, .evolving/) — none of
# it belongs in, or comes from, the ev repo's history.
#
# Usage:  integrations/claude-code/ev-setup.sh [--dry-run] [target-working-tree]   (default: .)
set -eu

# Locate the kit relative to THIS script, so the argument is the TARGET, never the source.
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
KIT=$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd)
SKILL_SRC="$KIT/skills/ev/SKILL.md"
HOOK_SRC="$KIT/integrations/agent-hooks/ev-brief-sessionstart.sh"
PRECOMMIT_SRC="$KIT/integrations/agent-hooks/pre-commit"
COLOCATE="$KIT/integrations/scaffold/ev-colocate.sh"

DRY=0
TARGET=.
for a in "$@"; do
  case "$a" in
    --dry-run) DRY=1 ;;
    -h|--help) echo "usage: ev-setup.sh [--dry-run] [target-working-tree]"; exit 0 ;;
    -*) echo "ev-setup: unknown option '$a'" >&2; exit 2 ;;
    *) TARGET=$a ;;
  esac
done

[ -f "$SKILL_SRC" ] || { echo "ev-setup: kit not found ($SKILL_SRC) — run from an ev checkout." >&2; exit 1; }
[ -e "$TARGET/.git" ] || { echo "ev-setup: '$TARGET' is not a git working tree (co-location needs one)." >&2; exit 1; }
TARGET=$(CDPATH= cd -- "$TARGET" && pwd)

# Run a command, or just narrate it under --dry-run.
run() { if [ "$DRY" = 1 ]; then printf '   [dry-run] %s\n' "$*"; else sh -c "$*"; fi; }
# A success line — suppressed under --dry-run, where nothing actually happened (no false ✓).
ok()  { [ "$DRY" = 1 ] || printf '   ✓ %s\n' "$*"; }

echo "ev-setup → $TARGET"

# 1. co-locate the ledger (the structure rung — reuses the neutral scaffold)
echo "1. ledger (co-located)"
if [ -d "$TARGET/.evolving" ]; then
  echo "   .evolving/ present — kept"
else
  run "sh \"$COLOCATE\" \"$TARGET\" >/dev/null"
  ok "ev init + staleness_ref=local-head"
fi

# 2. install the skill where Claude Code discovers it
echo "2. skill → .claude/skills/ev/SKILL.md"
SKILL_DST="$TARGET/.claude/skills/ev/SKILL.md"
run "mkdir -p \"$TARGET/.claude/skills/ev\""
if [ -f "$SKILL_DST" ] && ! cmp -s "$SKILL_SRC" "$SKILL_DST"; then
  run "cp \"$SKILL_DST\" \"$SKILL_DST.bak\""
  echo "   (existing differed → backed up to SKILL.md.bak)"
fi
run "cp \"$SKILL_SRC\" \"$SKILL_DST\""
ok "installed"

# 3. hooks — session-start brief (heed) + pre-commit gate
echo "3. hooks"
run "mkdir -p \"$TARGET/.claude/hooks\""
run "cp \"$HOOK_SRC\" \"$TARGET/.claude/hooks/ev-brief-sessionstart.sh\""
run "chmod +x \"$TARGET/.claude/hooks/ev-brief-sessionstart.sh\""
SETTINGS="$TARGET/.claude/settings.json"
HOOK_PATH="$TARGET/.claude/hooks/ev-brief-sessionstart.sh"
# The command quotes the path so the shell that runs the hook doesn't word-split a path with spaces;
# in JSON those quotes are escaped (\"), valid JSON either way.
if [ -f "$SETTINGS" ]; then
  echo "   .claude/settings.json exists — not auto-editing JSON. Add this SessionStart command:"
  printf '     sh "%s"\n' "$HOOK_PATH"
elif [ "$DRY" = 1 ]; then
  printf '   [dry-run] write %s with a SessionStart hook → sh "%s"\n' "$SETTINGS" "$HOOK_PATH"
else
  cat > "$SETTINGS" <<JSON
{
  "hooks": {
    "SessionStart": [
      { "hooks": [ { "type": "command", "command": "sh \"$HOOK_PATH\"" } ] }
    ]
  }
}
JSON
  ok "session-start brief wired (.claude/settings.json)"
fi

PRECOMMIT_DST="$TARGET/.git/hooks/pre-commit"
if [ -f "$PRECOMMIT_DST" ]; then
  echo "   .git/hooks/pre-commit exists — not overwriting (gate source: $PRECOMMIT_SRC)"
else
  run "cp \"$PRECOMMIT_SRC\" \"$PRECOMMIT_DST\""
  run "chmod +x \"$PRECOMMIT_DST\""
  ok "pre-commit gate installed"
fi

if [ "$DRY" = 1 ]; then
  echo "(dry-run) — no changes made."
else
  echo "done — a fresh Claude Code session here loads ev brief; a commit gates on a broken bound check."
fi
