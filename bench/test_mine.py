import contextlib
import csv
import io
import json
import os
import sqlite3
import tempfile
import unittest

import mine

SCHEMA = """
create table team (name text, title text, lead_session_id text, created_at text);
create table members (role text, kind text);
create table completions (role text, status text);
create table messages (ulid text, type text);
create table events (seq integer primary key, time text, type text, role text, subject text, data text);
"""


def make_db(path, name="demo", events=(), members=(), completions=(), messages=(), lead="lead-1"):
    c = sqlite3.connect(path)
    c.executescript(SCHEMA)
    c.execute("insert into team values (?,?,?,?)", (name, "A title", lead, "2026-10-20T10:00:00.000Z"))
    c.executemany("insert into members values (?,?)", members)
    c.executemany("insert into completions values (?,?)", completions)
    c.executemany("insert into messages values (?,?)", messages)
    for i, (t, ts, role, subj, data) in enumerate(events, 1):
        c.execute("insert into events values (?,?,?,?,?,?)", (i, ts, t, role, subj, json.dumps(data)))
    c.commit()
    c.close()


T0, T1 = "2026-10-20T10:00:00.000Z", "2026-10-20T10:10:00.000Z"
USAGE = {"claude-opus-5-5": {"input": 10, "output": 20, "cache_read": 30, "cache_creation": 40}}


class Mine(unittest.TestCase):
    def setUp(self):
        self.d = tempfile.TemporaryDirectory()
        self.addCleanup(self.d.cleanup)
        self.archive = os.path.join(self.d.name, "archive")
        self.projects = os.path.join(self.d.name, "projects")
        os.makedirs(os.path.join(self.projects, "p"))

    def add_team(self, dirname, **kw):
        os.makedirs(os.path.join(self.archive, dirname))
        make_db(os.path.join(self.archive, dirname, "team.db"), **kw)

    def run_mine(self):
        out = os.path.join(self.d.name, "out", "m.csv")
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(mine.main(["--archive", self.archive, "--projects", self.projects, "--out", out]), 0)
        with open(out) as f:
            return list(csv.DictReader(f))

    def test_row_counts_and_rates(self):
        ev = [("team.created", T0, None, "t", {}),
              ("member.reprompted", "2026-10-20T10:02:00.000Z", "a", None, {}),
              ("verify.recorded", "2026-10-20T10:03:00.000Z", "a", None, {"verdict": "FAIL", "flags": ["weakened:x.rs", "weakened:y.rs"]}),
              ("verify.recorded", "2026-10-20T10:04:00.000Z", "a", None, {"verdict": "PASS", "flags": []}),
              ("verify.recorded", "2026-10-20T10:04:00.000Z", "b", None, {"verdict": "PASS", "flags": []}),
              ("watch.line", "2026-10-20T10:05:00.000Z", None, None, {}),
              ("delivery.handed_off", "2026-10-20T10:06:00.000Z", "a", "q1", {"to": "lead"}),
              ("delivery.handed_off", "2026-10-20T10:06:00.000Z", "a", "n1", {"to": "lead"}),
              ("result.delivered", "2026-10-20T10:07:00.000Z", "a", None, {"route": "host_typing"}),
              ("result.delivered", "2026-10-20T10:07:00.000Z", "b", None, {"route": "watch"}),
              ("team.closed", T1, None, None, {})]
        self.add_team("demo-2026-10-20", events=ev, members=[("a", "implementer"), ("b", "implementer"), ("r", "reviewer")],
                      completions=[("a", "done"), ("b", "done"), ("r", "blocked")],
                      messages=[("q1", "question"), ("n1", "note")])
        r = self.run_mine()[0]
        self.assertEqual((r["members_implementer"], r["members_reviewer"], r["wall_s"]), ("2", "1", "600.0"))
        self.assertEqual((r["completions_done"], r["completions_blocked"]), ("2", "1"))
        self.assertEqual((r["reprompts"], r["implementers_reprompted"], r["fix_round_rate"]), ("1", "1", "0.5"))
        self.assertEqual((r["verify_first_pass_share"], r["verify_flags"]), ("0.5", "weakened=2"))
        self.assertEqual((r["wakeups_watch"], r["wakeups_question"], r["wakeups_result"]), ("1", "1", "1"))
        self.assertEqual(r["member_input"], "")  # no tokens recorded: empty, not 0
        self.assertEqual(r["lead_transcripts"], "")

    def test_excluded_names_come_from_the_team_row_not_the_folder(self):
        for n in ["smoke1", "twdummy3", "twtest", "tmuxsmoke2", "unreadsmoke", "holdchk", "plugtest", "bn1t", "real"]:
            self.add_team("folder-" + n.upper(), name=n)
        self.assertEqual([r["team"] for r in self.run_mine()], ["real"])

    def test_member_tokens_take_newest_total_per_session_then_sum(self):
        def deliver(sess, usage, ts):
            return ("result.delivering", ts, "a", None, {"session": sess, "usage": usage})
        small = {"claude-opus-5-5": {"input": 1, "output": 1, "cache_read": 1, "cache_creation": 1}}
        ev = [deliver("s1", small, T0), deliver("s1", USAGE, T1), deliver("s2", USAGE, T1),
              deliver("s2", dict(USAGE, **{"<synthetic>": {"input": 0}}), T1)]
        self.add_team("demo-2026-10-20", events=ev)
        r = self.run_mine()[0]
        self.assertEqual((r["member_input"], r["member_output"], r["member_cache_creation"]), ("20", "40", "80"))
        # 2 sessions x (10 in, 20 out, 30 read, 40 write@1h rate): opus-5-5
        self.assertEqual(float(r["member_usd"]), round(2 * (10 * 4 + 20 * 20 + 30 * 0.2 + 40 * 8) / 1e6, 4))

    def test_lead_tokens_only_inside_the_team_window_and_from_adopted_sessions(self):
        def msg(i, ts):
            return json.dumps({"type": "assistant", "timestamp": ts, "cwd": "/x", "message": {
                "id": i, "model": "claude-opus-5-5", "usage": {"input_tokens": 5, "output_tokens": 1}}})
        for sid, lines in {"lead-1": [msg("a", "2026-10-20T09:00:00.000Z"), msg("b", "2026-10-20T10:05:00.000Z")],
                           "old-1": [msg("c", "2026-10-20T10:06:00.000Z"), msg("c", "2026-10-20T10:06:00.000Z")]}.items():
            with open(os.path.join(self.projects, "p", sid + ".jsonl"), "w") as f:
                f.write("\n".join(lines) + "\n")
        ev = [("team.created", T0, None, "t", {}), ("team.lead_adopted", T0, None, None, {"old_session": "old-1", "new_session": "lead-1"}),
              ("team.closed", T1, None, None, {})]
        self.add_team("demo-2026-10-20", events=ev)
        r = self.run_mine()[0]
        self.assertEqual((r["lead_input"], r["lead_transcripts"]), ("10", "2"))

    def test_unpriced_model_is_named_and_leaves_usd_empty(self):
        self.add_team("demo-2026-10-20", events=[("result.delivering", T0, "a", None, {"session": "s", "usage": {"claude-x-1": {"input": 1}}})])
        r = self.run_mine()[0]
        self.assertIn("claude-x-1", r["price_error"])
        self.assertEqual(r["member_usd"], "")


if __name__ == "__main__":
    unittest.main()
