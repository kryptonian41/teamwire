#!/usr/bin/env bash
# Test run for the inner loop: the tests of the changed crates and of every
# workspace crate that depends on them, dev-dependencies included, and nothing
# else. The full gate (scripts/gate.sh) still runs at the end of each wave.
#   scripts/test-changed.sh <crate>...      those crates and their dependents
#   scripts/test-changed.sh                 the crates with files that differ
#                                           from the base (committed, staged,
#                                           unstaged and untracked): $TEAMWIRE_BASE,
#                                           else main, else origin/main
#   scripts/test-changed.sh --file PATH...  take PATH as a changed file
#                                           instead of asking git; repeat
#                                           the flag for more files
#   scripts/test-changed.sh --dry-run ...   print the package list and the
#                                           commands, build nothing
# A changed file tests the member whose directory holds it. It also tests
# each member that reads it from outside that member's directory. A member
# reads a path when one of its .rs files holds a string literal that starts
# with ../ (or /../, or {}/../ in a format string) and that literal, resolved against the file's folder or
# against the member's folder, names a file or folder outside the member's
# folder that exists or is itself a changed file. A changed file inside such
# a folder counts too. A renamed file counts under its old and its new name,
# so a deleted or renamed file still tests the member that names it. The script
# works this out from the code on every run, so no list of crates is kept by
# hand. Its limits: it cannot see a path that code builds in pieces, such as
# join("examples").join("presets"), so give such a path one literal.
# Tests run under nextest, as in the gate (docs/conventions.md section 2).
# Written for bash 3.2 (macOS), so no mapfile and no associative arrays.
set -u

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root" || exit 1

usage() {
  echo "usage: scripts/test-changed.sh [--dry-run] [--file PATH]... | [--dry-run] [crate...]" >&2
  exit 3
}

dry=0
args=""
given=""
while [ $# -gt 0 ]; do
  case "$1" in
    --dry-run) dry=1 ;;
    --file)
      [ $# -ge 2 ] && [ -n "$2" ] || usage
      given="$given
${2#./}"
      shift
      ;;
    -*) usage ;;
    *) args="$args $1" ;;
  esac
  shift
done
# A crate name and a changed file are two ways to say what changed; take one.
if [ -n "$args" ] && [ -n "$given" ]; then
  usage
fi

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

