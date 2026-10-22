#!/usr/bin/env python3
"""Grade benchmark runs (plan 5.4).

  reference  run a task's hidden tests in a clone at the original commit; the tests that pass are the expected set
  score      grade a finished run: outside files, fast gate, hidden tests against the expected set, diff for the blind grader
  blind      write the blind grader's inputs: one file per diff named by a random id, a rubric, a shuffled order, a key

Results go under --out (a folder you give); the source repository and the run's clone are never committed to.
"""
import argparse
import json
import os
import random
import re
import shutil
import subprocess
import sys
import tempfile

import clone as clonemod
import common

ANSI = re.compile(r"\x1b\[[0-9;]*m")
GATE_FLAGS = re.compile(r"WEAKENED|PROTECTED-CHANGED")
# Files that teamwire itself puts in a clone; they are not the agent's work.
TEAMWIRE_EXCLUDES = [":(exclude).worktrees", ":(exclude)TASK.md", ":(exclude)reports"]

RUBRIC = """# Rubric

Score each diff on three scales, 0 to 3. You see the brief and the diff only.

## Correctness: does the change do what the brief asks?
- 0: The change does not do what the brief asks, or breaks existing behaviour.
- 1: The change does part of the work, or has a clear bug in a main path.
- 2: The change does the work, with a small gap or a bug in an edge case.
- 3: The change does everything the brief asks, and the edge cases are handled.

## Scope: does the change touch only what the task needs?
- 0: Most of the diff is unrelated to the brief.
- 1: A large part of the diff is unrelated, or a protected or generated file is edited.
- 2: A few lines are unrelated to the brief but harmless.
- 3: Every changed line serves the brief.

## Test quality: would the tests catch a wrong change?
- 0: The change adds no tests, or tests that cannot fail.
- 1: The tests run the code but check little, or check the wrong thing.
- 2: The tests check the main behaviour but miss an edge case.
- 3: The tests fail without the change and cover the main behaviour and the edge cases.

Answer with a JSON object per diff: {"id": "...", "correctness": n, "scope": n, "test_quality": n, "reason": "one sentence"}.
"""


# ---------- hidden tests: which files, which tests ----------

def name_status(repo, sha):
    """[(status, path)] for a commit against its parent."""
    out = common.git(repo, "diff", "--name-status", "--no-renames", sha + "^", sha)
    return [tuple(l.split("\t", 1)) for l in out.splitlines()]


def hidden_spec(repo, commits):
    """Test-only files the commits add or change (the hidden tests), and what runs them.

    Returns {"files": [(commit, path)], "crates": {dir: [path]}, "ui_files": [path relative to ui/]}.
    A file changed by several commits is taken from the last one.
    """
    last = {}
    for sha in commits:
        for status, path in name_status(repo, sha):
            if common.is_test_only(path) and status != "D":
                last[path] = sha
    files = sorted(((c, p) for p, c in last.items()), key=lambda cp: cp[1])
    crates, ui = {}, []
    for _, p in files:
        if p.startswith("ui/"):
            ui.append(p[3:])
        else:
            crates.setdefault(p.split("/")[1], []).append(p)
    return {"files": files, "crates": crates, "ui_files": sorted(ui)}


def changed_by_commits(repo, commits):
    return {p for sha in commits for _, p in name_status(repo, sha)}


def copy_hidden(repo, spec, dest):
    for sha, path in spec["files"]:
        target = os.path.join(dest, path)
        os.makedirs(os.path.dirname(target), exist_ok=True)
        data = subprocess.run(["git", "-C", repo, "show", "%s:%s" % (sha, path)], stdout=subprocess.PIPE, check=True).stdout
        with open(target, "wb") as f:
            f.write(data)


def package_name(repo, sha, crate):
    m = re.search(r'^name\s*=\s*"([^"]+)"', common.git(repo, "show", "%s:crates/%s/Cargo.toml" % (sha, crate), check=False), re.M)
    return m.group(1) if m else crate


def hidden_binaries(paths):
    """Integration-test binaries (as cargo prints them) that hold hidden tests, or None for all of them.

    A top-level tests/NAME.rs is one binary. A file in a sub-folder (support code, a multi-file binary)
    may feed any binary, so every integration binary of the crate counts as hidden.
    """
    out = set()
    for p in paths:
        rest = p.split("/tests/", 1)[1]
        if "/" in rest:
            return None
        out.add("tests/" + rest)
    return out


# ---------- parsers ----------

def parse_cargo(text, crate):
    """`cargo test` output -> {"crate::binary::test": "ok"|"FAILED"|"ignored"}."""
    results, binary = {}, None
    for line in ANSI.sub("", text).splitlines():
        m = re.match(r"\s*Running (unittests )?(\S+)", line)
        if m:
            binary = ("unittests " if m.group(1) else "") + m.group(2)
            continue
        if re.match(r"\s*Doc-tests ", line):
            binary = "doctests"
            continue
        m = re.match(r"test (.+?) \.\.\. (ok|FAILED|ignored)\b", line)
        if m and binary:
            results["%s::%s::%s" % (crate, binary, m.group(1))] = m.group(2)
    return results


