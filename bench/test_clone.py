import contextlib
import io
import json
import os
import tempfile
import unittest

import clone
import gittest as g


class Clone(unittest.TestCase):
    def setUp(self):
        self.d = tempfile.TemporaryDirectory()
        self.addCleanup(self.d.cleanup)
        self.repo = g.init(os.path.join(self.d.name, "src"))
        self.c1 = g.commit(self.repo, {"a.txt": "1\n"})
        self.c2 = g.commit(self.repo, {"a.txt": "2\n"})
        self.c3 = g.commit(self.repo, {"answer.txt": "secret\n"})
        g.git(self.repo, "branch", "later", self.c3)
        g.git(self.repo, "tag", "v9")

    @staticmethod
    def load(path):
        with open(path) as f:
            return json.load(f)

    def refs(self):
        return g.git(self.repo, "for-each-ref")

    def test_clone_holds_nothing_past_the_parent(self):
        before = self.refs()
        dest = os.path.join(self.d.name, "out", "clone")
        os.makedirs(os.path.dirname(dest))
        self.assertEqual(clone.make_clone(self.repo, self.c2, dest), self.c2)
        self.assertEqual(g.git(dest, "rev-list", "--all").split(), [self.c2, self.c1])
        self.assertEqual(g.git(dest, "branch", "--show-current"), "bench")
        self.assertEqual(g.git(dest, "remote"), "")
        self.assertFalse(os.path.exists(os.path.join(dest, "answer.txt")))
        with self.assertRaises(RuntimeError):
            g.git(dest, "cat-file", "-e", self.c3)
        self.assertEqual(self.refs(), before)  # no branch or tag was made in the source
        clone.verify_no_paths(dest, [self.repo])

    def test_verify_catches_a_later_commit_a_remote_and_a_path(self):
        full = os.path.join(self.d.name, "full")
        g.git(self.d.name, "clone", "-q", self.repo, full)  # holds c3 and has origin
        with self.assertRaises(clone.LeakError):
            clone.verify_clone(full, self.c2)
        with self.assertRaises(clone.LeakError):
            clone.verify_no_paths(full, [self.repo])  # .git/config names the source as origin
        g.git(full, "remote", "remove", "origin")
        with self.assertRaises(clone.LeakError):
            clone.verify_clone(full, self.c2)  # c3 is still stored

    def test_unreachable_later_commit_is_caught(self):
        dest = os.path.join(self.d.name, "clone")
        clone.make_clone(self.repo, self.c2, dest)
        g.git(dest, "fetch", "-q", self.repo, self.c3)  # object arrives, then the ref goes away
        g.git(dest, "reflog", "expire", "--expire=now", "--all")
        with self.assertRaises(clone.LeakError):
            clone.verify_clone(dest, self.c2)

    def test_main_writes_settings_and_run_json_outside_the_clone(self):
        root = os.path.join(self.d.name, "runs")
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(clone.main(["--repo", self.repo, "--commit", self.c3, "--task", "t1", "--setup", "solo", "--root", root]), 0)
        (base,) = os.listdir(root)
        self.assertTrue(base.startswith("bn"))
        self.assertNotIn("solo", base)
        base = os.path.join(root, base)
        self.assertEqual(self.load(os.path.join(base, "settings.json")), {"enabledPlugins": {"claude-mem@thedotmack": False}})
        run = self.load(os.path.join(base, "run.json"))
        self.assertEqual((run["task"], run["setup"], run["parent"], run["commits"]), ("t1", "solo", self.c2, [self.c3]))
        self.assertEqual(run["clone"], os.path.join(os.path.realpath(base), "clone"))
        self.assertRegex(run["started_at"], r"^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ$")
        self.assertTrue(os.path.isdir(os.path.join(run["clone"], ".git")))

    def test_two_runs_get_different_random_folders(self):
        root = os.path.join(self.d.name, "runs")
        with contextlib.redirect_stdout(io.StringIO()):
            for setup in ("solo", "team"):
                clone.main(["--repo", self.repo, "--commit", self.c3, "--task", "t", "--setup", setup, "--root", root])
        self.assertEqual(len(os.listdir(root)), 2)

    def test_refuses_when_a_task_commit_is_in_the_parents_history(self):
        # c1 is an ancestor of c2, the parent of c3: a clone for [c3, c1] would let `git show c1` print it
        root = os.path.join(self.d.name, "runs")
        with contextlib.redirect_stderr(io.StringIO()) as err:
            self.assertEqual(clone.main(["--repo", self.repo, "--commit", self.c3, "--commit", self.c1, "--task", "t", "--setup", "s", "--root", root]), 1)
        self.assertIn(self.c1[:12], err.getvalue())
        self.assertFalse(os.path.exists(root))  # refused before any folder is made

    def test_main_fails_on_an_unknown_commit(self):
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(clone.main(["--repo", self.repo, "--commit", "0" * 40, "--task", "t", "--setup", "s", "--root", os.path.join(self.d.name, "r")]), 1)


class Range(unittest.TestCase):
    def setUp(self):
        self.d = tempfile.TemporaryDirectory()
        self.addCleanup(self.d.cleanup)
        self.repo = g.init(os.path.join(self.d.name, "src"))
        self.base = g.commit(self.repo, {"a.txt": "1\n"})
        g.git(self.repo, "checkout", "-q", "-b", "side")
        g.commit(self.repo, {"side.txt": "s\n"})
        g.git(self.repo, "checkout", "-q", "develop")
        g.commit(self.repo, {"main.txt": "m\n"})
        g.git(self.repo, "merge", "-q", "--no-ff", "-m", "merge", "side")
        self.target = g.git(self.repo, "rev-parse", "HEAD")
        self.after = g.commit(self.repo, {"later.txt": "l\n"})
        self.root = os.path.join(self.d.name, "runs")

    def run_main(self, *args):
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()) as err:
            code = clone.main(["--repo", self.repo, "--task", "t", "--setup", "s", "--root", self.root] + list(args))
        return code, err.getvalue()

    def test_clone_ends_at_the_start_and_run_json_records_the_range(self):
        self.assertEqual(self.run_main("--base", self.base, "--target", self.target)[0], 0)
        (name,) = os.listdir(self.root)
        with open(os.path.join(self.root, name, "run.json")) as f:
            run = json.load(f)
        self.assertEqual((run["base"], run["target"], run["parent"]), (self.base, self.target, self.base))
        self.assertEqual(g.git(run["clone"], "rev-list", "--all").split(), [self.base])
        self.assertEqual(g.git(run["clone"], "rev-parse", "HEAD"), self.base)
        for later in (self.target, self.after):
            with self.assertRaises(RuntimeError):
                g.git(run["clone"], "cat-file", "-e", later)

    def test_refuses_a_start_that_is_not_an_ancestor_of_the_end(self):
        for base, target in ((self.after, self.target), (self.target, self.target)):
            code, err = self.run_main("--base", base, "--target", target)
            self.assertEqual(code, 1)
            self.assertIn("not an ancestor", err)
        self.assertFalse(os.path.exists(self.root))

    def test_range_and_commit_modes_exclude_each_other(self):
        for args in (["--base", self.base, "--target", self.target, "--commit", self.target], ["--base", self.base], []):
            with self.assertRaises(SystemExit):
                self.run_main(*args)


if __name__ == "__main__":
    unittest.main()