# The string literals of Rust code, read from the files named on the command
# line. Each literal that starts with ../ (or /../, as after
# concat!(env!("CARGO_MANIFEST_DIR"), ...), or {}/../, as in a format string
# given CARGO_MANIFEST_DIR) is resolved against the folder of
# its file and against the member's folder, and each result outside the
# member's folder is printed as "<package> <path>". Comments, char literals
# and raw strings are skipped or read as Rust reads them, so a quote inside
# another literal starts nothing.
scan='
function norm(p,   n, parts, out, i, k) {
  n = split(p, parts, "/")
  k = 0
  for (i = 1; i <= n; i++) {
    if (parts[i] == "" || parts[i] == ".") continue
    if (parts[i] == "..") {
      if (k == 0) return ""
      k--
      continue
    }
    out[++k] = parts[i]
  }
  p = ""
  for (i = 1; i <= k; i++) p = (i == 1) ? out[i] : p "/" out[i]
  return p
}
function emit(p) {
  if (p == "") return
  # Inside the member, or a folder that holds the member: neither is a file
  # outside it.
  if (p == member || index(p, member "/") == 1) return
  if (index(member "/", p "/") == 1) return
  print pkg, p
}
function literal(s,   dir) {
  # "{}/../x" is a format string whose {} holds the folder of the member.
  if (substr(s, 1, 3) == "{}/") s = substr(s, 3)
  if (substr(s, 1, 1) == "/") s = substr(s, 2)
  if (substr(s, 1, 3) != "../") return
  dir = FILENAME
  sub(/\/[^\/]*$/, "", dir)
  emit(norm(dir "/" s))
  emit(norm(member "/" s))
}
FNR == 1 { state = "code" }
{
  line = $0
  n = length(line)
  i = 1
  while (i <= n) {
    if (state == "code") {
      c = substr(line, i, 1)
      rest = substr(line, i)
      if (rest ~ /^\/\//) break
      if (rest ~ /^\/\*/) { state = "block"; i += 2; continue }
      if (match(rest, /^[A-Za-z_][A-Za-z0-9_]*/)) {
        word = substr(rest, 1, RLENGTH)
        after = substr(rest, RLENGTH + 1)
        if ((word == "r" || word == "br") && match(after, /^#*"/)) {
          hashes = substr(after, 1, RLENGTH - 1)
          state = "raw"
          buf = ""
          i += length(word) + RLENGTH
          continue
        }
        i += length(word)
        continue
      }
      if (c == q) {
        if (substr(line, i + 1, 1) == "\\") {
          j = index(substr(line, i + 2), q)
          i += (j > 0) ? j + 2 : 1
        } else if (substr(line, i + 2, 1) == q) {
          i += 3
        } else {
          i++
        }
        continue
      }
      if (c == "\"") { state = "str"; buf = ""; i++; continue }
      i++
    } else if (state == "str") {
      c = substr(line, i, 1)
      if (c == "\\") { buf = buf substr(line, i, 2); i += 2; continue }
      if (c == "\"") { literal(buf); state = "code"; i++; continue }
      buf = buf c
      i++
    } else if (state == "raw") {
      j = index(substr(line, i), "\"" hashes)
      if (j == 0) { buf = buf substr(line, i); break }
      buf = buf substr(line, i, j - 1)
      literal(buf)
      state = "code"
      i += j + length(hashes)
    } else {
      j = index(substr(line, i), "*/")
      if (j == 0) break
      state = "code"
      i += j + 1
    }
  }
  if (state == "str" || state == "raw") buf = buf "\n"
}
'

# "<package> <path>" for each file or folder outside a member's directory
# that a string literal of the member's Rust code names, and that exists or is
# one of the changed files in $files. A deleted or renamed file no longer
# exists, yet its reader must still be tested.
reads_outside() {
  echo "$members" | while read -r dir name; do
    [ -n "$dir" ] || continue
    rs="$(grep -rlF --include='*.rs' '../' "$dir")"
    [ -n "$rs" ] || continue
    # shellcheck disable=SC2086
    awk -v member="$dir" -v pkg="$name" -v q="'" "$scan" $rs
  done | sort -u | while read -r pkg path; do
    if [ -e "$path" ] || echo "$files" | grep -qxF -e "$path"; then
      echo "$pkg $path"
    fi
  done
}

gate_hint="test-changed: a change to the root Cargo.toml, Cargo.lock or toolchain needs scripts/gate.sh"
build_file=0
changed=""
if [ -n "$args" ]; then
  for p in $args; do
    if ! is_member "$p"; then
      echo "test-changed: '$p' is not a workspace package" >&2
      exit 3
    fi
    changed="$changed $p"
  done
elif [ -n "$given" ]; then
  files="$given"
else
  # A failing git command must not read as "no changed files": git prints its
  # own message to stderr, and the script stops.
  # The ref to compare against: $TEAMWIRE_BASE, else a local main, else
  # origin/main (a fresh clone has only the remote branch).
  base="${TEAMWIRE_BASE:-}"
  if [ -z "$base" ]; then
    if git rev-parse --verify --quiet refs/heads/main >/dev/null; then
      base=main
    elif git rev-parse --verify --quiet refs/remotes/origin/main >/dev/null; then
      base=origin/main
    else
      echo "test-changed: no local main and no origin/main; set TEAMWIRE_BASE to the ref to compare against" >&2
      exit 1
    fi
  fi
  echo "test-changed: base: $base"
  base="$(git merge-base "$base" HEAD)" || exit 1
  # --no-renames lists a rename as its old and its new name.
  tracked="$(git diff --name-only --no-renames "$base")" || exit 1
  untracked="$(git ls-files --others --exclude-standard)" || exit 1
  files="$tracked
$untracked"
fi
if [ -z "$args" ]; then
  # A file belongs to the member whose directory prefixes its path. A member
  # also reads files outside its directory, by include_str!, #[path] or a
  # path joined to CARGO_MANIFEST_DIR. reads_outside lists those files.
  reads="$(reads_outside)"
  changed="$(
    echo "$files" | while read -r f; do
      [ -n "$f" ] || continue
      echo "$members" | awk -v f="$f" 'NF == 2 && index(f, $1 "/") == 1 { print $2 }'
      echo "$reads" | awk -v f="$f" 'NF == 2 && (f == $2 || index(f, $2 "/") == 1) { print $1 }'
    done | sort -u
  )"
  if [ -z "$changed" ]; then
    echo "test-changed: no changed file belongs to a workspace crate; nothing to test"
    echo "$gate_hint"
    exit 0
  fi
  # A root build file can change how every crate builds, and the crates the
  # script tests are not all of them, so it says to run the full gate too.
  if echo "$files" | grep -qxF -e Cargo.toml -e Cargo.lock \
    -e rust-toolchain.toml -e .cargo/config.toml; then
    build_file=1
  fi
fi

# Reverse dependencies inside the workspace. A line of `cargo tree -i` that
# carries a path in parentheses is a workspace package; registry crates have none.
packages=""
for p in $changed; do
  tree="$(cargo tree -i "$p" -e all --prefix none --locked --offline)" || exit 1
  packages="$packages
$(echo "$tree" | grep ' (/' | awk '{ print $1 }')"
done
packages="$(echo "$packages" | sort -u)"
if [ -z "$packages" ]; then
  echo "test-changed: cargo tree found no packages for:$changed" >&2
  exit 1
fi

# The end-to-end and crash tests run the `teamwire` binary, so they test every
# crate the CLI contains. They have no Cargo dependency on teamwire-cli, so
# cargo tree cannot see that link; add them by hand.
case "
$packages
" in
  *"
teamwire-cli
"*) packages="$packages
teamwire-e2e
teamwire-crash-tests" ;;
esac

packages="$(echo "$packages" | sort -u)"
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
# The printed commands and the run commands are the same variables.
build="cargo build --locked --all-targets$pflags$extra"
# --no-tests=warn: a changed crate can have no tests, and that is not a failure.
test="cargo nextest run --locked --no-tests=warn$pflags"

echo "test-changed: changed: $(echo $changed)"
echo "test-changed: packages: $(echo $packages)"
echo "test-changed: $build"
echo "test-changed: TEAMWIRE_FAKES_PREBUILT=1 $test"
if [ "$build_file" -eq 1 ]; then
  echo "$gate_hint"
fi
if [ "$dry" -eq 1 ]; then
  exit 0
fi

# shellcheck disable=SC2086
$build || exit 1
# shellcheck disable=SC2086
TEAMWIRE_FAKES_PREBUILT=1 $test
