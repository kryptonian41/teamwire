import contextlib
import io
import json
import os
import tempfile
import unittest

import common
import cost


def msg(i, cwd, model="claude-opus-5-5", inp=1_000_000):
    return json.dumps({"type": "assistant", "cwd": cwd, "timestamp": "2026-10-22T10:00:00.000Z", "message": {
        "id": i, "model": model, "usage": {"input_tokens": inp, "output_tokens": 0}}})


class Cost(unittest.TestCase):
    def setUp(self):
        self.d = tempfile.TemporaryDirectory()
        self.addCleanup(self.d.cleanup)
        root = os.path.realpath(self.d.name)
        self.clone = os.path.join(root, "real", "clone")
        os.makedirs(self.clone)
        self.link = os.path.join(root, "link")
        os.symlink(os.path.join(root, "real"), self.link)  # like /tmp -> /private/tmp
        self.projects = os.path.join(root, "projects")
        self.out = os.path.join(root, "out")

    def write(self, rel, *lines):
        p = os.path.join(self.projects, rel)
        os.makedirs(os.path.dirname(p), exist_ok=True)
        with open(p, "w") as f:
            f.write("\n".join(lines) + "\n")
        return p

    def run_cost(self, *extra, clone=None):
        err = io.StringIO()
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(err):
            code = cost.main(["--clone", clone or os.path.join(self.link, "clone"), "--projects", self.projects, "--out-dir", self.out] + list(extra))
        return code, err.getvalue()

    def result(self):
        with open(os.path.join(self.out, "cost-clone.json")) as f:
            return json.load(f)

    def test_finds_by_cwd_through_a_symlink_and_counts_each_message_once(self):
        wt = os.path.join(self.clone, ".worktrees", "member")
        self.write("p1/lead.jsonl", msg("m1", self.clone), msg("m1", self.clone), msg("m2", self.clone))
        self.write("p2/member.jsonl", msg("m3", wt, "claude-sonnet-5-5"))
        self.write("p3/other.jsonl", msg("x", "/somewhere/else"))
        self.write("p4/near.jsonl", msg("y", self.clone + "-sibling"))  # a name prefix is not "under"
        # a subagent beside the lead's session, repeating one parent message and adding one
        self.write("p1/lead/subagents/agent-1.jsonl", msg("m2", "/elsewhere"), msg("m4", "/elsewhere"))
        code, _ = self.run_cost()
        self.assertEqual(code, 0)
        r = self.result()
        self.assertEqual(len(r["files"]), 3)
        self.assertEqual(r["by_model"]["claude-opus-5-5"]["tokens"]["input"], 3_000_000)  # m1, m2, m4
        self.assertEqual(r["by_model"]["claude-sonnet-5-5"]["tokens"]["input"], 1_000_000)
        self.assertAlmostEqual(r["total_usd"], 3 * 4.0 + 2.0)
        self.assertAlmostEqual(sum(f["usd"] for f in r["files"]), r["total_usd"])

    def test_extra_session_id_and_its_subagents_are_included(self):
        self.write("p1/lead.jsonl", msg("m1", self.clone))
        self.write("cache/closer-sess.jsonl", msg("c1", "/cache/closer"))
        self.write("cache/closer-sess/subagents/agent-2.jsonl", msg("c2", "/cache/closer"))
        self.run_cost()
        self.assertAlmostEqual(self.result()["total_usd"], 4.0)
        self.run_cost("--session", "closer-sess")
        self.assertAlmostEqual(self.result()["total_usd"], 12.0)

    def test_claude_json_total_is_shown_beside_ours(self):
        self.write("p1/lead.jsonl", msg("m1", self.clone))
        j = os.path.join(self.d.name, "c.json")
        with open(j, "w") as f:
            json.dump({"type": "result", "total_cost_usd": 3.5}, f)
        self.run_cost("--claude-json", j)
        r = self.result()
        self.assertEqual((r["claude_total_cost_usd"], r["difference_usd"]), (3.5, 0.5))

    def test_unknown_model_fails_and_names_the_id(self):
        self.write("p1/lead.jsonl", msg("m1", self.clone, "claude-mystery-9"))
        code, err = self.run_cost()
        self.assertEqual(code, 1)
        self.assertIn("claude-mystery-9", err)

    def test_no_transcript_is_an_error(self):
        os.makedirs(self.projects)
        self.assertEqual(self.run_cost()[0], 1)


if __name__ == "__main__":
    unittest.main()
