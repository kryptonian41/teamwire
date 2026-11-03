#!/usr/bin/env python3
"""Price every transcript a benchmark run produced (plan 5.5).

A transcript belongs to the run when any of its lines has a `cwd` equal to the clone or under it
(teamwire members work in <clone>/.worktrees/...).
When run.json sits beside the clone, a transcript that began before its `started_at` is left out. Subagent transcripts beside such a session count too.
Use --session for a member whose cwd is elsewhere. Each assistant message id is counted once across all files.
Reads ~/.claude/projects only; writes one JSON file under --out-dir.
"""
import argparse
import json
import os
import sys

import common


def started_before(path, since):
    """True when the transcript's first line is older than `since` (to the second): an earlier run at the same path."""
    first = common.transcript_first_ts(path)
    return bool(since and first and first[:19] < since[:19])


def find_transcripts(projects, clone, sessions=(), since=None):
    """Return the sorted transcript paths of a run: by cwd under `clone`, by session id, and their subagents.

    A transcript found by cwd whose first line is older than `since` belongs to an earlier run that used
    the same path, and is left out. A session named in `sessions` is always taken.
    """
    clone = os.path.realpath(clone)
    wanted = set(sessions)
    found = set()
    for proj in sorted(os.scandir(projects), key=lambda e: e.name):
        if not proj.is_dir():
            continue
        for e in os.scandir(proj.path):
            if e.is_file() and e.name.endswith(".jsonl"):
                cwd = common.transcript_cwd(e.path)
                inside = bool(cwd) and (os.path.realpath(cwd) == clone or os.path.realpath(cwd).startswith(clone + os.sep))
                if inside and not started_before(e.path, since) or e.name[:-6] in wanted:
                    found.add(e.path)
                    found.update(_subagents(os.path.join(proj.path, e.name[:-6])))
            elif e.is_dir() and e.name in wanted:
                found.update(_subagents(e.path))
    # A subagent file whose own lines run inside the clone counts even if its parent session is elsewhere.
    for proj in os.scandir(projects):
        if proj.is_dir():
            for e in os.scandir(proj.path):
                for p in _subagents(e.path) if e.is_dir() else []:
                    cwd = common.transcript_cwd(p)
                    if cwd and (os.path.realpath(cwd) == clone or os.path.realpath(cwd).startswith(clone + os.sep)) \
                            and not started_before(p, since):
                        found.add(p)
    return sorted(found)


def _subagents(session_dir):
    sub = os.path.join(session_dir, "subagents")
    if not os.path.isdir(sub):
        return []
    return [os.path.join(sub, n) for n in sorted(os.listdir(sub)) if n.endswith(".jsonl")]


def claude_total(path):
    """total_cost_usd from the JSON that `claude -p --output-format json` printed."""
    with open(path) as f:
        data = json.load(f)
    for item in reversed(data if isinstance(data, list) else [data]):
        if isinstance(item, dict) and "total_cost_usd" in item:
            return float(item["total_cost_usd"])
    raise ValueError("no total_cost_usd in %s" % path)


def price_run(paths, prices):
    per_file = common.collect(paths)
    files = []
    for p in paths:
        models = {m: {"tokens": t, "usd": common.usd(prices, m, t)} for m, t in per_file[p].items()}
        files.append({"path": p, "usd": sum(v["usd"] for v in models.values()), "models": models})
    by_model = common.merge_models(per_file)
    return {"files": files,
            "by_model": {m: {"tokens": t, "usd": common.usd(prices, m, t)} for m, t in sorted(by_model.items())},
            "total_usd": common.total_usd(prices, by_model)}


def main(argv=None):
    here = os.path.dirname(os.path.abspath(__file__))
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0], epilog=__doc__.split("\n", 1)[1],
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--clone", required=True, help="the run's clone path")
    ap.add_argument("--projects", default=os.path.expanduser("~/.claude/projects"), help="transcript root (read only)")
    ap.add_argument("--session", action="append", default=[], help="extra session id to include; repeat")
    ap.add_argument("--claude-json", help="file holding the JSON that `claude -p --output-format json` printed")
    ap.add_argument("--prices", default=common.PRICES_PATH)
    ap.add_argument("--out-dir", default=os.path.join(here, "results"))
    ap.add_argument("--name", help="name for the result file (default: the clone folder's name)")
    a = ap.parse_args(argv)

    prices = common.load_prices(a.prices)
    since = None
    run_json = os.path.join(os.path.dirname(os.path.realpath(a.clone)), "run.json")
    if os.path.exists(run_json):
        with open(run_json) as f:
            since = json.load(f).get("started_at")
    paths = find_transcripts(a.projects, a.clone, a.session, since)
    if not paths:
        print("error: no transcript has a cwd under %s" % os.path.realpath(a.clone), file=sys.stderr)
        return 1
    try:
        result = price_run(paths, prices)
    except common.PriceError as e:
        print("error: %s" % e, file=sys.stderr)
        return 1
    result.update(clone=os.path.realpath(a.clone), prices_read_on=prices["read_on"])
    if a.claude_json:
        result["claude_total_cost_usd"] = claude_total(a.claude_json)
        result["difference_usd"] = result["total_usd"] - result["claude_total_cost_usd"]

    os.makedirs(a.out_dir, exist_ok=True)
    out = os.path.join(a.out_dir, "cost-%s.json" % (a.name or os.path.basename(os.path.realpath(a.clone)) or "run"))
    with open(out, "w") as f:
        json.dump(result, f, indent=2)
        f.write("\n")

    print("%d transcript files, prices read on %s" % (len(paths), prices["read_on"]))
    for m, v in result["by_model"].items():
        print("  %-20s %s  $%.4f" % (m, " ".join("%s=%d" % (t, v["tokens"].get(t, 0)) for t in common.TYPES if v["tokens"].get(t)), v["usd"]))
    for f in result["files"]:
        print("  $%8.4f  %s" % (f["usd"], f["path"]))
    print("total: $%.4f" % result["total_usd"])
    if a.claude_json:
        print("claude -p total_cost_usd: $%.4f, difference: $%.4f" % (result["claude_total_cost_usd"], result["difference_usd"]))
    print("written to %s" % out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
