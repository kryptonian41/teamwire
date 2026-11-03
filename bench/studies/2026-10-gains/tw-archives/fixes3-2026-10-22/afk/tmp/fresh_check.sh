#!/usr/bin/env bash
# Runs one member's setup and check on a fresh clone of bench, the way
# teamwire launch and teamwire verify run them: setup as `sh -c SCRIPT
# teamwire-setup <path>` in <path>, check as `sh -c CHECK` in <path>.
# Usage: fresh_check.sh <role> <setup> <check>
set -u
role="$1"
setup="$2"
check="$3"
afk=/private/tmp/claude-501/bnruns/bne68n09a4/twhome/teams/fixes3/afk
dir="$afk/tmp/$role"
log="$afk/tmp/$role.log"
res="$afk/tmp/$role.result"
: >"$log"
start=$SECONDS
{
  echo "== clone"
  git clone --quiet /private/tmp/claude-501/bnruns/bne68n09a4/clone "$dir" || exit 1
  cd "$dir" || exit 1
  git checkout --quiet bench || exit 1
  echo "HEAD $(git rev-parse HEAD)"
  echo "== setup: $setup"
} >>"$log" 2>&1
cd "$dir" || exit 1
sh -c "$setup" teamwire-setup "$dir" >>"$log" 2>&1
setup_rc=$?
echo "setup exit $setup_rc after $((SECONDS - start))s" >>"$log"
echo "== check: $check" >>"$log"
cstart=$SECONDS
if [ "$setup_rc" -eq 0 ]; then
  sh -c "$check" >>"$log" 2>&1
  check_rc=$?
else
  check_rc=skipped
fi
echo "check exit $check_rc after $((SECONDS - cstart))s" >>"$log"
echo "role=$role setup_rc=$setup_rc check_rc=$check_rc check_s=$((SECONDS - cstart)) head=$(git rev-parse HEAD)" >"$res"
