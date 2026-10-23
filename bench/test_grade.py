import contextlib
import io
import json
import os
import tempfile
import unittest
from unittest import mock

import gittest as g
import grade

CARGO = """
   Compiling x v0.1.0
     Running unittests src/lib.rs (target/debug/deps/x-abc)

running 2 tests
test unit::one ... ok
test unit::two ... FAILED

     Running tests/handoff.rs (target/debug/deps/handoff-def)

running 3 tests
test refuses_when_busy ... ok
test nested::module::case ... FAILED
test slow_one ... ignored, takes too long

   Doc-tests x

running 1 test
test src/lib.rs - f (line 3) ... ok
"""

JUNIT = """<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="bun test" tests="5">
  <testsuite name="src/web/a.test.ts" file="src/web/a.test.ts" tests="4">
    <testsuite name="grp" file="src/web/a.test.ts">
      <testcase name="passes" classname="grp" file="src/web/a.test.ts" />
      <testcase name="fails" classname="grp" file="src/web/a.test.ts"><failure type="AssertionError" /></testcase>
    </testsuite>
    <testcase name="skipped" classname="" file="src/web/a.test.ts"><skipped /></testcase>
    <testcase name="top" classname="" file="src/web/a.test.ts" />
  </testsuite>
  <testsuite name="src/web/b.test.tsx" file="src/web/b.test.tsx">
    <testcase name="boom" classname="" file="src/web/b.test.tsx"><error type="Error" /></testcase>
  </testsuite>
</testsuites>
"""


class Parsers(unittest.TestCase):
    def test_cargo_results_carry_the_binary(self):
        r = grade.parse_cargo(CARGO, "x")
        self.assertEqual(r["x::tests/handoff.rs::refuses_when_busy"], "ok")
        self.assertEqual(r["x::tests/handoff.rs::nested::module::case"], "FAILED")
        self.assertEqual(r["x::tests/handoff.rs::slow_one"], "ignored")
        self.assertEqual(r["x::unittests src/lib.rs::unit::two"], "FAILED")
        self.assertEqual(r["x::doctests::src/lib.rs - f (line 3)"], "ok")
        self.assertEqual(len(r), 6)

    def test_junit_results_carry_the_file_and_pass_without_a_pass_line(self):
        r = grade.parse_junit(JUNIT)
        self.assertEqual(r, {"src/web/a.test.ts::grp > passes": "pass", "src/web/a.test.ts::grp > fails": "fail",
                             "src/web/a.test.ts::skipped": "skip", "src/web/a.test.ts::top": "pass",
                             "src/web/b.test.tsx::boom": "fail"})

    def test_a_repeated_junit_name_fails_if_any_copy_fails(self):
        x = '<testsuites><testcase name="n" file="f.test.ts"/><testcase name="n" file="f.test.ts"><failure/></testcase><testcase name="n" file="f.test.ts"/></testsuites>'
        self.assertEqual(grade.parse_junit(x), {"f.test.ts::n": "fail"})

    def test_cargo_ignores_result_lines_in_the_failures_section(self):
        text = """     Running tests/a.rs (x)
running 2 tests
test beta ... FAILED
test alpha ... ok

failures:

---- beta stdout ----
test beta ... ok
test alpha ... FAILED

failures:
    beta

test result: FAILED. 1 passed; 1 failed

     Running tests/b.rs (y)
test gamma ... ok
"""
        self.assertEqual(grade.parse_cargo(text, "c"), {"c::tests/a.rs::beta": "FAILED", "c::tests/a.rs::alpha": "ok", "c::tests/b.rs::gamma": "ok"})

    def test_gate_steps_and_flags(self):
        g_ = grade.parse_gate("gate: base: abc\ngate 1/3 fmt: ok (1s)\ngate 2/3 tidy: ok (2s)\nWEAKENED x.rs\ngate 3/3 clippy: FAILED (3s): cargo clippy\nWEAKENED not a step line\n")
        self.assertEqual((g_["last_step"], g_["reached_tidy"]), ("clippy", True))
        self.assertEqual(g_["flagged_lines"], ["WEAKENED x.rs", "WEAKENED not a step line"])
        stopped = grade.parse_gate("gate 1/3 fmt: FAILED (1s): cargo fmt\n")
        self.assertEqual((stopped["last_step"], stopped["reached_tidy"]), ("fmt", False))

    def test_missing_ignored_and_failed_expected_tests_fail(self):
        expected = ["a", "b", "c", "d"]
        got = grade.score_tests(expected, {"a": "ok", "b": "FAILED", "c": "ignored", "extra": "ok"})
        self.assertEqual(got, {"a": "pass", "b": "fail", "c": "fail", "d": "fail"})


