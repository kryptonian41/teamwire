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
import xml.etree.ElementTree as ET

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
    return spec_of(last)


def range_status(repo, base, target):
    out = common.git(repo, "diff", "--name-status", "--no-renames", base, target)
    return [tuple(l.split("\t", 1)) for l in out.splitlines()]


def hidden_spec_range(repo, base, target):
    """The hidden tests of a range: test-only files added or changed between base and target, all read from target."""
    return spec_of({p: target for s, p in range_status(repo, base, target) if common.is_test_only(p) and s != "D"})


def spec_of(last):
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

    A top-level tests/NAME.rs and a tests/NAME/main.rs are each one binary. Any other file in a sub-folder
    (support code) may feed any binary, so every integration binary of the crate counts as hidden.
    """
    out = set()
    for p in paths:
        rest = p.split("/tests/", 1)[1]
        if "/" in rest and not re.fullmatch(r"[^/]+/main\.rs", rest):
            return None
        out.add("tests/" + rest)
    return out


def crate_binaries(clone, crate, wanted):
    """The binaries to run for a crate: the wanted ones, or (wanted is None) every one in its tests/ folder."""
    if wanted is not None:
        return sorted(wanted)
    root = os.path.join(clone, "crates", crate, "tests")
    names = os.listdir(root) if os.path.isdir(root) else []
    return sorted("tests/" + (n if n.endswith(".rs") else n + "/main.rs") for n in names
                  if n.endswith(".rs") or os.path.exists(os.path.join(root, n, "main.rs")))


def test_flag_name(binary):
    """`cargo test --test` takes the binary's name: tests/NAME.rs and tests/NAME/main.rs are both NAME."""
    return re.sub(r"\.rs$", "", binary.split("/")[1])


def compile_errors(text):
    """`error...` lines of cargo output, each with the `-->` location line that follows it."""
    lines, found = ANSI.sub("", text).splitlines(), []
    for i, line in enumerate(lines):
        if re.match(r"error(\[\w+\])?:", line) and not re.match(r"error: (could not compile|test failed|aborting)", line):
            where = lines[i + 1].strip() if i + 1 < len(lines) and lines[i + 1].lstrip().startswith("-->") else ""
            found.append((line.strip() + " " + where).strip())
    return found


# ---------- parsers ----------

def parse_cargo(text, crate):
    """`cargo test` output -> {"crate::binary::test": "ok"|"FAILED"|"ignored"}.

    Result lines are read only before a binary's `failures:` section: after it, cargo prints the
    captured output of failed tests, and that output may itself hold a `test x ... ok` line.
    """
    results, binary, in_failures = {}, None, False
    for line in ANSI.sub("", text).splitlines():
        m = re.match(r"\s*Running (unittests )?(\S+)", line)
        if m:
            binary, in_failures = ("unittests " if m.group(1) else "") + m.group(2), False
            continue
        if re.match(r"\s*Doc-tests ", line):
            binary, in_failures = "doctests", False
            continue
        if line.strip() == "failures:":
            in_failures = True
        m = re.match(r"test (.+?) \.\.\. (ok|FAILED|ignored)\b", line)
        if m and binary and not in_failures:
            results["%s::%s::%s" % (crate, binary, m.group(1))] = m.group(2)
    return results


