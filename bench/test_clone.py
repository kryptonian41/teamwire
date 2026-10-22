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
        base = os.path.join(self.d.name, "run1")
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(clone.main(["--repo", self.repo, "--parent", self.c2, "--task", "t1", "--setup", "solo", "--dir", base]), 0)
        self.assertEqual(self.load(os.path.join(base, "settings.json")), {"enabledPlugins": {"claude-mem@thedotmack": False}})
        run = self.load(os.path.join(base, "run.json"))
        self.assertEqual((run["task"], run["setup"], run["parent"]), ("t1", "solo", self.c2))
        self.assertEqual(run["clone"], os.path.join(os.path.realpath(base), "clone"))
        self.assertRegex(run["started_at"], r"^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ$")
        self.assertTrue(os.path.isdir(os.path.join(run["clone"], ".git")))

    def test_main_fails_on_an_unknown_parent(self):
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(clone.main(["--repo", self.repo, "--parent", "0" * 40, "--task", "t", "--setup", "s", "--dir", os.path.join(self.d.name, "r")]), 1)


if __name__ == "__main__":
    unittest.main()