class Repo(unittest.TestCase):
    def setUp(self):
        self.d = tempfile.TemporaryDirectory()
        self.addCleanup(self.d.cleanup)
        self.repo = g.init(os.path.join(self.d.name, "r"))
        g.commit(self.repo, {"crates/a/Cargo.toml": '[package]\nname = "teamwire-a"\n', "crates/a/src/lib.rs": "1\n",
                             "crates/a/tests/old.rs": "old\n", ".gitignore": "target/\n"})
        self.parent = g.git(self.repo, "rev-parse", "HEAD")

    def test_hidden_spec_takes_test_only_files_from_the_last_commit_that_changed_them(self):
        c1 = g.commit(self.repo, {"crates/a/tests/t.rs": "v1\n", "crates/a/src/lib.rs": "2\n", "ui/x.test.ts": "u\n"})
        c2 = g.commit(self.repo, {"crates/a/tests/t.rs": "v2\n", "crates/b/tests/sub/support.rs": "s\n"}, remove=["crates/a/tests/old.rs"])
        spec = grade.hidden_spec(self.repo, [c1, c2])
        self.assertEqual(spec["files"], [(c2, "crates/a/tests/t.rs"), (c2, "crates/b/tests/sub/support.rs"), (c1, "ui/x.test.ts")])
        self.assertEqual(sorted(spec["crates"]), ["a", "b"])
        self.assertEqual(spec["ui_files"], ["x.test.ts"])  # a deleted test file is not copied back
        dest = os.path.join(self.d.name, "dest")
        os.makedirs(os.path.join(dest, "crates/a/tests"))
        with open(os.path.join(dest, "crates/a/tests/t.rs"), "w") as f:
            f.write("agent's version\n")
        grade.copy_hidden(self.repo, spec, dest)
        with open(os.path.join(dest, "crates/a/tests/t.rs")) as f:
            self.assertEqual(f.read(), "v2\n")
        self.assertTrue(os.path.exists(os.path.join(dest, "crates/b/tests/sub/support.rs")))

    def test_hidden_binaries(self):
        self.assertEqual(grade.hidden_binaries(["crates/a/tests/t.rs", "crates/a/tests/u.rs"]), {"tests/t.rs", "tests/u.rs"})
        self.assertIsNone(grade.hidden_binaries(["crates/a/tests/t.rs", "crates/a/tests/support/x.rs"]))

    def test_hidden_binaries_take_a_main_rs_folder_as_one_binary(self):
        self.assertEqual(grade.hidden_binaries(["crates/a/tests/t.rs", "crates/a/tests/big/main.rs"]), {"tests/t.rs", "tests/big/main.rs"})
        self.assertEqual(grade.test_flag_name("tests/big/main.rs"), "big")
        self.assertEqual(grade.test_flag_name("tests/t.rs"), "t")

    def test_compile_errors_keep_error_lines_with_their_location(self):
        text = ("warning: unused\n\x1b[1merror[E0432]: unresolved import\x1b[0m\n --> crates/a/tests/u.rs:3:5\n  |\n"
                "error: cannot find macro `m`\n --> crates/a/tests/u.rs:9:1\nerror: could not compile `a` (test \"u\")\n"
                "error: test failed, to rerun pass `--test u`\n")
        self.assertEqual(grade.compile_errors(text), ["error[E0432]: unresolved import --> crates/a/tests/u.rs:3:5",
                                                      "error: cannot find macro `m` --> crates/a/tests/u.rs:9:1"])

    def test_outside_files_count_every_kind_of_change_but_not_ignored_or_teamwire_files(self):
        r = self.repo
        g.commit(r, {"crates/a/src/lib.rs": "agent\n"})  # committed, and in the original commit
        g.commit(r, {"crates/a/src/extra.rs": "x\n"})  # committed, outside
        with open(os.path.join(r, "crates/a/src/lib.rs"), "a") as f:
            f.write("unstaged\n")
        with open(os.path.join(r, "staged.txt"), "w") as f:
            f.write("s\n")
        g.git(r, "add", "staged.txt")
        for p, text in {"untracked.txt": "u\n", "target/junk": "j\n", "TASK.md": "t\n", ".worktrees/m/f": "w\n", "reports/x.md": "r\n"}.items():
            os.makedirs(os.path.dirname(os.path.join(r, p)) or r, exist_ok=True)
            with open(os.path.join(r, p), "w") as f:
                f.write(text)
        index_before = g.git(r, "diff", "--cached", "--name-only")
        got = grade.outside_files(r, self.parent, {"crates/a/src/lib.rs"})
        self.assertEqual(got, ["crates/a/src/extra.rs", "staged.txt", "untracked.txt"])
        self.assertEqual(g.git(r, "diff", "--cached", "--name-only"), index_before)  # the clone's own index is untouched

    def test_snapshot_diff_is_file_contents_only(self):
        g.commit(self.repo, {"crates/a/src/lib.rs": "changed\n"}, msg="secret commit message")
        diff = grade.snapshot(self.repo, self.parent, [])
        self.assertIn("+changed", diff)
        self.assertNotIn("secret commit message", diff)

    def test_hidden_spec_range_reads_added_and_modified_test_files_from_the_end_only(self):
        g.commit(self.repo, {"crates/a/tests/mod.rs": "m0\n"})
        start = g.git(self.repo, "rev-parse", "HEAD")
        g.git(self.repo, "checkout", "-q", "-b", "side")
        g.commit(self.repo, {"crates/a/tests/mod.rs": "m1\n", "ui/new.test.tsx": "u\n", "crates/a/src/lib.rs": "2\n"},
                 remove=["crates/a/tests/old.rs"])
        g.git(self.repo, "checkout", "-q", "develop")
        g.commit(self.repo, {"crates/b/tests/b.rs": "b\n", "docs/x.md": "d\n"})
        g.git(self.repo, "merge", "-q", "--no-ff", "-m", "merge", "side")
        end = g.git(self.repo, "rev-parse", "HEAD")
        spec = grade.hidden_spec_range(self.repo, start, end)
        self.assertEqual(spec["files"], [(end, "crates/a/tests/mod.rs"), (end, "crates/b/tests/b.rs"), (end, "ui/new.test.tsx")])
        self.assertEqual(spec["ui_files"], ["new.test.tsx"])  # old.rs was deleted in the range, so it is not hidden
        self.assertEqual({p for _, p in grade.range_status(self.repo, start, end)},
                         {"crates/a/tests/mod.rs", "crates/a/tests/old.rs", "ui/new.test.tsx", "crates/a/src/lib.rs", "crates/b/tests/b.rs", "docs/x.md"})