def parse_junit(xml_text):
    """bun's JUnit report -> {"file::describe > name": "pass"|"fail"|"skip"}. Bun prints no (pass) lines to a pipe."""
    results = {}
    for tc in ET.fromstring(xml_text).iter("testcase"):
        name = tc.get("name", "")
        if tc.get("classname"):
            name = tc.get("classname") + " > " + name
        if tc.find("failure") is not None or tc.find("error") is not None:
            status = "fail"
        elif tc.find("skipped") is not None:
            status = "skip"
        else:
            status = "pass"
        tid = "%s::%s" % (tc.get("file", ""), name)
        if results.get(tid) != "fail":  # a repeated name fails if any copy fails
            results[tid] = status
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
    """Run every crate's tests (and bun for ui) in `clone`.

    Returns (results of the hidden tests, problems, compile errors per "crate::tests/binary"), where a problem
    is a command that did not finish (bun install failed, a timeout). A binary that does not build shows up
    as hidden files without tests, and its errors are listed.
    """
    results, problems, errors, last = {}, [], {}, commits[-1]
    for crate, paths in sorted(spec["crates"].items()):
        pkg = package_name(repo, last, crate)
        # One cargo run per binary: a binary that does not compile must fail only its own tests.
        for binary in crate_binaries(clone, crate, hidden_binaries(paths)):
            name = test_flag_name(binary)
            code, text = run(["cargo", "test", "-p", pkg, "--locked", "--test", name, "--no-fail-fast"], clone,
                             os.path.join(out, "raw-cargo-%s-%s.txt" % (pkg, name)), timeout)
            if code is None:
                problems.append("cargo test -p %s --test %s timed out" % (pkg, name))
            if code != 0 and compile_errors(text):
                errors["%s::%s" % (pkg, binary)] = compile_errors(text)
            for tid, status in parse_cargo(text, pkg).items():
                if tid.split("::")[1] == binary:
                    results[tid] = status
    if spec["ui_files"]:
        ui = os.path.join(clone, "ui")
        code, _ = run(["bun", "install", "--frozen-lockfile"], ui, os.path.join(out, "raw-bun-install.txt"), timeout)
        if code != 0:
            problems.append("bun install --frozen-lockfile failed (exit %s)" % code)
        report = os.path.join(out, "raw-bun-junit.xml")
        if os.path.exists(report):
            os.remove(report)
        run(["bun", "test", "--reporter=junit", "--reporter-outfile=" + report] + spec["ui_files"], ui,
            os.path.join(out, "raw-bun-test.txt"), timeout)
        if os.path.exists(report):
            try:
                with open(report) as f:
                    for tid, status in parse_junit(f.read()).items():
                        if tid.split("::")[0] in spec["ui_files"]:
                            results[tid] = status
            except ET.ParseError as e:
                problems.append("bun's JUnit report does not parse: %s" % e)
        else:
            problems.append("bun wrote no JUnit report")
    return results, problems, errors


def files_without_passing_tests(repo, commits, spec, results):
    """Hidden files that contribute no passing test. A file in a sub-folder of tests/ (support code)
    has no binary of its own, so its crate must pass at least one integration test."""
    ok = {t for t, s in results.items() if passed(s)}
    missing = []
    for crate, paths in sorted(spec["crates"].items()):
        pkg = package_name(repo, commits[-1], crate)
        for p in paths:
            rest = p.split("/tests/", 1)[1]
            prefix = "%s::tests/%s::" % (pkg, rest) if "/" not in rest else "%s::tests/" % pkg
            if not any(t.startswith(prefix) for t in ok):
                missing.append(p)
    for p in spec["ui_files"]:
        if not any(t.startswith(p + "::") for t in ok):
            missing.append("ui/" + p)
    return missing


# ---------- the run's work ----------

def snapshot(clone, parent, diff_args, ref=None):
    """git diff of the run's work against `parent`, teamwire files left out.

    Without `ref`: the clone's whole working tree (committed, staged, unstaged and untracked files,
    ignored files left out), through a temporary index so the clone's own index is untouched.
    With `ref`: that branch or sha's tree; the working tree is not used.
    """
    if ref:
        return common.git(clone, "diff", "--no-renames", *diff_args, parent, ref, "--", ".", *TEAMWIRE_EXCLUDES)
    with tempfile.TemporaryDirectory() as d:
        env = dict(os.environ, GIT_INDEX_FILE=os.path.join(d, "index"))
        common.git(clone, "read-tree", parent, env=env)
        common.git(clone, "add", "-A", "--", ".", *TEAMWIRE_EXCLUDES, env=env)
        return common.git(clone, "diff", "--cached", "--no-renames", *diff_args, parent, env=env)