def parse_bun(text):
    """`bun test` output -> {"file::test": "pass"|"fail"|"skip"}."""
    results, current = {}, None
    for line in ANSI.sub("", text).splitlines():
        m = re.match(r"^(\S.*\.test\.tsx?):$", line)
        if m:
            current = m.group(1)
            continue
        m = re.match(r"^\((pass|fail|skip|todo)\) (.+?)(?: \[[\d.]+m?s\])?$", line)
        if m and current:
            results["%s::%s" % (current, m.group(2))] = "skip" if m.group(1) == "todo" else m.group(1)
    return results


def passed(status):
    return status in ("ok", "pass")


def score_tests(expected, results):
    """Each expected test is pass or fail; one missing from the output (or ignored, or in a crate that did not compile) fails."""
    return {t: ("pass" if passed(results.get(t)) else "fail") for t in expected}


# ---------- running things ----------

def run(args, cwd, out_file, timeout, env=None):
    """Run a command with an argument list, keep the raw output, return (exit code or None on timeout, text)."""
    try:
        r = subprocess.run(args, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, universal_newlines=True,
                           timeout=timeout, env=env)
        code, text = r.returncode, r.stdout
    except subprocess.TimeoutExpired as e:
        code, text = None, (e.stdout or "") + "\n[timed out after %ss]\n" % timeout
        if isinstance(text, bytes):
            text = text.decode(errors="replace")
    with open(out_file, "w") as f:
        f.write(text)
    return code, text


def run_hidden(repo, commits, clone, spec, out, timeout):
    """Run every crate's tests (and bun for ui) in `clone`; return the hidden tests' results."""
    results, last = {}, commits[-1]
    for crate, paths in sorted(spec["crates"].items()):
        pkg = package_name(repo, last, crate)
        _, text = run(["cargo", "test", "-p", pkg, "--locked", "--no-fail-fast"], clone,
                      os.path.join(out, "raw-cargo-%s.txt" % pkg), timeout)
        wanted = hidden_binaries(paths)
        for tid, status in parse_cargo(text, pkg).items():
            binary = tid.split("::")[1]
            if binary.startswith("tests/") and (wanted is None or binary in wanted):
                results[tid] = status
    if spec["ui_files"]:
        ui = os.path.join(clone, "ui")
        run(["bun", "install", "--frozen-lockfile"], ui, os.path.join(out, "raw-bun-install.txt"), timeout)
        _, text = run(["bun", "test"] + spec["ui_files"], ui, os.path.join(out, "raw-bun-test.txt"), timeout)
        results.update(parse_bun(text))
    return results


# ---------- the run's working tree ----------

def snapshot(clone, parent, diff_args):
    """git diff of the clone's whole working tree against `parent`: committed, staged, unstaged and
    untracked files, ignored files left out. A temporary index keeps the clone's own index untouched."""
    with tempfile.TemporaryDirectory() as d:
        env = dict(os.environ, GIT_INDEX_FILE=os.path.join(d, "index"))
        common.git(clone, "read-tree", parent, env=env)
        common.git(clone, "add", "-A", "--", ".", *TEAMWIRE_EXCLUDES, env=env)
        return common.git(clone, "diff", "--cached", "--no-renames", *diff_args, parent, env=env)


def outside_files(clone, parent, original_paths):
    changed = snapshot(clone, parent, ["--name-only"]).split("\n")
    return sorted(p for p in changed if p and p not in original_paths)


def gate(clone, parent, out, timeout):
    # TEAMWIRE_BASE: a bundle clone has no main or origin/main, and the gate exits 1 without it.
    code, text = run(["scripts/gate.sh", "fast"], clone, os.path.join(out, "raw-gate.txt"), timeout,
                     env=dict(os.environ, TEAMWIRE_BASE=parent))
    return {"exit": code, "flagged_lines": [l.strip() for l in ANSI.sub("", text).splitlines() if GATE_FLAGS.search(l)]}


# ---------- commands ----------

def cmd_reference(a):
    out = os.path.abspath(a.out)
    os.makedirs(out, exist_ok=True)
    spec = hidden_spec(a.repo, a.commit)
    if not spec["files"]:
        raise SystemExit("error: the commits add or change no test-only file")
    base = tempfile.mkdtemp(prefix="bench-ref-")
    dest = os.path.join(os.path.realpath(base), "clone")
    try:
        clonemod.make_clone(a.repo, a.commit[-1], dest)
        copy_hidden(a.repo, spec, dest)
        results = run_hidden(a.repo, a.commit, dest, spec, out, a.timeout)
    finally:
        if not a.keep:
            shutil.rmtree(base, ignore_errors=True)
    expected = sorted(t for t, s in results.items() if passed(s))
    if not expected:
        raise SystemExit("error: zero hidden tests passed in the reference run (found %d); see raw output in %s" % (len(results), out))
    with open(os.path.join(out, "expected.json"), "w") as f:
        json.dump({"commits": a.commit, "hidden_files": [p for _, p in spec["files"]], "expected": expected,
                   "not_passing_in_reference": sorted(t for t in results if t not in expected)}, f, indent=2)
        f.write("\n")
    print("expected set: %d hidden tests pass in the reference clone (%d did not); written to %s" % (
        len(expected), len(results) - len(expected), os.path.join(out, "expected.json")))
    return 0


