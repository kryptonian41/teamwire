#!/usr/bin/env bash
# The full gate of docs/conventions.md section 2: six steps, in order, one
# line per step with its wall time, stopping at the first failure.
#   scripts/gate.sh        all six steps
#   scripts/gate.sh fast   steps 1-3
# A failing step's output is printed after its line. A passing step still
# prints its WEAKENED and PROTECTED-CHANGED lines, because tidy reports those
# and exits 0. Timing uses bash's $SECONDS, because macOS has no GNU date
# under that name.
set -u

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root" || exit 1

case "${1:-full}" in
  full) last=6 ;;
  fast) last=3 ;;
  *) echo "usage: scripts/gate.sh [fast]" >&2; exit 3 ;;
esac

log="$(mktemp -t teamwire-gate.XXXXXX)"
trap 'rm -f "$log"' EXIT

step() {
  local n="$1" name="$2"
  shift 2
  local start=$SECONDS
  if "$@" >"$log" 2>&1; then
    echo "gate $n/$last $name: ok ($((SECONDS - start))s)"
    grep -E '^(WEAKENED|PROTECTED-CHANGED) ' "$log" || true
  else
    echo "gate $n/$last $name: FAILED ($((SECONDS - start))s): $*"
    cat "$log"
    exit 1
  fi
}

step 1 fmt cargo fmt --all -- --check
step 2 tidy cargo xtask tidy --base main
step 3 clippy cargo clippy --workspace --all-targets --locked -- -D warnings
if [ "$last" -eq 3 ]; then
  exit 0
fi
# cargo build, not cargo test, writes the dep-info files that the prebuilt
# fakes are checked against (tests/fakes/src/bins.rs), so a fresh checkout
# needs it before the tests skip their own rebuild of the fakes.
step 4 test sh -c 'cargo build --workspace --locked && TEAMWIRE_FAKES_PREBUILT=1 cargo test --workspace --locked'
step 5 doc env RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
step 6 docs scripts/check-docs.sh
