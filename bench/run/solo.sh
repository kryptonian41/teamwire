#!/usr/bin/env bash
# One Claude Code session on Opus in print mode.
# usage: bench/run/solo.sh <task> <solo|subagents> <brief file> <budget usd> <clone.py task flags...>
set -u
source "$(dirname "$0")/env.sh"
task=$1; setup=$2; brief=$3; budget=$4; shift 4
case $setup in solo) block=(Agent Workflow);; subagents) block=(Workflow);; *) echo "setup must be solo or subagents" >&2; exit 2;; esac
run=$(make_clone "$task" "$setup" "$@") || exit 1
echo "run dir: $run"
cp "$brief" "$run/brief.md"
cd "$run/clone"
date -u +%FT%TZ > "$run/start.txt"
# --disallowedTools takes a list, so the brief goes on standard input.
"$TIMEOUT" 10800 claude -p --model opus --max-budget-usd "$budget" --output-format json \
  --settings "$run/settings.json" --permission-mode bypassPermissions \
  --disallowedTools "${block[@]}" < "$brief" > "$run/claude.json" 2> "$run/claude.err"
echo "exit $?" > "$run/exit.txt"
date -u +%FT%TZ > "$run/end.txt"
python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); print("cost",d.get("total_cost_usd"),"turns",d.get("num_turns"),"subtype",d.get("subtype")); print((d.get("result") or "")[-800:])' "$run/claude.json"
