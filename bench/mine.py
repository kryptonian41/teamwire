#!/usr/bin/env python3
"""Mine archived teamwire teams into one CSV row per team (plan section 4).

Reads every <archive>/*/team.db read-only and the lead transcripts under ~/.claude/projects.
Writes only the CSV. An empty cell means "not recorded", never 0.
"""
import argparse
import csv
import fnmatch
import json
import os
import re
import sqlite3
import sys

import common

# Matched against team.name. Benchmark teams from part two start with "bn".
EXCLUDED = ["smoke*", "twdummy*", "twtest", "tmuxsmoke*", "unreadsmoke", "holdchk", "plugtest", "bn*"]
# "closer" and "survey" are left out: they are plain words in most briefs, so a match proves nothing.
PRESETS = ["implement-then-review", "parallel-tickets", "dependent-tickets", "contract-split"]
KINDS = ["implementer", "reviewer", "investigator", "closer"]
TOKEN_TYPES = ["input", "output", "cache_read", "cache_creation"]

COLUMNS = (
    ["team", "archive", "date", "title", "shape"]
    + ["members_" + k for k in KINDS]
    + ["wall_s", "completions_done", "completions_blocked", "completions_failed", "reprompts",
       "implementers_reprompted", "fix_round_rate", "verify_first_pass_share", "verify_flags",
       "wakeups_result", "wakeups_question", "wakeups_watch"]
    + ["member_" + t for t in TOKEN_TYPES] + ["member_models", "member_usd"]
    + ["lead_" + t for t in TOKEN_TYPES] + ["lead_models", "lead_usd", "lead_transcripts", "lead_shared_with", "price_error"]
)


def is_excluded(name):
    return any(fnmatch.fnmatchcase(name, p) for p in EXCLUDED)


def open_ro(path):
    return sqlite3.connect("file:%s?mode=ro&immutable=1" % path, uri=True)


def seconds_between(a, b):
    from datetime import datetime
    f = lambda s: datetime.strptime(s, "%Y-%m-%dT%H:%M:%S.%fZ")
    return round((f(b) - f(a)).total_seconds(), 1)


def member_tokens(events):
    """result.delivering carries running totals per session: keep the newest per session, sum sessions."""
    newest = {}
    for ev in events:  # in seq order
        if ev["type"] == "result.delivering" and ev["data"].get("session") and ev["data"].get("usage"):
            newest[ev["data"]["session"]] = ev["data"]["usage"]
    if not newest:
        return None
    out = {}
    for usage in newest.values():
        for model, u in usage.items():
            if model == "<synthetic>":  # carries no billable usage
                continue
            t = out.setdefault(model, dict.fromkeys(common.TYPES, 0))
            t["input"] += u.get("input", 0)
            t["output"] += u.get("output", 0)
            t["cache_read"] += u.get("cache_read", 0)
            t["cache_creation_unsplit"] += u.get("cache_creation", 0)
    return out


def index_transcripts(projects):
    """session id -> [main file, subagent files...]"""
    idx = {}
    if not os.path.isdir(projects):
        return idx
    for proj in os.scandir(projects):
        if not proj.is_dir():
            continue
        for e in os.scandir(proj.path):
            if e.is_file() and e.name.endswith(".jsonl"):
                idx.setdefault(e.name[:-6], []).insert(0, e.path)
            elif e.is_dir():
                sub = os.path.join(e.path, "subagents")
                if os.path.isdir(sub):
                    idx.setdefault(e.name, []).extend(
                        os.path.join(sub, n) for n in sorted(os.listdir(sub)) if n.endswith(".jsonl"))
    return idx


def fill_tokens(row, prefix, by_model, prices):
    """Write token and usd cells from {model: {type: n}}; a model without a price blanks the usd."""
    if not by_model:
        return
    for t in TOKEN_TYPES:
        if t == "cache_creation":
            row[prefix + t] = sum(v[k] for v in by_model.values()
                                  for k in ("cache_write_5m", "cache_write_1h", "cache_creation_unsplit"))
        else:
            row[prefix + t] = sum(v[t] for v in by_model.values())
    row[prefix + "models"] = ";".join(sorted(by_model))
    try:
        row[prefix + "usd"] = round(common.total_usd(prices, by_model), 4)
    except common.PriceError as e:
        row["price_error"] = (row.get("price_error", "") + " " + str(e)).strip()