def outside_files(clone, parent, original_paths, ref=None):
    changed = snapshot(clone, parent, ["--name-only"], ref).split("\n")
    return sorted(p for p in changed if p and p not in original_paths)


def parse_gate(text):
    steps = [{"n": int(m.group(1)), "name": m.group(3), "result": m.group(4)}
             for m in re.finditer(r"^gate (\d+)/(\d+) (\w+): (ok|FAILED)", ANSI.sub("", text), re.M)]
    return {"steps": steps, "last_step": steps[-1]["name"] if steps else None,
            "reached_tidy": any(s["name"] == "tidy" for s in steps),
            "flagged_lines": [l.strip() for l in ANSI.sub("", text).splitlines() if GATE_FLAGS.search(l)]}


def gate(copy, parent, out, timeout):
    # TEAMWIRE_BASE: a bundle clone has no main or origin/main, and the gate exits 1 without it.
    code, text = run(["scripts/gate.sh", "fast"], copy, os.path.join(out, "raw-gate.txt"), timeout,
                     env=dict(os.environ, TEAMWIRE_BASE=parent))
    return dict(parse_gate(text), exit=code)


def make_grading_copy(clone, parent, dest, ref, binary_diff):
    """A throwaway copy of the agent's tree, so grading never writes into the run's clone.

    The copy is a local clone of the run's clone, checked out at the agent's ref, or at the parent with
    the working-tree diff applied. The agent's own scripts/ and xtask/ are replaced by the parent's:
    the gate must run the parent's checks, not ones the agent rewrote.
    """
    common.git(os.path.dirname(dest), "clone", "--quiet", "--local", "--no-checkout", clone, dest)
    common.git(dest, "checkout", "--quiet", "--detach", ref or parent)
    if not ref:
        patch = os.path.join(os.path.dirname(dest), "work.patch")
        with open(patch, "w") as f:
            f.write(binary_diff)
        if binary_diff:
            common.git(dest, "apply", "--whitespace=nowarn", patch)
    for d in ("scripts", "xtask"):
        shutil.rmtree(os.path.join(dest, d), ignore_errors=True)
        if common.git(dest, "ls-tree", "--name-only", parent, d).strip():
            common.git(dest, "checkout", parent, "--", d)


# ---------- commands ----------

def checkout_clone(repo, target, dest):
    """A clone checked out at the end commit: the tree a range-mode run is asked to reach."""
    common.git(os.path.dirname(dest), "clone", "--quiet", "--local", "--no-checkout", repo, dest)
    common.git(dest, "checkout", "--quiet", "--detach", target)


def task_of(a):
    """Resolve --commit or --base/--target into what both commands need."""
    rev = lambda r: common.git(a.repo, "rev-parse", "--verify", r + "^{commit}").strip()
    if a.commit:
        commits = [rev(c) for c in a.commit]
        return {"commits": commits, "lookup": commits, "spec": hidden_spec(a.repo, commits),
                "changed": changed_by_commits(a.repo, commits), "ids": {"commits": commits}}
    try:
        base, target = clonemod.task_range(a.repo, a.base, a.target)
    except clonemod.LeakError as e:
        raise SystemExit("error: %s" % e)
    return {"commits": [], "lookup": [target], "spec": hidden_spec_range(a.repo, base, target),
            "changed": {p for _, p in range_status(a.repo, base, target)}, "ids": {"commits": [], "base": base, "target": target}}


