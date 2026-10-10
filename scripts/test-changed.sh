#!/usr/bin/env bash
# Test run for the inner loop: the tests of the changed crates and of every
# workspace crate that depends on them, dev-dependencies included, and nothing
# else. The full gate (scripts/gate.sh) still runs at the end of each wave.
#   scripts/test-changed.sh <crate>...      those crates and their dependents
#   scripts/test-changed.sh                 the crates with files that differ
#                                           from main (committed, staged,
#                                           unstaged and untracked)
#   scripts/test-changed.sh --dry-run ...   print the package list and the
#                                           commands, build nothing
# Plain `cargo test`, not nextest: see docs/conventions.md section 2. Written
# for bash 3.2 (macOS), so no mapfile and no associative arrays.
set -u

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root" || exit 1

dry=0
args=""
for a in "$@"; do
  case "$a" in
    --dry-run) dry=1 ;;
    -*) echo "usage: scripts/test-changed.sh [--dry-run] [crate...]" >&2; exit 3 ;;
    *) args="$args $a" ;;
  esac
done

# "<dir> <package name>" for each workspace member, read from its Cargo.toml.
members=""
for toml in crates/*/Cargo.toml tests/fakes/Cargo.toml tests/crash/Cargo.toml \
  tests/e2e/Cargo.toml xtask/Cargo.toml; do
  [ -f "$toml" ] || continue
  name="$(sed -n 's/^name = "\(.*\)"$/\1/p' "$toml" | head -1)"
  members="$members
$(dirname "$toml") $name"
done

is_member() {
  echo "$members" | awk -v n="$1" '$2 == n { found = 1 } END { exit !found }'
}

changed=""
if [ -n "$args" ]; then
  for p in $args; do
    if ! is_member "$p"; then
      echo "test-changed: '$p' is not a workspace package" >&2
      exit 3
    fi
    changed="$changed $p"
  done
else
  base="$(git merge-base main HEAD 2>/dev/null || echo main)"
  files="$(
    git diff --name-only "$base"
    git ls-files --others --exclude-standard
  )"
  # A file belongs to the member whose directory prefixes its path.
  changed="$(
    echo "$files" | while read -r f; do
      [ -n "$f" ] || continue
      echo "$members" | awk -v f="$f" 'NF == 2 && index(f, $1 "/") == 1 { print $2 }'
    done | sort -u
  )"
  if [ -z "$changed" ]; then
    echo "test-changed: no changed file belongs to a workspace crate; nothing to test"
    echo "test-changed: a change to the root Cargo.toml, Cargo.lock or toolchain needs scripts/gate.sh"
    exit 0
  fi
fi

# Reverse dependencies inside the workspace. A line of `cargo tree -i` that
# carries a path in parentheses is a workspace package; registry crates have none.
packages="$(
  for p in $changed; do
    cargo tree -i "$p" -e all --prefix none --locked --offline 2>/dev/null |
      grep ' (/' | awk '{ print $1 }'
  done | sort -u
)"
if [ -z "$packages" ]; then
  echo "test-changed: cargo tree found no packages for:$changed" >&2
  exit 1
fi

packages="$(echo $packages)"
pflags=""
for p in $packages; do
  pflags="$pflags -p $p"
done

# One build for the tests, the fake binaries and the CLI binary, so the
# features match and the tests need not rebuild them (tests/fakes/src/bins.rs).
extra=""
for p in teamwire-fakes teamwire-cli; do
  case " $packages " in
    *" $p "*) ;;
    *) extra="$extra -p $p" ;;
  esac
done
build="cargo build --locked --all-targets$pflags$extra"
test="env TEAMWIRE_FAKES_PREBUILT=1 cargo test --locked$pflags"

echo "test-changed: changed: $(echo $changed)"
echo "test-changed: packages: $(echo $packages)"
echo "test-changed: $build"
echo "test-changed: $test"
if [ "$dry" -eq 1 ]; then
  exit 0
fi

$build || exit 1
# shellcheck disable=SC2086
TEAMWIRE_FAKES_PREBUILT=1 cargo test --locked $pflags