def cmd_score(a):
    out = os.path.abspath(a.out)
    os.makedirs(out, exist_ok=True)
    with open(os.path.join(a.run_dir, "run.json")) as f:
        run_info = json.load(f)
    clone, parent = run_info["clone"], run_info["parent"]
    with open(a.expected) as f:
        expected = json.load(f)["expected"]
    spec = hidden_spec(a.repo, a.commit)
    # Order matters: the outside-files list, the diff and the gate see the agent's work only.
    # Copying the hidden tests in comes last because it changes files.
    outside = outside_files(clone, parent, changed_by_commits(a.repo, a.commit))
    diff = snapshot(clone, parent, [])
    with open(os.path.join(out, "diff.patch"), "w") as f:
        f.write(diff)
    gate_result = gate(clone, parent, out, a.timeout)
    copy_hidden(a.repo, spec, clone)
    results = run_hidden(a.repo, a.commit, clone, spec, out, a.timeout)
    tests = score_tests(expected, results)
    score = {"task": run_info["task"], "setup": run_info["setup"], "parent": parent, "commits": a.commit,
             "hidden_passed": sum(1 for v in tests.values() if v == "pass"), "hidden_total": len(tests),
             "tests": tests, "gate": gate_result, "outside_files": outside, "diff_lines": diff.count("\n")}
    with open(os.path.join(out, "score.json"), "w") as f:
        json.dump(score, f, indent=2)
        f.write("\n")
    print("%s/%s: hidden tests %d/%d, gate exit %s, %d files outside the task" % (
        score["task"], score["setup"], score["hidden_passed"], score["hidden_total"], gate_result["exit"], len(outside)))
    return 0


def cmd_blind(a):
    briefs = {}
    for item in a.brief:
        task, _, path = item.partition("=")
        with open(path) as f:
            briefs[task] = f.read()
    runs = []
    for d in a.runs:
        with open(os.path.join(d, "score.json")) as f:
            task = json.load(f)["task"]
        if task not in briefs:
            raise SystemExit("error: no --brief for task %r (run %s)" % (task, d))
        with open(os.path.join(d, "diff.patch")) as f:
            runs.append((d, task, f.read()))
    rng = random.Random(a.seed)
    ids = rng.sample(["%08x" % i for i in range(1 << 20)], len(runs))
    out = os.path.abspath(a.out)
    os.makedirs(os.path.join(out, "diffs"), exist_ok=True)
    key = {}
    for rid, (d, task, diff) in zip(ids, runs):
        with open(os.path.join(out, "diffs", rid + ".md"), "w") as f:
            f.write("# Diff %s\n\n## Brief\n\n%s\n\n## Diff against the starting commit\n\n~~~~diff\n%s~~~~\n" % (rid, briefs[task].rstrip(), diff))
        key[rid] = {"run_dir": d, "task": task}
    order = list(ids)
    rng.shuffle(order)
    with open(os.path.join(out, "rubric.md"), "w") as f:
        f.write(RUBRIC)
    with open(os.path.join(out, "order.txt"), "w") as f:
        f.write("\n".join(order) + "\n")
    with open(os.path.join(out, "key.json"), "w") as f:  # the grader never reads this file
        json.dump(key, f, indent=2)
        f.write("\n")
    print("%d diffs written to %s/diffs; order in order.txt, key in key.json" % (len(runs), out))
    return 0


def main(argv=None):
    here = os.path.dirname(os.path.abspath(__file__))
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0], formatter_class=argparse.RawDescriptionHelpFormatter,
                                 epilog=__doc__.split("\n", 1)[1])
    sub = ap.add_subparsers(dest="cmd", required=True)
    for name in ("reference", "score"):
        p = sub.add_parser(name)
        p.add_argument("--repo", default=os.path.dirname(here), help="source repository (read only)")
        p.add_argument("--commit", action="append", required=True, help="the original task commit; repeat for a task of several, oldest first")
        p.add_argument("--out", required=True, help="folder for results and raw output")
        p.add_argument("--timeout", type=int, default=3600, help="seconds allowed per command")
        if name == "reference":
            p.add_argument("--keep", action="store_true", help="keep the temporary clone")
        else:
            p.add_argument("--run-dir", required=True, help="folder that clone.py made (holds run.json and clone/)")
            p.add_argument("--expected", required=True, help="expected.json from the reference command")
    p = sub.add_parser("blind")
    p.add_argument("--runs", nargs="+", required=True, help="result folders of graded runs (each holds score.json and diff.patch)")
    p.add_argument("--brief", action="append", required=True, help="TASK=path to that task's brief; repeat per task")
    p.add_argument("--seed", type=int, help="fixes the ids and the order")
    p.add_argument("--out", default=os.path.join(here, "results", "blind"))
    a = ap.parse_args(argv)
    return {"reference": cmd_reference, "score": cmd_score, "blind": cmd_blind}[a.cmd](a)


if __name__ == "__main__":
    sys.exit(main())