def cherry_picked_clone(repo, commits, dest):
    """A clone at the first commit's parent with the task commits cherry-picked in order: the tree the
    run's agent is asked to reach. (The reference is the grader's own; it need not be leak-free.)"""
    env = dict(os.environ, GIT_COMMITTER_NAME="bench", GIT_COMMITTER_EMAIL="bench@localhost",
               GIT_AUTHOR_NAME="bench", GIT_AUTHOR_EMAIL="bench@localhost")
    parent = common.git(repo, "rev-parse", "--verify", commits[0] + "^").strip()
    common.git(os.path.dirname(dest), "clone", "--quiet", "--local", "--no-checkout", repo, dest)
    common.git(dest, "checkout", "--quiet", "--detach", parent)
    for c in commits:
        try:
            common.git(dest, "cherry-pick", c, env=env)
        except RuntimeError:
            common.git(dest, "cherry-pick", "--abort", check=False, env=env)
            raise SystemExit("error: commit %s does not apply cleanly on the parent %s plus the commits before it" % (c, parent[:12]))
    return parent


def short(ident):
    return {k: ([c[:8] for c in v] if isinstance(v, list) else v and v[:8]) for k, v in ident.items() if v}


def cmd_reference(a):
    out = os.path.abspath(a.out)
    os.makedirs(out, exist_ok=True)
    task = task_of(a)
    commits, spec = task["lookup"], task["spec"]
    if not spec["files"]:
        raise SystemExit("error: the task adds or changes no test-only file")
    base = tempfile.mkdtemp(prefix="bench-ref-")
    try:
        dest = os.path.join(os.path.realpath(base), "clone")
        if a.commit:
            cherry_picked_clone(a.repo, commits, dest)
        else:
            checkout_clone(a.repo, commits[0], dest)
        results, problems, errors = run_hidden(a.repo, commits, dest, spec, out, a.timeout)
    finally:
        if not a.keep:
            shutil.rmtree(base, ignore_errors=True)
    missing = files_without_passing_tests(a.repo, commits, spec, results)
    if problems or missing:
        raise SystemExit("error: the reference run is not usable (raw output in %s)\n%s" % (out, "\n".join(
            ["  " + p for p in problems] + ["  hidden file with no passing test: " + m for m in missing])))
    expected = sorted(t for t, s in results.items() if passed(s))
    with open(os.path.join(out, "expected.json"), "w") as f:
        json.dump({**task["ids"], "hidden_files": [p for _, p in spec["files"]], "expected": expected,
                   "not_passing_in_reference": sorted(t for t in results if t not in expected),
                   "compile_errors": errors}, f, indent=2)
        f.write("\n")
    print("expected set: %d hidden tests pass in the reference clone (%d did not); written to %s" % (
        len(expected), len(results) - len(expected), os.path.join(out, "expected.json")))
    return 0


