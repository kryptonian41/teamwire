#!/usr/bin/env python3
"""List candidate task commits for the pilot benchmark (plan 5.1).

Read only: it runs `git log`, `git diff` and `git show` in the repository and writes one CSV.
A commit qualifies when every test it adds or changes is in a test-only file.
Whether a brief can name the interface in one line is the lead's judgement and is not checked.
"""
import argparse
import csv
import os
import re
import sys

import common

COLUMNS = ["sha", "date", "subject", "crates", "test_files", "other_files", "lines", "test_command"]


def numstat(repo, sha):
    out = common.git(repo, "diff", "--numstat", "--no-renames", sha + "^", sha)
    files = []
    for line in out.splitlines():
        a, d, path = line.split("\t", 2)
        files.append((path, 0 if a == "-" else int(a), 0 if d == "-" else int(d)))
    return files


def touches_inline_tests(repo, sha, path):
    """True when the diff of a non-test-only Rust file edits its unit-test section (from the first #[cfg(test)])."""
    if not path.endswith(".rs"):
        return False
    new = common.git(repo, "show", "%s:%s" % (sha, path), check=False).splitlines()
    start = next((i + 1 for i, l in enumerate(new) if l.strip().startswith("#[cfg(test)]")), None)
    if start is None:
        # A file of tests that is not under tests/ (such as src/x/tests.rs) is not test-only either.
        return any(l.strip() == "#[test]" for l in new) and os.path.basename(path) == "tests.rs"
    diff = common.git(repo, "diff", "-U0", "--no-renames", sha + "^", sha, "--", path)
    for m in re.finditer(r"^@@ -\S+ \+(\d+)(?:,(\d+))? @@", diff, re.M):
        first, count = int(m.group(1)), int(m.group(2) if m.group(2) is not None else 1)
        # A pure deletion (count 0) sits after line `first`.
        if max(first, first + count - 1) >= start:
            return True
    # An old-side-only edit at the very end of the file still counts when it removed test lines.
    return bool(re.search(r"^-\s*(#\[test\]|#\[cfg\(test\)\])", diff, re.M))


def crate_of(path):
    m = re.match(r"crates/([^/]+)/", path)
    return m.group(1) if m else ("ui" if path.startswith("ui/") else None)


def package_name(repo, sha, crate):
    m = re.search(r'^name\s*=\s*"([^"]+)"', common.git(repo, "show", "%s:crates/%s/Cargo.toml" % (sha, crate), check=False), re.M)
    return m.group(1) if m else crate


def candidate(repo, sha, lo, hi):
    files = numstat(repo, sha)
    lines = sum(a + d for _, a, d in files)
    if not lo <= lines <= hi:
        return None
    paths = [p for p, _, _ in files]
    if any(common.is_protected(p) for p in paths):
        return None
    tests = [p for p in paths if common.is_test_only(p)]
    if not tests:
        return None
    others = [p for p in paths if p not in tests]
    if any(touches_inline_tests(repo, sha, p) for p in others):
        return None
    crates = sorted({crate_of(p) for p in paths if crate_of(p)})
    test_crates = sorted({crate_of(p) for p in tests})
    cmds = ["cargo test -p %s --locked" % package_name(repo, sha, c) for c in test_crates if c != "ui"]
    ui_tests = [p[3:] for p in tests if p.startswith("ui/")]
    if ui_tests:
        cmds.append("cd ui && bun test " + " ".join(ui_tests))
    date, subject = common.git(repo, "log", "-1", "--format=%cs%x00%s", sha).rstrip("\n").split("\0", 1)
    return {"sha": sha[:12], "date": date, "subject": subject, "crates": ",".join(crates),
            "test_files": " ".join(tests), "other_files": " ".join(others), "lines": lines,
            "test_command": " && ".join(cmds)}


def main(argv=None):
    here = os.path.dirname(os.path.abspath(__file__))
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--repo", default=os.path.dirname(here), help="repository to read (default: this one)")
    ap.add_argument("--branch", default="develop")
    ap.add_argument("--after-tag", default="v0.1.0", help="only commits newer than this tag")
    ap.add_argument("--min-lines", type=int, default=30)
    ap.add_argument("--max-lines", type=int, default=200)
    ap.add_argument("--out", default=os.path.join(here, "results", "candidates.csv"))
    a = ap.parse_args(argv)

    shas = common.git(a.repo, "rev-list", "--no-merges", "%s..%s" % (a.after_tag, a.branch)).split()
    rows = []
    for sha in shas:
        if common.git(a.repo, "rev-list", "--parents", "-n1", sha).count(" ") != 1:
            continue  # a root commit has no parent to diff against
        c = candidate(a.repo, sha, a.min_lines, a.max_lines)
        if c:
            rows.append(c)
    os.makedirs(os.path.dirname(os.path.abspath(a.out)), exist_ok=True)
    with open(a.out, "w", newline="") as f:
        w = csv.DictWriter(f, COLUMNS)
        w.writeheader()
        w.writerows(rows)
    print("%d of %d commits qualify; written to %s" % (len(rows), len(shas), a.out))
    for r in rows:
        print("%s %s %-9s %3d lines  %s\n    tests: %s\n    other: %s\n    run:   %s" % (
            r["sha"], r["date"], r["crates"], r["lines"], r["subject"], r["test_files"], r["other_files"], r["test_command"]))
    return 0


if __name__ == "__main__":
    sys.exit(main())
