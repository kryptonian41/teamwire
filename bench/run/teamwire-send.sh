#!/usr/bin/env bash
# Pastes the run's first message into its tmux session as one bracketed paste,
# submits it, and records the start time.
# usage: bench/run/teamwire-send.sh <run dir>
set -u
run=$1; name=$(basename "$run")
tmux load-buffer -b first "$run/first-message.txt"
tmux paste-buffer -p -b first -t "$name"
sleep 2; tmux send-keys -t "$name" Enter
date -u +%FT%TZ > "$run/start.txt"
