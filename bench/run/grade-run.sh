#!/usr/bin/env bash
# Leak scan, cost and hidden-test score for one finished run, into
# $BENCH_RESULTS/<task>/<setup>/. Needs the task's reference first:
#   python3 bench/grade.py reference <task flags> --out $BENCH_RESULTS/<task>/reference
# usage: bench/run/grade-run.sh <run dir> <task> <ref: branch with the result> <grade.py task flags...>
set -u
source "$(dirname "$0")/env.sh"
run=$1; task=$2; ref=$3; shift 3
setup=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["setup"])' "$run/run.json")
out=$BENCH_RESULTS/$task/$setup; mkdir -p "$out"
# A session may read its own transcript folder; that is no leak.
enc=$(echo "$run/clone" | tr '/.' '--'); allow=(); for d in ~/.claude/projects/${enc}*; do [ -d "$d" ] && allow+=(--allow "$d"); done
cd "$REPO"
echo "== leaks"
python3 bench/leaks.py --clone "$run/clone" --source "$REPO" --source ~/.teamwire/claude-plugin --runs-root "$BENCH_RUNS" ${allow[@]+"${allow[@]}"} > "$out/leaks.txt" 2>&1
echo "leak exit $?"; tail -15 "$out/leaks.txt"
echo "== cost"
cj=(); [ -f "$run/claude.json" ] && cj=(--claude-json "$run/claude.json")
python3 bench/cost.py --clone "$run/clone" ${cj[@]+"${cj[@]}"} --out-dir "$out" --name run > "$out/cost.txt" 2>&1
grep -E "^  claude|^total|difference" "$out/cost.txt"
echo "== score"
CARGO_TARGET_DIR=$BENCH_GRADE_TARGET python3 bench/grade.py score "$@" --run-dir "$run" --ref "$ref" \
  --expected "$BENCH_RESULTS/$task/reference/expected.json" --out "$out" 2>&1 | tail -6
