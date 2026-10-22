import contextlib
import io
import json
import os
import tempfile
import unittest

import leaks


def call(name, **inp):
    return json.dumps({"type": "assistant", "cwd": "CLONE", "message": {"id": "m", "model": "x", "content": [
        {"type": "text", "text": "reading /Users/me/src/secret in prose is not a tool call"},
        {"type": "tool_use", "id": "t", "name": name, "input": inp}]}})


class Leaks(unittest.TestCase):
    def setUp(self):
        self.d = tempfile.TemporaryDirectory()
        self.addCleanup(self.d.cleanup)
        root = os.path.realpath(self.d.name)
        self.home = os.path.join(root, "home")
        self.source = os.path.join(root, "src", "teamwire")
        self.runs = os.path.join(root, "runs")
        self.clone = os.path.join(self.runs, "bn111", "clone")
        for p in (self.home, self.source, self.clone, os.path.join(self.runs, "bn222", "clone")):
            os.makedirs(p)
        self.projects = os.path.join(root, "projects")

    def transcript(self, *lines):
        p = os.path.join(self.projects, "p", "s.jsonl")
        os.makedirs(os.path.dirname(p), exist_ok=True)
        with open(p, "w") as f:
            f.write("\n".join(l.replace("CLONE", self.clone) for l in lines) + "\n")
        return p

    def scan(self, *lines, allow=()):
        p = self.transcript(*lines)
        return leaks.scan([p], [self.source, os.path.join(self.home, ".teamwire/archive"), os.path.join(self.home, ".claude/projects"), self.runs],
                          allow, home=self.home, skip_inside=[os.path.dirname(self.clone)])

    def test_bash_read_and_grep_naming_off_limits_paths_are_hits(self):
        hits = self.scan(
            call("Bash", command="cd %s && git log --all" % self.source),
            call("Read", file_path=os.path.join(self.home, ".teamwire/archive/x-2026/team.db")),
            call("Grep", pattern="answer", path=os.path.join(self.runs, "bn222", "clone")),
            call("Bash", command="ls ~/.claude/projects | head"),
            call("Bash", command="cat $HOME/.teamwire/archive/a/briefs/b.md"))
        self.assertEqual([(h[1], h[2]) for h in hits], [(1, "Bash"), (2, "Read"), (3, "Grep"), (4, "Bash"), (5, "Bash")])
        self.assertEqual(hits[0][3], self.source)
        self.assertEqual(hits[3][3], "~/.claude/projects")

    def test_paths_inside_the_clone_and_the_runs_own_folder_and_prose_are_not_hits(self):
        self.assertEqual(self.scan(
            call("Read", file_path=os.path.join(self.clone, "src/lib.rs")),
            call("Bash", command="ls %s/bn111" % self.runs),
            call("Bash", command="ls %s-other" % self.source),  # a longer name is not the source
            call("Edit", file_path=os.path.join(self.clone, "a.rs"), new_string="x")), [])

    def test_allowed_path_is_not_a_hit(self):
        home = os.path.join(self.home, ".teamwire/archive/mine")
        line = call("Read", file_path=os.path.join(home, "state.db"))
        self.assertEqual(len(self.scan(line)), 1)
        self.assertEqual(self.scan(line, allow=[home]), [])

    def test_main_exit_code(self):
        self.transcript(call("Bash", command="ls " + self.source))
        argv = ["--clone", self.clone, "--source", self.source, "--projects", self.projects]
        with contextlib.redirect_stdout(io.StringIO()) as out:
            self.assertEqual(leaks.main(argv), 1)
        self.assertIn("LEAK", out.getvalue())
        self.assertIn("Bash", out.getvalue())
        self.transcript(call("Bash", command="ls ."))
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(leaks.main(argv), 0)


if __name__ == "__main__":
    unittest.main()
