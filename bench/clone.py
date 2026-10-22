#!/usr/bin/env python3
"""Make a leak-free clone of the repository at a task's parent commit, plus the run's settings (plan 5.2).

The source repository is only read: the bundle is built from a ref that exists in a temporary
bare copy. The clone holds the parent and its ancestors, no `origin`, and no path of the source.
Settings and run.json are written next to the clone, never inside it.
"""
import argparse
import datetime
import json
import os
import shutil
import sys
import tempfile

import common


class LeakError(Exception):
    pass


def verify_clone(clone, parent):
    """Fail loudly unless the clone holds nothing past `parent` and has no remote."""
    allowed = set(common.git(clone, "rev-list", parent).split())
    refs = common.git(clone, "for-each-ref", "--format=%(refname) %(objectname)").splitlines()
    for line in refs:
        name, sha = line.split(" ")
        if sha not in allowed and common.git(clone, "rev-parse", sha + "^{commit}", check=False).strip() not in allowed:
            raise LeakError("ref %s points past the parent: %s" % (name, sha))
    reachable = set(common.git(clone, "rev-list", "--all").split())
    if not reachable <= allowed:
        raise LeakError("commits outside the parent's history are reachable: %s" % sorted(reachable - allowed)[:3])
    # Unreachable commits would still be readable with `git show <sha>`, so check every commit object too.
    objects = common.git(clone, "cat-file", "--batch-all-objects", "--batch-check=%(objecttype) %(objectname)")
    extra = [l.split()[1] for l in objects.splitlines() if l.startswith("commit ") and l.split()[1] not in allowed]
    if extra:
        raise LeakError("commit objects outside the parent's history are stored: %s" % extra[:3])
    if common.git(clone, "remote").strip():
        raise LeakError("the clone still has a remote: %s" % common.git(clone, "remote").split())
    if common.git(clone, "rev-parse", "HEAD").strip() != parent:
        raise LeakError("HEAD is not the parent commit")


def verify_no_paths(clone, paths):
    """No text file in .git (config, logs, packed-refs ...) may name the given paths."""
    for root, dirs, files in os.walk(os.path.join(clone, ".git")):
        if os.path.basename(root) == "objects":
            dirs[:] = []
            continue
        for n in files:
            with open(os.path.join(root, n), "rb") as f:
                data = f.read()
            for p in paths:
                if p.encode() in data:
                    raise LeakError("%s names %s" % (os.path.join(root, n), p))


def make_clone(repo, parent, dest):
    """Clone `repo` at `parent` into `dest` (must not exist). Returns the full parent sha."""
    repo = os.path.realpath(repo)
    parent = common.git(repo, "rev-parse", "--verify", parent + "^{commit}").strip()
    if os.path.exists(dest):
        raise LeakError("destination already exists: %s" % dest)
    tmp = tempfile.mkdtemp(prefix="bench-bundle-")
    try:
        bare = os.path.join(tmp, "copy.git")
        # A bare copy of the source: refs are created and deleted here, never in the source.
        common.git(tmp, "clone", "--bare", "--quiet", repo, bare)
        for line in common.git(bare, "for-each-ref", "--format=%(refname)").split():
            common.git(bare, "update-ref", "-d", line)
        common.git(bare, "update-ref", "refs/heads/bench", parent)
        bundle = os.path.join(tmp, "task.bundle")
        common.git(bare, "bundle", "create", "--quiet", bundle, "refs/heads/bench")
        common.git(tmp, "clone", "--quiet", "--branch", "bench", bundle, dest)
        common.git(dest, "remote", "remove", "origin")
        common.git(dest, "reflog", "expire", "--expire=now", "--all")
        # The bundle's own refs left behind by the clone (packed-refs, FETCH_HEAD) must be gone too.
        verify_clone(dest, parent)
        verify_no_paths(dest, [repo, tmp])
    except BaseException:
        shutil.rmtree(dest, ignore_errors=True)
        raise
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    return parent


def write_run_files(base, clone, task, setup, parent):
    settings = os.path.join(base, "settings.json")
    with open(settings, "w") as f:
        json.dump({"enabledPlugins": {"claude-mem@thedotmack": False}}, f, indent=2)
        f.write("\n")
    run = os.path.join(base, "run.json")
    with open(run, "w") as f:
        json.dump({"task": task, "setup": setup, "parent": parent, "clone": clone,
                   "started_at": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")},
                  f, indent=2)
        f.write("\n")
    return settings, run


def main(argv=None):
    here = os.path.dirname(os.path.abspath(__file__))
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--repo", default=os.path.dirname(here), help="source repository (read only)")
    ap.add_argument("--parent", required=True, help="the task commit's parent: the newest commit the run may see")
    ap.add_argument("--task", required=True, help="task id, for run.json")
    ap.add_argument("--setup", required=True, help="setup name, for run.json")
    ap.add_argument("--dir", help="folder to create for the run (default: a new folder under the temp directory)")
    a = ap.parse_args(argv)

    base = a.dir or tempfile.mkdtemp(prefix="bench-%s-%s-" % (a.task, a.setup))
    os.makedirs(base, exist_ok=True)
    clone = os.path.join(os.path.realpath(base), "clone")
    try:
        parent = make_clone(a.repo, a.parent, clone)
    except (LeakError, RuntimeError) as e:
        print("clone failed: %s" % e, file=sys.stderr)
        return 1
    settings, run = write_run_files(os.path.realpath(base), clone, a.task, a.setup, parent)
    print("clone:    %s (branch bench at %s)\nsettings: %s\nrun.json: %s" % (clone, parent[:12], settings, run))
    return 0


if __name__ == "__main__":
    sys.exit(main())
