#!/usr/bin/env bash
# Opens an interactive Opus session for a /teamwire:afk run in a detached tmux
# session named after the run folder, with its own TEAMWIRE_HOME, so the run
# cannot read your real teamwire archive. Prints the run folder.
# Next: accept the folder-trust prompt in the session, then run teamwire-send.sh.
# usage: bench/run/teamwire-start.sh <task> <brief file> <setup line> <clone.py task flags...>
set -u
source "$(dirname "$0")/env.sh"
task=$1; brief=$2; extra=$3; shift 3
run=$(make_clone "$task" teamwire "$@") || exit 1
name=$(basename "$run"); mkdir -p "$run/twhome"
cp "$brief" "$run/brief.md"
{ printf '/teamwire:afk '; cat "$brief"; printf '\n%s\n' "$extra"; } > "$run/first-message.txt"
tmux new-session -d -s "$name" -x 220 -y 50 -c "$run/clone" \
  "env TEAMWIRE_HOME=$run/twhome claude --model opus --permission-mode bypassPermissions --settings $run/settings.json"
echo "$run"
