import json
import os
import tempfile
import unittest

import common


def line(mid, model="claude-opus-5-5", ts="2026-10-20T10:00:00.000Z", cwd="/x", **usage):
    u = {"input_tokens": 1, "output_tokens": 2, "cache_read_input_tokens": 3,
         "cache_creation_input_tokens": 0}
    u.update(usage)
    return json.dumps({"type": "assistant", "timestamp": ts, "cwd": cwd, "requestId": "req_" + str(mid),
                       "message": {"id": mid, "model": model, "usage": u}})


class Transcript(unittest.TestCase):
    def setUp(self):
        self.d = tempfile.TemporaryDirectory()
        self.addCleanup(self.d.cleanup)

    def write(self, name, *lines):
        p = os.path.join(self.d.name, name)
        with open(p, "w") as f:
            f.write("\n".join(lines) + "\n")
        return p

    def test_blocks_of_one_message_count_once(self):
        p = self.write("a.jsonl", line("m1"), line("m1"), line("m1"), line("m2"))
        t = common.merge_models(common.collect([p]))["claude-opus-5-5"]
        self.assertEqual((t["input"], t["output"], t["cache_read"]), (2, 4, 6))

    def test_last_line_of_a_message_wins(self):
        # a subagent file writes a placeholder output count on early lines and the real one on the last
        p = self.write("a.jsonl", line("m1", output_tokens=5), line("m1", output_tokens=5), line("m1", output_tokens=608),
                       line("m2", output_tokens=7))
        t = common.merge_models(common.collect([p]))["claude-opus-5-5"]
        self.assertEqual(t["output"], 615)

    def test_message_in_parent_and_subagent_counts_once(self):
        a = self.write("a.jsonl", line("m1"))
        b = self.write("b.jsonl", line("m1"), line("m2"))
        r = common.collect([a, b])
        self.assertEqual(r[a]["claude-opus-5-5"]["input"], 1)
        self.assertEqual(r[b]["claude-opus-5-5"]["input"], 1)

    def test_request_id_is_the_fallback_key(self):
        rec = json.loads(line("x"))
        del rec["message"]["id"]
        s = json.dumps(rec)
        p = self.write("a.jsonl", s, s)
        self.assertEqual(common.merge_models(common.collect([p]))["claude-opus-5-5"]["input"], 1)

    def test_synthetic_and_non_assistant_lines_are_skipped(self):
        p = self.write("a.jsonl", line("m1", model="<synthetic>"),
                       json.dumps({"type": "user", "message": {"role": "user"}}), "not json")
        self.assertEqual(common.collect([p]), {p: {}})

    def test_cache_write_split_and_unsplit(self):
        split = line("m1", cache_creation_input_tokens=10,
                     cache_creation={"ephemeral_5m_input_tokens": 4, "ephemeral_1h_input_tokens": 6})
        unsplit = line("m2", cache_creation_input_tokens=7)
        p = self.write("a.jsonl", split, unsplit)
        t = common.merge_models(common.collect([p]))["claude-opus-5-5"]
        self.assertEqual((t["cache_write_5m"], t["cache_write_1h"], t["cache_creation_unsplit"]), (4, 6, 7))

    def test_time_window(self):
        p = self.write("a.jsonl", line("m1", ts="2026-10-20T09:00:00.000Z"),
                       line("m2", ts="2026-10-20T11:00:00.000Z"), line("m3", ts="2026-10-20T13:00:00.000Z"))
        t = common.merge_models(common.collect([p], "2026-10-20T10:00:00.000Z", "2026-10-20T12:00:00.000Z"))
        self.assertEqual(t["claude-opus-5-5"]["input"], 1)

    def test_advisor_iteration_is_priced_under_its_own_model(self):
        rec = json.loads(line("m1"))
        rec["message"]["usage"]["iterations"] = [
            {"type": "message", "input_tokens": 1, "output_tokens": 2},
            {"type": "advisor_message", "model": "claude-fable-5-1", "input_tokens": 100, "output_tokens": 5}]
        p = self.write("a.jsonl", json.dumps(rec))
        t = common.merge_models(common.collect([p]))
        self.assertEqual(t["claude-fable-5-1"]["input"], 100)
        self.assertEqual(t["claude-opus-5-5"]["input"], 1)

    def test_transcript_cwd(self):
        p = self.write("a.jsonl", json.dumps({"type": "user"}), line("m1", cwd="/tmp/c"))
        self.assertEqual(common.transcript_cwd(p), "/tmp/c")


class Prices(unittest.TestCase):
    def test_known_model_costs_per_type(self):
        pr = common.load_prices()
        toks = {"input": 1_000_000, "output": 1_000_000, "cache_read": 1_000_000,
                "cache_write_5m": 1_000_000, "cache_write_1h": 1_000_000, "cache_creation_unsplit": 1_000_000}
        # opus-5-5: 4 + 20 + 0.2 + 5 + 8 + 8 (unsplit is priced at the 1h rate)
        self.assertAlmostEqual(common.usd(pr, "claude-opus-5-5", toks), 45.2)

    def test_unknown_model_names_the_id(self):
        with self.assertRaises(common.PriceError) as cm:
            common.usd(common.load_prices(), "claude-mystery-9", {"input": 1})
        self.assertIn("claude-mystery-9", str(cm.exception))

    def test_alias_matches_only_when_listed(self):
        pr = common.load_prices()
        self.assertEqual(common.base_model(pr, "claude-opus-5-5[1m]"), "claude-opus-5-5")
        self.assertEqual(common.base_model(pr, "claude-haiku-4-5-20251001"), "claude-haiku-4-5")
        with self.assertRaises(common.PriceError):
            common.base_model(pr, "claude-opus-5-5-20260101")

    def test_table_has_source_and_date(self):
        pr = common.load_prices()
        self.assertTrue(pr["source"] and pr["read_on"] == "2026-10-26")


if __name__ == "__main__":
    unittest.main()
