#!/usr/bin/env bash
# Prints a line per $2 of spend and stops when the team closes. Past $CAP
# (default 15) it kills the run's tmux session. Run it with no time limit:
# AFK's own budget is a time limit, not dollars.
# usage: CAP=75 bench/run/teamwire-watch.sh <run dir>
set -u
source "$(dirname "$0")/env.sh"
run=$1; name=$(basename "$run"); step=0; seen=0
while true; do
  usd=$(cd "$REPO" && python3 bench/cost.py --clone "$run/clone" --out-dir "$run" 2>/dev/null | sed -n 's/^total: \$//p')
  usd=${usd:-0}
  s=$(python3 -c "print(int(float('$usd')//2))")
  if [ "$s" -gt "$step" ]; then step=$s; echo "spend \$$usd"; fi
  if python3 -c "import sys; sys.exit(0 if float('$usd')>float('${CAP:-15}') else 1)"; then
    echo "OVER BUDGET \$$usd: killing tmux session $name"; tmux kill-session -t "$name"; date -u +%FT%TZ > "$run/end.txt"; exit 0
  fi
  st=$(TEAMWIRE_HOME=$run/twhome teamwire teams 2>&1 | head -1)
  # An AFK team prints as "(open, afk)".
  case "$st" in *"(open"*) seen=1;; esac
  if [ "$seen" = 1 ]; then case "$st" in *"(open"*) ;; *) echo "TEAM CLOSED \$$usd: $st"; date -u +%FT%TZ > "$run/end.txt"; exit 0;; esac; fi
  tmux has-session -t "$name" 2>/dev/null || { echo "SESSION GONE \$$usd"; exit 0; }
  sleep 60
done