def cmd_score(a):
    out = os.path.abspath(a.out)
    os.makedirs(out, exist_ok=True)
    with open(os.path.join(a.run_dir, "run.json")) as f:
        run_info = json.load(f)
    task = task_of(a)
    commits, spec = task["commits"], task["spec"]
    with open(a.expected) as f:
        expected_info = json.load(f)
    # Both files must name the same task: the same commits, and in range mode the same start and end.
    key = lambda info: {k: info.get(k) or [] if k == "commits" else info.get(k) for k in ("commits", "base", "target")}
    want = {"commits": [], "base": None, "target": None, **task["ids"]}
    if key(expected_info) != want or key(run_info) != want:
        raise SystemExit("error: the task differs: given %s, expected.json %s, run.json %s" % (
            short(want), short(key(expected_info)), short(key(run_info))))
    clone, parent = run_info["clone"], run_info["parent"]
    ref = None if a.ref == "HEAD" else common.git(clone, "rev-parse", "--verify", a.ref + "^{commit}").strip()
    # The run's clone is only read. The outside-files list and the diff come from it untouched;
    # the gate and the tests run in a throwaway copy that gets the hidden tests.
    outside = outside_files(clone, parent, task["changed"], ref)
    diff = snapshot(clone, parent, [], ref)
    with open(os.path.join(out, "diff.patch"), "w") as f:
        f.write(diff)
    with tempfile.TemporaryDirectory(prefix="bench-grade-") as tmp:
        copy = os.path.join(os.path.realpath(tmp), "copy")
        make_grading_copy(clone, parent, copy, ref, "" if ref else snapshot(clone, parent, ["--binary"]))
        gate_result = gate(copy, parent, out, a.timeout)
        copy_hidden(a.repo, spec, copy)
        results, problems, errors = run_hidden(a.repo, task["lookup"], copy, spec, out, a.timeout)
    tests = score_tests(expected_info["expected"], results)
    warnings = (["the diff is empty: nothing to grade (is the result on another branch? use --ref)"] if not diff.strip() else []) + problems
    score = {"task": run_info["task"], "setup": run_info["setup"], "parent": parent, **task["ids"], "ref": a.ref,
             "hidden_passed": sum(1 for v in tests.values() if v == "pass"), "hidden_total": len(tests),
             "tests": tests, "compile_errors": errors, "gate": gate_result, "outside_files": outside, "diff_lines": diff.count("\n"),
             "warnings": warnings}
    with open(os.path.join(out, "score.json"), "w") as f:
        json.dump(score, f, indent=2)
        f.write("\n")
    print("%s/%s: hidden tests %d/%d, gate exit %s (last step %s), %d files outside the task" % (
        score["task"], score["setup"], score["hidden_passed"], score["hidden_total"], gate_result["exit"],
        gate_result["last_step"], len(outside)))
    for w in warnings:
        print("WARNING: " + w)
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
    key_out = os.path.abspath(a.key_out)
    if key_out == out or key_out.startswith(out + os.sep):
        raise SystemExit("error: --key-out must be outside --out (%s)" % out)
    os.makedirs(os.path.dirname(key_out), exist_ok=True)
    with open(key_out, "w") as f:
        json.dump(key, f, indent=2)
        f.write("\n")
    print("%d diffs written to %s/diffs; order in order.txt; key in %s" % (len(runs), out, key_out))
    return 0


def main(argv=None):
    here = os.path.dirname(os.path.abspath(__file__))
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0], formatter_class=argparse.RawDescriptionHelpFormatter,
                                 epilog=__doc__.split("\n", 1)[1])
    sub = ap.add_subparsers(dest="cmd", required=True)
    for name in ("reference", "score"):
        p = sub.add_parser(name)
        p.add_argument("--repo", default=os.path.dirname(here), help="source repository (read only)")
        p.add_argument("--commit", action="append", help="the original task commit; repeat for a task of several, oldest first")
        p.add_argument("--base", help="range mode: the start commit (use with --target, not --commit)")
        p.add_argument("--target", help="range mode: the end commit; the reference is a checkout of it")
        p.add_argument("--out", required=True, help="folder for results and raw output")
        p.add_argument("--timeout", type=int, default=3600, help="seconds allowed per command")
        if name == "reference":
            p.add_argument("--keep", action="store_true", help="keep the temporary clone")
        else:
            p.add_argument("--run-dir", required=True, help="folder that clone.py made (holds run.json and clone/)")
            p.add_argument("--ref", default="HEAD", help="branch or sha in the run's clone that holds the result; "
                           "HEAD (default) grades the clone's working tree")
            p.add_argument("--expected", required=True, help="expected.json from the reference command")
    p = sub.add_parser("blind")
    p.add_argument("--runs", nargs="+", required=True, help="result folders of graded runs (each holds score.json and diff.patch)")
    p.add_argument("--brief", action="append", required=True, help="TASK=path to that task's brief; repeat per task")
    p.add_argument("--seed", type=int, help="fixes the ids and the order")
    p.add_argument("--out", default=os.path.join(here, "results", "blind"))
    p.add_argument("--key-out", required=True, help="where to write key.json; keep it outside --out, where the grader reads")
    a = ap.parse_args(argv)
    if a.cmd in ("reference", "score") and (bool(a.commit) == bool(a.base or a.target) or bool(a.base) != bool(a.target)):
        ap.error("give either --commit, or --base together with --target")
    return {"reference": cmd_reference, "score": cmd_score, "blind": cmd_blind}[a.cmd](a)


if __name__ == "__main__":
    sys.exit(main())
