import contextlib
import csv
import io
import os
import tempfile
import unittest

import candidates
import gittest as g


class Candidates(unittest.TestCase):
    def setUp(self):
        self.d = tempfile.TemporaryDirectory()
        self.addCleanup(self.d.cleanup)
        self.repo = g.init(os.path.join(self.d.name, "r"))
        g.commit(self.repo, {"crates/a/Cargo.toml": '[package]\nname = "teamwire-a"\n', "crates/a/src/lib.rs": "fn a() {}\n"}, "old")
        g.commit(self.repo, {"crates/a/tests/old.rs": g.lines(40)}, "qualifying but before the tag")
        g.git(self.repo, "tag", "v0.1.0")
        self.shas = {}

    def run_list(self):
        out = os.path.join(self.d.name, "o.csv")
        with contextlib.redirect_stdout(io.StringIO()):
            candidates.main(["--repo", self.repo, "--out", out])
        with open(out) as f:
            return list(csv.DictReader(f))

    def test_filter(self):
        r = self.repo
        good = g.commit(r, {"crates/a/src/lib.rs": g.lines(20), "crates/a/tests/t.rs": g.lines(20, "t")}, "good rust")
        g.commit(r, {"crates/a/src/lib.rs": g.lines(21), "crates/a/tests/t.rs": g.lines(21, "t")}, "tiny")  # 2 lines changed
        g.commit(r, {"crates/a/src/lib.rs": g.lines(30) + "#[cfg(test)]\nmod tests {\n}\n"}, "inline test only")
        g.commit(r, {"crates/a/src/lib.rs": g.lines(30) + "#[cfg(test)]\nmod tests {\n" + g.lines(30, "u") + "}\n",
                     "crates/a/tests/t.rs": g.lines(5)}, "inline test change")
        g.commit(r, {"Cargo.lock": g.lines(40), "crates/a/tests/t.rs": g.lines(30, "z")}, "protected")
        g.commit(r, {"docs/adr/0001.md": g.lines(40), "crates/a/tests/t.rs": g.lines(31, "z")}, "protected dir")
        g.commit(r, {"crates/a/src/lib.rs": g.lines(300)}, "too large and no tests")
        ui = g.commit(r, {"ui/src/x.ts": g.lines(15), "ui/src/x.test.ts": g.lines(20)}, "good ui")
        g.git(r, "checkout", "-q", "-b", "side")
        g.commit(r, {"crates/a/tests/s.rs": g.lines(50)}, "side work")
        g.git(r, "checkout", "-q", "develop")
        g.commit(r, {"other.txt": "x\n"}, "main work")
        g.git(r, "merge", "-q", "--no-ff", "-m", "merge", "side")
        rows = self.run_list()
        self.assertEqual(sorted(x["subject"] for x in rows), ["good rust", "good ui", "side work"])
        by = {x["subject"]: x for x in rows}
        self.assertEqual(by["good rust"]["sha"], good[:12])
        self.assertEqual(by["good rust"]["test_command"], "cargo test -p teamwire-a --locked")
        self.assertEqual(by["good rust"]["crates"], "a")
        self.assertEqual(by["good rust"]["other_files"], "crates/a/src/lib.rs")
        self.assertEqual(by["good ui"]["test_command"], "cd ui && bun test src/x.test.ts")
        self.assertEqual(by["good ui"]["sha"], ui[:12])

    def test_commit_before_the_tag_is_left_out(self):
        g.commit(self.repo, {"crates/a/tests/t.rs": g.lines(40)}, "after tag")
        self.assertEqual([x["subject"] for x in self.run_list()], ["after tag"])

    def test_unit_tests_in_src_file_disqualify(self):
        src = "fn a() {}\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() {}\n}\n"
        g.commit(self.repo, {"crates/a/src/lib.rs": src}, "base with inline tests")
        g.commit(self.repo, {"crates/a/src/lib.rs": src.replace("fn t() {}", "fn t() { assert!(true); }") + g.lines(1)[:0],
                             "crates/a/tests/t.rs": g.lines(40)}, "edits inline test")
        self.assertEqual(self.run_list(), [])

    def test_added_test_in_any_src_file_disqualifies(self):
        body = "fn a() {}\n\n    #[test]\n    fn t() {}\n" + g.lines(10)
        g.commit(self.repo, {"crates/a/src/cmd/member_tests.rs": body, "crates/a/tests/t.rs": g.lines(40)}, "tests in a differently named file")
        self.assertEqual(self.run_list(), [])

    def test_src_edit_above_inline_tests_is_fine(self):
        src = "fn a() {}\n#[cfg(test)]\nmod tests {\n}\n"
        g.commit(self.repo, {"crates/a/src/lib.rs": src}, "base")
        g.commit(self.repo, {"crates/a/src/lib.rs": g.lines(10) + src, "crates/a/tests/t.rs": g.lines(40)}, "edits above")
        self.assertEqual([x["subject"] for x in self.run_list()], ["edits above"])


if __name__ == "__main__":
    unittest.main()