class Blind(unittest.TestCase):
    def make_run(self, base, name, task, setup, diff):
        d = os.path.join(base, name)
        os.makedirs(d)
        with open(os.path.join(d, "score.json"), "w") as f:
            json.dump({"task": task, "setup": setup}, f)
        with open(os.path.join(d, "diff.patch"), "w") as f:
            f.write(diff)
        return d

    def test_blind_inputs_hide_the_setup_and_are_reproducible(self):
        with tempfile.TemporaryDirectory() as base:
            runs = [self.make_run(base, "r%d" % i, "t1" if i < 2 else "t2", s, "+line %d\n" % i)
                    for i, s in enumerate(["solo", "teamwire-lead", "solo-subagents"])]
            for t in ("t1", "t2"):
                with open(os.path.join(base, t + ".md"), "w") as f:
                    f.write("Brief for %s\n" % t)

            def go(seed, out):
                with contextlib.redirect_stdout(io.StringIO()):
                    grade.main(["blind", "--runs"] + runs + ["--brief", "t1=" + base + "/t1.md", "--brief", "t2=" + base + "/t2.md",
                                                             "--seed", str(seed), "--out", out, "--key-out", out + "-key.json"])
                return out

            out = go(7, os.path.join(base, "o1"))
            names = sorted(os.listdir(os.path.join(out, "diffs")))
            self.assertEqual(len(names), 3)
            for n in names + ["rubric.md", "order.txt"]:
                p = os.path.join(out, "diffs", n) if n.endswith(".md") and n != "rubric.md" else os.path.join(out, n)
                with open(p) as f:
                    text = f.read()
                for word in ("solo", "teamwire", "subagents"):
                    self.assertNotIn(word, n + text)
            with open(os.path.join(out, "order.txt")) as f:
                order = f.read().split()
            self.assertEqual(sorted(order), sorted(n[:-3] for n in names))
            self.assertNotIn("key.json", os.listdir(out))
            with open(out + "-key.json") as f:
                key = json.load(f)
            self.assertEqual(sorted(key), sorted(order))
            self.assertEqual({v["task"] for v in key.values()}, {"t1", "t2"})
            with open(os.path.join(out, "diffs", order[0] + ".md")) as f:
                self.assertIn("Brief for", f.read())
            with open(os.path.join(out, "rubric.md")) as f:
                rubric = f.read()
            for w in ("Correctness", "Scope", "Test quality"):
                self.assertIn(w, rubric)
            with self.assertRaises(SystemExit):
                with contextlib.redirect_stdout(io.StringIO()):
                    grade.main(["blind", "--runs"] + runs + ["--brief", "t1=" + base + "/t1.md", "--brief", "t2=" + base + "/t2.md",
                                                             "--out", os.path.join(base, "o9"), "--key-out", os.path.join(base, "o9", "key.json")])
            again = go(7, os.path.join(base, "o2"))
            with open(os.path.join(again, "order.txt")) as f:
                self.assertEqual(f.read().split(), order)
            other = go(8, os.path.join(base, "o3"))
            with open(other + "-key.json") as f:
                self.assertNotEqual(json.load(f), key)