def team_row(db_path, archive_name, prices, transcripts):
    c = open_ro(db_path)
    c.row_factory = sqlite3.Row
    team = c.execute("select * from team").fetchone()
    events = [{"seq": r["seq"], "time": r["time"], "type": r["type"], "role": r["role"],
               "subject": r["subject"], "data": json.loads(r["data"])}
              for r in c.execute("select * from events order by seq")]
    row = dict.fromkeys(COLUMNS, "")
    row.update(team=team["name"], archive=archive_name, date=team["created_at"][:10], title=team["title"])

    members = c.execute("select role, kind from members").fetchall()
    kind_of = {m["role"]: m["kind"] for m in members}
    for k in KINDS:
        row["members_" + k] = sum(1 for m in members if m["kind"] == k)

    brief_text = ""
    for sub in ("briefs", "contract"):
        d = os.path.join(os.path.dirname(db_path), sub)
        if os.path.isdir(d):
            for n in sorted(os.listdir(d)):
                with open(os.path.join(d, n), errors="replace") as f:
                    brief_text += f.read()
    found = [p for p in PRESETS if re.search(r"(?<![\w-])%s(?![\w-])" % re.escape(p), brief_text)]
    row["shape"] = found[0] if found else ""

    created = next((e["time"] for e in events if e["type"] == "team.created"), team["created_at"])
    closed = next((e["time"] for e in events if e["type"] == "team.closed"), None)
    if closed:
        row["wall_s"] = seconds_between(created, closed)

    for s in ("done", "blocked", "failed"):
        row["completions_" + s] = c.execute("select count(*) from completions where status=?", (s,)).fetchone()[0]

    reprompts = [e for e in events if e["type"] == "member.reprompted"]
    implementers = [r for r, k in kind_of.items() if k == "implementer"]
    row["reprompts"] = len(reprompts)
    reprompted = {e["role"] for e in reprompts if kind_of.get(e["role"]) == "implementer"}
    row["implementers_reprompted"] = len(reprompted)
    # Section 2.7 of the field report counts fix rounds only for implementers on a team with a reviewer or closer.
    if implementers and (row["members_reviewer"] or row["members_closer"]):
        row["fix_round_rate"] = round(len(reprompted) / len(implementers), 3)

    first = {}
    flags = {}
    for e in events:
        if e["type"] != "verify.recorded":
            continue
        first.setdefault(e["role"], e["data"].get("verdict"))
        for f in e["data"].get("flags") or []:
            k = f.split(":")[0]
            flags[k] = flags.get(k, 0) + 1
    if first:
        row["verify_first_pass_share"] = round(sum(1 for v in first.values() if v == "PASS") / len(first), 3)
    row["verify_flags"] = ";".join("%s=%d" % kv for kv in sorted(flags.items()))

    msg_type = {r["ulid"]: r["type"] for r in c.execute("select ulid, type from messages")}
    row["wakeups_watch"] = sum(1 for e in events if e["type"] == "watch.line")
    row["wakeups_result"] = sum(1 for e in events if e["type"] == "result.delivered" and e["data"].get("route") != "watch")
    row["wakeups_question"] = sum(1 for e in events if e["type"] == "delivery.handed_off"
                                  and e["data"].get("to") == "lead" and msg_type.get(e["subject"]) == "question")

    fill_tokens(row, "member_", member_tokens(events), prices)

    sessions = {team["lead_session_id"]} - {None}
    for e in events:
        if e["type"] == "team.lead_adopted":
            sessions |= {e["data"].get("old_session"), e["data"].get("new_session")} - {None}
    end = closed or (events[-1]["time"] if events else created)
    row["_lead"] = (sessions, created, end)
    files = [p for s in sorted(sessions) for p in transcripts.get(s, [])]
    row["lead_transcripts"] = len(files) if files else ""
    if files:
        by_model = common.merge_models(common.collect(files, created, end))
        fill_tokens(row, "lead_", by_model, prices)
    c.close()
    return row


def mark_shared_leads(rows):
    """Name, per team, the other teams whose time windows overlap on the same lead session. The lead
    tokens of both teams then include the same messages, so a sum over teams counts them twice."""
    for r in rows:
        sessions, start, end = r.pop("_lead")
        r["_mine"] = (sessions, start, end)
    for r in rows:
        sessions, start, end = r["_mine"]
        r["lead_shared_with"] = ";".join(sorted(
            o["archive"] for o in rows if o is not r and sessions & o["_mine"][0] and start < o["_mine"][2] and o["_mine"][1] < end))
    for r in rows:
        del r["_mine"]


def main(argv=None):
    here = os.path.dirname(os.path.abspath(__file__))
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--archive", default=os.path.expanduser("~/.teamwire/archive"),
                    help="folder of <team>-<date>/team.db archives (read only)")
    ap.add_argument("--projects", default=os.path.expanduser("~/.claude/projects"),
                    help="Claude Code transcript root (read only)")
    ap.add_argument("--out", default=os.path.join(here, "results", "mine.csv"), help="CSV file to write")
    ap.add_argument("--prices", default=common.PRICES_PATH)
    a = ap.parse_args(argv)

    prices = common.load_prices(a.prices)
    transcripts = index_transcripts(a.projects)
    rows, skipped = [], 0
    for d in sorted(os.listdir(a.archive)):
        db = os.path.join(a.archive, d, "team.db")
        if not os.path.isfile(db):
            continue
        c = open_ro(db)
        name = c.execute("select name from team").fetchone()[0]
        c.close()
        if is_excluded(name):
            skipped += 1
            continue
        rows.append(team_row(db, d, prices, transcripts))
    mark_shared_leads(rows)
    os.makedirs(os.path.dirname(os.path.abspath(a.out)), exist_ok=True)
    with open(a.out, "w", newline="") as f:
        w = csv.DictWriter(f, COLUMNS)
        w.writeheader()
        w.writerows(rows)
    print("%d teams written to %s (%d excluded)" % (len(rows), a.out, skipped))
    print("with member tokens: %d, with lead tokens: %d, with a price error: %d" % (
        sum(1 for r in rows if r["member_input"] != ""), sum(1 for r in rows if r["lead_input"] != ""),
        sum(1 for r in rows if r["price_error"])))
    for r in rows:
        if r["price_error"]:
            print("  %s: %s" % (r["archive"], r["price_error"]), file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
