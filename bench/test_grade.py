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

BUN = """bun test v1.2.0

src/web/model/a.test.ts:
(pass) groups rows > by role [0.31ms]
(fail) groups rows > by kind [1.20ms]
(skip) later
(todo) maybe

src/web/b.test.tsx:
(pass) renders

 3 pass
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

    def test_bun_results_carry_the_file(self):
        r = grade.parse_bun(BUN)
        self.assertEqual(r["src/web/model/a.test.ts::groups rows > by role"], "pass")
        self.assertEqual(r["src/web/model/a.test.ts::groups rows > by kind"], "fail")
        self.assertEqual(r["src/web/model/a.test.ts::later"], "skip")
        self.assertEqual(r["src/web/b.test.tsx::renders"], "pass")
        self.assertEqual(len(r), 5)

    def test_colour_codes_are_ignored(self):
        self.assertEqual(grade.parse_bun("a.test.ts:\n\x1b[32m(pass)\x1b[0m x [1ms]\n"), {"a.test.ts::x": "pass"})
        self.assertEqual(grade.parse_cargo("\x1b[1mRunning\x1b[0m tests/a.rs (x)\ntest t ... ok\n", "c"), {"c::tests/a.rs::t": "ok"})

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
                                                             "--seed", str(seed), "--out", out])
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
            with open(os.path.join(out, "key.json")) as f:
                key = json.load(f)
            self.assertEqual(sorted(key), sorted(order))
            self.assertEqual({v["task"] for v in key.values()}, {"t1", "t2"})
            with open(os.path.join(out, "diffs", order[0] + ".md")) as f:
                self.assertIn("Brief for", f.read())
            with open(os.path.join(out, "rubric.md")) as f:
                rubric = f.read()
            for w in ("Correctness", "Scope", "Test quality"):
                self.assertIn(w, rubric)
            again = go(7, os.path.join(base, "o2"))
            with open(os.path.join(again, "order.txt")) as f:
                self.assertEqual(f.read().split(), order)
            other = go(8, os.path.join(base, "o3"))
            with open(os.path.join(other, "key.json")) as f:
                self.assertNotEqual(json.load(f), key)


FAKE_CARGO = """#!/bin/sh
echo "     Running tests/t.rs (target/debug/deps/t-1)"
echo "test alpha ... ok"
if grep -q GOOD crates/a/src/lib.rs; then echo "test beta ... ok"; else echo "test beta ... FAILED"; fi
"""
FAKE_GATE = """#!/bin/sh
echo "base=$TEAMWIRE_BASE"
echo "WEAKENED: crates/a/tests/t.rs"
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
        self.parent = g.commit(r, {"crates/a/Cargo.toml": '[package]\nname = "pkg-a"\n', "crates/a/src/lib.rs": "BAD\n"})
        self.commit = g.commit(r, {"crates/a/src/lib.rs": "GOOD\n", "crates/a/tests/t.rs": "t\n"})
        self.bin = os.path.join(self.d.name, "bin")
        os.makedirs(self.bin)
        self.fake_cargo(FAKE_CARGO)
        patcher = mock.patch.dict(os.environ, {"PATH": self.bin + os.pathsep + os.environ["PATH"]})
        patcher.start()
        self.addCleanup(patcher.stop)

    def fake_cargo(self, text):
        p = os.path.join(self.bin, "cargo")
        with open(p, "w") as f:
            f.write(text)
        os.chmod(p, 0o755)

    def quiet(self, argv):
        with contextlib.redirect_stdout(io.StringIO()):
            return grade.main(argv)

    def reference(self):
        out = os.path.join(self.d.name, "ref")
        self.quiet(["reference", "--repo", self.repo, "--commit", self.commit, "--out", out])
        with open(os.path.join(out, "expected.json")) as f:
            return os.path.join(out, "expected.json"), json.load(f)

    def test_reference_with_no_passing_hidden_test_is_an_error(self):
        self.fake_cargo("#!/bin/sh\necho nothing\n")
        with self.assertRaises(SystemExit) as cm:
            self.reference()
        self.assertIn("zero hidden tests", str(cm.exception))

    def test_reference_then_score_a_wrong_run(self):
        path, exp = self.reference()
        self.assertEqual(exp["expected"], ["pkg-a::tests/t.rs::alpha", "pkg-a::tests/t.rs::beta"])
        base = os.path.join(self.d.name, "run")
        os.makedirs(base)
        with contextlib.redirect_stdout(io.StringIO()):
            import clone as clonemod
            clonemod.main(["--repo", self.repo, "--parent", self.parent, "--task", "t1", "--setup", "solo", "--dir", base])
        with open(os.path.join(base, "clone", "junk.txt"), "w") as f:
            f.write("outside\n")
        out = os.path.join(self.d.name, "score")
        self.quiet(["score", "--repo", self.repo, "--commit", self.commit, "--run-dir", base, "--expected", path, "--out", out])
        with open(os.path.join(out, "score.json")) as f:
            sc = json.load(f)
        self.assertEqual(sc["tests"], {"pkg-a::tests/t.rs::alpha": "pass", "pkg-a::tests/t.rs::beta": "fail"})
        self.assertEqual((sc["hidden_passed"], sc["hidden_total"]), (1, 2))
        self.assertEqual(sc["outside_files"], ["junk.txt"])
        self.assertEqual(sc["gate"]["exit"], 1)
        self.assertEqual(sc["gate"]["flagged_lines"], ["WEAKENED: crates/a/tests/t.rs"])
        with open(os.path.join(out, "raw-gate.txt")) as f:
            self.assertIn("base=" + self.parent, f.read())  # the gate got TEAMWIRE_BASE
        with open(os.path.join(out, "diff.patch")) as f:
            diff = f.read()
        self.assertIn("junk.txt", diff)
        self.assertNotIn("tests/t.rs", diff)  # the hidden test was copied in after the diff was taken


if __name__ == "__main__":
    unittest.main()
