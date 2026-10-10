#!/bin/sh
# Architecture-phase check: every required document exists and carries its
# required sections. Structure only; the reviewer judges the content.
set -u
root="$(cd "$(dirname "$0")/.." && pwd)"
fail=0
need_file() { [ -s "$root/$1" ] || { echo "MISSING $1"; fail=1; }; }
need_head() { grep -qiE "^#+ .*$2" "$root/$1" 2>/dev/null || { echo "$1: no heading matching '$2'"; fail=1; }; }

need_file docs/requirements.md
need_head docs/requirements.md "invariant"
need_file docs/ARCHITECTURE.md
for h in "overview" "components" "data model" "message (integrity|lifecycle)" "stability|failure" "metrics|observability" "harness" "security|trust" "install"; do
  need_head docs/ARCHITECTURE.md "$h"
done
need_file docs/api/core.md
need_file docs/api/mcp.md
need_file docs/api/cli.md
need_file docs/failure-modes.md
need_file docs/metrics.md
need_file docs/test-strategy.md
need_file docs/build-plan.md
grep -qiE "\| *owns *\|" "$root/docs/build-plan.md" 2>/dev/null || { echo "docs/build-plan.md: no ticket table with an Owns column"; fail=1; }
grep -qiE "\| *check *\|" "$root/docs/build-plan.md" 2>/dev/null || { echo "docs/build-plan.md: no ticket table with a Check column"; fail=1; }
n=$(ls "$root"/docs/adr/*.md 2>/dev/null | wc -l | tr -d ' ')
[ "$n" -ge 5 ] || { echo "docs/adr: $n ADRs, need at least 5"; fail=1; }
[ $fail -eq 0 ] && echo "docs check: ok"
exit $fail
