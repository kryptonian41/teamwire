#!/usr/bin/env python3
"""Scan a run's transcripts for tool calls that name a path the run must not read (plan 5.2, detection).

Prevention is not possible: an agent can read the source checkout, the teamwire archive, the transcripts
and other runs' folders. This lists every tool call whose input names one of those paths, so that a
run which looked can be reported and thrown out. Exit 1 when any hit is found. Reads transcripts only.
"""
import argparse
import json
import os
import re
import sys

import common
import cost


def leak_patterns(roots, home):
    """One regex per root, matching its absolute form and its ~ and $HOME forms, up to the end of the path."""
    pats = []
    for root in roots:
        root = os.path.realpath(root) if os.path.exists(root) else os.path.normpath(root)
        forms = {root}
        if root.startswith(home + os.sep):
            rel = root[len(home):]
            forms |= {"~" + rel, "$HOME" + rel, "${HOME}" + rel}
        alt = "|".join(re.escape(f) for f in sorted(forms, key=len, reverse=True))
        pats.append((root, re.compile(r"(?:%s)(?![\w.-])[^\s\"'\;|&<>)]*" % alt)))
    return pats


def tool_calls(path):
    """Yield (line number, tool name, input as JSON text) for every tool_use block of a transcript."""
    with open(path, errors="replace") as f:
        for n, line in enumerate(f, 1):
            if '"tool_use"' not in line:
                continue
            try:
                o = json.loads(line)
            except ValueError:
                continue
            content = (o.get("message") or {}).get("content")
            for block in content if isinstance(content, list) else []:
                if isinstance(block, dict) and block.get("type") == "tool_use":
                    yield n, block.get("name"), json.dumps(block.get("input"), ensure_ascii=False)


def scan(paths, roots, allow=(), home=None, skip_inside=()):
    """Hits as (file, line, tool, matched path). A match under an `allow` or `skip_inside` path is no hit."""
    home = os.path.realpath(home or os.path.expanduser("~"))
    pats = leak_patterns(roots, home)
    ok = [os.path.realpath(p) for p in list(allow) + list(skip_inside)]
    hits = []
    for path in paths:
        for n, tool, text in tool_calls(path):
            for _root, pat in pats:
                for m in pat.finditer(text):
                    hit = m.group(0)
                    expanded = os.path.expanduser(hit.replace("$HOME", "~").replace("${HOME}", "~"))
                    if any(expanded == p or expanded.startswith(p + os.sep) for p in ok):
                        continue
                    hits.append((path, n, tool, hit))
    return hits


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0], epilog=__doc__.split("\n", 1)[1],
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--clone", required=True, help="the run's clone path")
    ap.add_argument("--source", action="append", required=True, help="the source checkout (a path the run must not read); repeat")
    ap.add_argument("--runs-root", help="folder that holds all runs; other runs' folders are off limits, this run's own is not")
    ap.add_argument("--allow", action="append", default=[], help="a path that is expected (such as the run's TEAMWIRE_HOME); repeat")
    ap.add_argument("--projects", default=os.path.expanduser("~/.claude/projects"), help="transcript root (read only)")
    ap.add_argument("--session", action="append", default=[], help="extra session id to scan; repeat")
    a = ap.parse_args(argv)

    clone = os.path.realpath(a.clone)
    roots = list(a.source) + [os.path.expanduser("~/.teamwire/archive"), os.path.expanduser("~/.claude/projects")]
    if a.runs_root:
        roots.append(a.runs_root)
    paths = cost.find_transcripts(a.projects, clone, a.session)
    if not paths:
        print("error: no transcript has a cwd under %s" % clone, file=sys.stderr)
        return 1
    # This run's own folder (the clone's parent) sits under the runs root and is not a leak.
    hits = scan(paths, roots, a.allow, skip_inside=[os.path.dirname(clone)])
    print("%d transcript files scanned, %d hits" % (len(paths), len(hits)))
    for path, n, tool, hit in hits:
        print("LEAK %s:%d %s %s" % (path, n, tool, hit))
    return 1 if hits else 0


if __name__ == "__main__":
    sys.exit(main())