FAKE_CARGO = """#!/bin/sh
case "$3" in
  pkg-b)
    [ -f crates/b/BROKEN ] && exit 101
    echo "     Running tests/tb.rs (target/debug/deps/tb-1)"
    echo "test tb_one ... ok"
    exit 0;;
esac
echo "     Running tests/t.rs (target/debug/deps/t-1)"
echo "test alpha ... ok"
if grep -q GOOD crates/a/src/lib.rs; then echo "test beta ... ok"; else echo "test beta ... FAILED"; fi
if [ -f d.txt ]; then echo "test gamma ... ok"; else echo "test gamma ... FAILED"; fi
"""
# $6 is the binary named by --test; binary u does not compile while the file U_BROKEN exists
FAKE_CARGO_TWO_BINARIES = """#!/bin/sh
case "$6" in
  t) echo "     Running tests/t.rs (target/debug/deps/t-1)"; echo "test t_one ... ok";;
  u) if [ -f U_BROKEN ]; then
       echo "error[E0432]: unresolved import"; echo " --> crates/a/tests/u.rs:3:5"; echo "error: could not compile"; exit 101
     fi
     echo "     Running tests/u.rs (target/debug/deps/u-1)"; echo "test u_one ... ok";;
esac
"""
FAKE_GATE = """#!/bin/sh
echo "gate 1/3 fmt: ok (0s)"
echo "base=$TEAMWIRE_BASE"
echo "gate 2/3 tidy: ok (0s)"
echo "WEAKENED: crates/a/tests/t.rs"
echo "gate 3/3 clippy: FAILED (0s): cargo clippy"
exit 1
"""


