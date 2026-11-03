# Shared settings for the run scripts. Sourced, never run.
# BENCH_RUNS must be a folder outside this repository: a run must not see the
# source checkout, and each run gets a fresh bn<random> folder under it.
BENCH="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPO="$(dirname "$BENCH")"
: "${BENCH_RUNS:?set BENCH_RUNS to a folder outside the repository for run clones}"
BENCH_RESULTS="${BENCH_RESULTS:-$BENCH/results}"
# Grading builds every hidden test; a shared target keeps later grades warm.
BENCH_GRADE_TARGET="${BENCH_GRADE_TARGET:-$BENCH_RUNS/grade-target}"
TIMEOUT="$(command -v gtimeout || command -v timeout)" || { echo "need GNU timeout (brew install coreutils)" >&2; exit 1; }

# make_clone <task> <setup> <clone.py flags...>: prints the run folder.
make_clone() {
  local task=$1 setup=$2 out run; shift 2
  out=$(cd "$REPO" && python3 bench/clone.py "$@" --task "$task" --setup "$setup" --root "$BENCH_RUNS") || return 1
  run=$(dirname "$(echo "$out" | sed -n 's/^run.json: //p')")
  # A gate older than TEAMWIRE_BASE compares against a local main.
  git -C "$run/clone" branch main
  echo "$run"
}
