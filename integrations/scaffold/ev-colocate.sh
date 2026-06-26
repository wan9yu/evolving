#!/usr/bin/env sh
# ev-colocate — set up an ev ledger CO-LOCATED with a guarded working tree.
#
# WHY THIS EXISTS (the drift class it eliminates): `ev` derives ALL git state — staleness sha,
# receipt commit, the triggered-by diff — from the directory it runs in, the SAME root as the
# ledger (`.evolving/`). So if the ledger lives in a SEPARATE repo from the guarded code (e.g. a
# pull-only, auto-committed copy), every git call reads the LEDGER's HEAD, never the code's real
# working state — and bound checks stall at stale/not-run, never firing red on a real change.
# (The catch-loop only fires when ev can SEE the guarded code's real git state.) This scaffold makes
# the right layout the only layout: the ledger is born at the root of the guarded working tree, so
# the wrong setup is unrepresentable.
#
# Usage:  ev-colocate.sh <path-to-guarded-repo>
set -eu

GUARDED="${1:-.}"

if [ ! -e "$GUARDED/.git" ]; then
  echo "ev-colocate: '$GUARDED' is not a git working tree." >&2
  echo "  Co-location is meaningless without one — ev reads staleness + receipts from the working" >&2
  echo "  tree at the ledger root. Point this at the guarded repo's checkout (a real working copy)." >&2
  exit 1
fi

cd "$GUARDED"
ROOT=$(pwd)

if [ -d .evolving ]; then
  echo "ev-colocate: .evolving/ already present at $ROOT — already co-located."
else
  ev init >/dev/null
  echo "✓ ev ledger initialized, co-located at $ROOT/.evolving"
fi

# Track the WORKING HEAD (not @{upstream}): a local, not-yet-pushed edit is the live origin, so a
# bound check goes red on a real working change rather than reading stale against a remote ref.
# (`ev init` seeds staleness_ref = "live-origin"; for a catch-loop trial you want local-head.)
CONFIG=.evolving/config
if grep -q '^staleness_ref' "$CONFIG" 2>/dev/null; then
  sed -i.bak 's/^staleness_ref.*/staleness_ref = local-head/' "$CONFIG" && rm -f "$CONFIG.bak"
else
  printf 'staleness_ref = local-head\n' >>"$CONFIG"
fi
echo "✓ staleness_ref = local-head (the catch-loop tracks your working HEAD)"

# Self-check: prove ev can see this working tree's git state from here (the co-location invariant).
if ev verify >/dev/null 2>&1; then
  echo "✓ ev sees the working tree from $ROOT — bound checks will reflect real changes"
fi

cat <<EOF

Co-located. Run ev FROM HERE ($ROOT):
  ev decide "<ruling>" --assume "<premise>" \\
     --assume-test "<a test that PASSES here>" \\
     --counter-test "<a NEGATIVE control that FAILS on this clean state>" \\
     --on-platform local --triggered-by <file> --surface <name> \\
     --verified-at-sha \$(git rev-parse HEAD) --blame "<you>"
  ev check --run --platform local        # green now
  # break the premise (edit the guarded file), then:
  ev check --run --platform local --exit-on-red   # RED — the catch-loop fires
EOF