class EndToEnd(unittest.TestCase):
    """reference and score with a fake `cargo` on PATH: the wiring around the parsers."""

    def setUp(self):
        self.d = tempfile.TemporaryDirectory()
        self.addCleanup(self.d.cleanup)
        r = self.repo = g.init(os.path.join(self.d.name, "r"))
        os.makedirs(os.path.join(r, "scripts"))
        with open(os.path.join(r, "scripts/gate.sh"), "w") as f:
            f.write(FAKE_GATE)
        os.chmod(os.path.join(r, "scripts/gate.sh"), 0o755)
        self.parent = g.commit(r, {"crates/a/Cargo.toml": '[package]\nname = "pkg-a"\n', "crates/a/src/lib.rs": "BAD\n",
                                   "crates/b/Cargo.toml": '[package]\nname = "pkg-b"\n', "crates/b/src/lib.rs": "b\n"})
        self.commit = g.commit(r, {"crates/a/src/lib.rs": "GOOD\n", "crates/a/tests/t.rs": "t\n"})
        self.bin = os.path.join(self.d.name, "bin")
        os.makedirs(self.bin)
        self.fake_cargo(FAKE_CARGO)
        patcher = mock.patch.dict(os.environ, {"PATH": self.bin + os.pathsep + os.environ["PATH"]})
        patcher.start()
        self.addCleanup(patcher.stop)
        self.n = 0

    def fake_cargo(self, text):
        p = os.path.join(self.bin, "cargo")
        with open(p, "w") as f:
            f.write(text)
        os.chmod(p, 0o755)

    def quiet(self, argv):
        with contextlib.redirect_stdout(io.StringIO()):
            return grade.main(argv)

    def reference(self, *commits):
        self.n += 1
        out = os.path.join(self.d.name, "ref%d" % self.n)
        argv = ["reference", "--repo", self.repo, "--out", out]
        for c in commits or [self.commit]:
            argv += ["--commit", c]
        self.quiet(argv)
        with open(os.path.join(out, "expected.json")) as f:
            return os.path.join(out, "expected.json"), json.load(f)

    def make_run(self, *commits):
        import clone as clonemod
        with contextlib.redirect_stdout(io.StringIO()):
            clonemod.main(["--repo", self.repo, "--task", "t1", "--setup", "solo", "--root", os.path.join(self.d.name, "runs")]
                          + [x for c in commits or [self.commit] for x in ("--commit", c)])
        root = os.path.join(self.d.name, "runs")
        return os.path.join(root, sorted(os.listdir(root), key=lambda n: os.path.getmtime(os.path.join(root, n)))[-1])

    def score(self, base, expected, *extra, commits=None):
        self.n += 1
        out = os.path.join(self.d.name, "score%d" % self.n)
        argv = ["score", "--repo", self.repo, "--run-dir", base, "--expected", expected, "--out", out] + list(extra)
        for c in commits or [self.commit]:
            argv += ["--commit", c]
        self.quiet(argv)
        with open(os.path.join(out, "score.json")) as f:
            return out, json.load(f)

    def test_a_binary_that_does_not_compile_fails_only_its_own_tests(self):
        self.fake_cargo(FAKE_CARGO_TWO_BINARIES)
        c = g.commit(self.repo, {"crates/a/tests/t.rs": "t3\n", "crates/a/tests/u.rs": "u\n"})
        path, exp = self.reference(c)
        self.assertEqual(exp["expected"], ["pkg-a::tests/t.rs::t_one", "pkg-a::tests/u.rs::u_one"])
        self.assertEqual(exp["compile_errors"], {})
        self.assertTrue(os.path.exists(os.path.join(os.path.dirname(path), "raw-cargo-pkg-a-u.txt")))
        base = self.make_run(c)
        with open(os.path.join(base, "clone", "U_BROKEN"), "w") as f:
            f.write("x\n")
        _, sc = self.score(base, path, commits=[c])
        self.assertEqual(sc["tests"], {"pkg-a::tests/t.rs::t_one": "pass", "pkg-a::tests/u.rs::u_one": "fail"})
        self.assertEqual(sc["compile_errors"], {"pkg-a::tests/u.rs": ["error[E0432]: unresolved import --> crates/a/tests/u.rs:3:5"]})

    def test_reference_with_no_passing_hidden_test_is_an_error(self):
        self.fake_cargo("#!/bin/sh\necho nothing\n")
        with self.assertRaises(SystemExit) as cm:
            self.reference()
        self.assertIn("crates/a/tests/t.rs", str(cm.exception))

    def test_reference_names_a_hidden_file_that_runs_no_test(self):
        # crate b's tests do not build in the reference: the run must not pass with crate a's tests alone
        c = g.commit(self.repo, {"crates/b/tests/tb.rs": "x\n", "crates/b/BROKEN": "x\n", "crates/a/tests/t.rs": "t2\n"})
        with self.assertRaises(SystemExit) as cm:
            self.reference(c)
        self.assertIn("crates/b/tests/tb.rs", str(cm.exception))
        self.assertNotIn("crates/a/tests/t.rs", str(cm.exception))

    def test_reference_applies_the_task_commits_to_the_parent_not_the_history_between_them(self):
        a = self.commit
        g.commit(self.repo, {"d.txt": "unrelated\n"}, "D")  # gamma passes only where D exists
        c = g.commit(self.repo, {"crates/a/tests/t.rs": "t2\n"})
        _, exp = self.reference(a, c)
        self.assertEqual(exp["expected"], ["pkg-a::tests/t.rs::alpha", "pkg-a::tests/t.rs::beta"])
        self.assertEqual(exp["commits"], [a, c])

    def test_reference_stops_naming_a_commit_that_does_not_apply(self):
        c1 = g.commit(self.repo, {"crates/a/tests/t.rs": "one\n"})
        g.commit(self.repo, {"crates/a/tests/t.rs": "two\n"})
        c3 = g.commit(self.repo, {"crates/a/tests/t.rs": "three\n"})
        with self.assertRaises(SystemExit) as cm:
            self.reference(c1, c3)  # c3 changes lines that the skipped commit changed
        self.assertIn(c3, str(cm.exception))

    def test_score_grades_a_copy_and_is_repeatable(self):
        path, exp = self.reference()
        self.assertEqual(exp["expected"], ["pkg-a::tests/t.rs::alpha", "pkg-a::tests/t.rs::beta"])
        base = self.make_run()
        clone_dir = os.path.join(base, "clone")
        for p, text in {"junk.txt": "outside\n", "crates/a/tests/t.rs": "the agent's own test\n", "scripts/gate.sh": "#!/bin/sh\nexit 0\n"}.items():
            os.makedirs(os.path.dirname(os.path.join(clone_dir, p)), exist_ok=True)
            with open(os.path.join(clone_dir, p), "w") as f:
                f.write(text)
        out1, sc = self.score(base, path)
        self.assertEqual(sc["tests"], {"pkg-a::tests/t.rs::alpha": "pass", "pkg-a::tests/t.rs::beta": "fail"})
        self.assertEqual((sc["hidden_passed"], sc["hidden_total"]), (1, 2))
        self.assertEqual(sc["outside_files"], ["junk.txt", "scripts/gate.sh"])
        # the gate that ran is the parent's, not the agent's `exit 0` version
        self.assertEqual(sc["gate"]["exit"], 1)
        self.assertEqual(sc["gate"]["flagged_lines"], ["WEAKENED: crates/a/tests/t.rs"])
        self.assertEqual((sc["gate"]["last_step"], sc["gate"]["reached_tidy"]), ("clippy", True))
        with open(os.path.join(out1, "raw-gate.txt")) as f:
            self.assertIn("base=" + self.parent, f.read())
        with open(os.path.join(out1, "diff.patch")) as f:
            diff1 = f.read()
        self.assertIn("the agent's own test", diff1)
        # the run's clone keeps the agent's files, and a second score gives the same diff
        with open(os.path.join(clone_dir, "crates/a/tests/t.rs")) as f:
            self.assertEqual(f.read(), "the agent's own test\n")
        out2, _ = self.score(base, path)
        with open(os.path.join(out2, "diff.patch")) as f:
            self.assertEqual(f.read(), diff1)

    def test_score_grades_a_named_branch_and_warns_on_an_empty_diff(self):
        path, _ = self.reference()
        base = self.make_run()
        clone_dir = os.path.join(base, "clone")
        g.git(clone_dir, "checkout", "-q", "-b", "afk/team")
        g.commit(clone_dir, {"crates/a/src/lib.rs": "GOOD\n", "other.txt": "o\n"})
        g.git(clone_dir, "checkout", "-q", "bench")
        _, empty = self.score(base, path)
        self.assertTrue(any("diff is empty" in w for w in empty["warnings"]))
        self.assertEqual(empty["hidden_passed"], 1)  # beta fails on the untouched parent
        _, sc = self.score(base, path, "--ref", "afk/team")
        self.assertEqual(sc["warnings"], [])
        self.assertEqual((sc["hidden_passed"], sc["outside_files"]), (2, ["other.txt"]))

    def test_score_refuses_when_the_commits_do_not_belong_together(self):
        path, _ = self.reference()
        other = g.commit(self.repo, {"crates/a/tests/t.rs": "other task\n"})
        base = self.make_run()
        with self.assertRaises(SystemExit) as cm:
            self.score(base, path, commits=[other])
        self.assertIn("differ", str(cm.exception))
        path2, _ = self.reference(other)
        with self.assertRaises(SystemExit):
            self.score(base, path2, commits=[other])  # run.json was made for self.commit

    def range_setup(self):
        """start = self.parent; the end is a merge, so cherry-picking its commits would not rebuild its tree."""
        g.git(self.repo, "checkout", "-q", "-b", "side", self.parent)
        g.commit(self.repo, {"d.txt": "from the side branch\n"})
        g.git(self.repo, "checkout", "-q", "develop")
        g.git(self.repo, "merge", "-q", "--no-ff", "-m", "merge", "side")
        self.end = g.git(self.repo, "rev-parse", "HEAD")
        g.commit(self.repo, {"after.txt": "past the end\n"})

    def range_reference(self):
        out = os.path.join(self.d.name, "rref")
        self.quiet(["reference", "--repo", self.repo, "--out", out, "--base", self.parent, "--target", self.end])
        with open(os.path.join(out, "expected.json")) as f:
            return os.path.join(out, "expected.json"), json.load(f)

    def range_run(self):
        import clone as clonemod
        root = os.path.join(self.d.name, "rruns")
        with contextlib.redirect_stdout(io.StringIO()):
            clonemod.main(["--repo", self.repo, "--task", "t1", "--setup", "solo", "--root", root, "--base", self.parent, "--target", self.end])
        (name,) = os.listdir(root)
        return os.path.join(root, name)

    def range_score(self, run, expected, *extra):
        self.n += 1
        out = os.path.join(self.d.name, "rscore%d" % self.n)
        self.quiet(["score", "--repo", self.repo, "--run-dir", run, "--expected", expected, "--out", out,
                    "--base", self.parent, "--target", self.end] + list(extra))
        with open(os.path.join(out, "score.json")) as f:
            return json.load(f)

    def test_range_reference_checks_out_the_end_including_a_merge(self):
        self.range_setup()
        _, exp = self.range_reference()
        # gamma passes only where d.txt exists, and d.txt arrives by the merge
        self.assertEqual(exp["expected"], ["pkg-a::tests/t.rs::alpha", "pkg-a::tests/t.rs::beta", "pkg-a::tests/t.rs::gamma"])
        self.assertEqual((exp["base"], exp["target"], exp["commits"]), (self.parent, self.end, []))
        self.assertEqual(exp["hidden_files"], ["crates/a/tests/t.rs"])

    def test_range_score_compares_with_the_start_and_refuses_another_task(self):
        self.range_setup()
        path, _ = self.range_reference()
        run = self.range_run()
        clone_dir = os.path.join(run, "clone")
        self.assertEqual(g.git(clone_dir, "rev-parse", "HEAD"), self.parent)
        null = self.range_score(run, path)
        self.assertEqual((null["parent"], null["base"], null["target"]), (self.parent, self.parent, self.end))
        self.assertEqual(null["tests"], {"pkg-a::tests/t.rs::alpha": "pass", "pkg-a::tests/t.rs::beta": "fail", "pkg-a::tests/t.rs::gamma": "fail"})
        self.assertEqual(null["outside_files"], [])
        for p, text in {"crates/a/src/lib.rs": "GOOD\n", "d.txt": "d\n", "junk.txt": "j\n"}.items():
            with open(os.path.join(clone_dir, p), "w") as f:
                f.write(text)
        sc = self.range_score(run, path)
        self.assertEqual(sc["hidden_passed"], 3)
        self.assertEqual(sc["outside_files"], ["junk.txt"])  # lib.rs and d.txt changed between start and end
        for argv in (["--base", self.parent, "--target", self.commit], ["--base", self.end, "--target", self.end], ["--commit", self.commit]):
            with self.assertRaises(SystemExit) as cm:
                self.quiet(["score", "--repo", self.repo, "--run-dir", run, "--expected", path, "--out", os.path.join(self.d.name, "x")] + argv)
            self.assertTrue("differ" in str(cm.exception) or "ancestor" in str(cm.exception), str(cm.exception))

    def test_range_gate_gets_the_start_as_its_base(self):
        self.range_setup()
        path, _ = self.range_reference()
        self.range_score(self.range_run(), path)
        with open(os.path.join(self.d.name, "rscore1", "raw-gate.txt")) as f:
            self.assertIn("base=" + self.parent, f.read())

    def test_range_flags_exclude_commit_flags(self):
        for argv in (["--base", self.parent, "--target", self.commit, "--commit", self.commit], ["--base", self.parent], []):
            with self.assertRaises(SystemExit):
                with contextlib.redirect_stderr(io.StringIO()):
                    self.quiet(["reference", "--repo", self.repo, "--out", os.path.join(self.d.name, "o")] + argv)


if __name__ == "__main__":
    unittest.main()
