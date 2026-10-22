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


def task_parent(repo, commits):
    """The parent of the first task commit. Refuses when any task commit is already in that parent's
    history: `git show <commit>` in the clone would print the answer."""
    commits = [common.git(repo, "rev-parse", "--verify", c + "^{commit}").strip() for c in commits]
    parent = common.git(repo, "rev-parse", "--verify", commits[0] + "^").strip()
    for c in commits:
        try:
            common.git(repo, "merge-base", "--is-ancestor", c, parent)
        except RuntimeError:
            continue
        raise LeakError("task commit %s is in the history of the parent %s: the clone would hold the answer" % (c[:12], parent[:12]))
    return commits, parent


def write_run_files(base, clone, task, setup, parent, commits):
    settings = os.path.join(base, "settings.json")
    with open(settings, "w") as f:
        json.dump({"enabledPlugins": {"claude-mem@thedotmack": False}}, f, indent=2)
        f.write("\n")
    run = os.path.join(base, "run.json")
    with open(run, "w") as f:
        json.dump({"task": task, "setup": setup, "parent": parent, "commits": commits, "clone": clone,
                   "started_at": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")},
                  f, indent=2)
        f.write("\n")
    return settings, run


def main(argv=None):
    here = os.path.dirname(os.path.abspath(__file__))
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--repo", default=os.path.dirname(here), help="source repository (read only)")
    ap.add_argument("--commit", action="append", required=True,
                    help="a task commit; repeat for a task of several, oldest first. The clone ends at the first one's parent")
    ap.add_argument("--task", required=True, help="task id, for run.json")
    ap.add_argument("--setup", required=True, help="setup name, for run.json")
    ap.add_argument("--root", required=True,
                    help="folder that holds all runs; each run gets a new random sub-folder named bn<random>")
    a = ap.parse_args(argv)

    try:
        commits, parent = task_parent(a.repo, a.commit)
        os.makedirs(a.root, exist_ok=True)
        # A random name: an agent that lists the runs folder learns nothing about the task or setup.
        base = tempfile.mkdtemp(prefix="bn", dir=a.root)
        clone = os.path.join(os.path.realpath(base), "clone")
        make_clone(a.repo, parent, clone)
    except (LeakError, RuntimeError) as e:
        if "base" in locals():
            shutil.rmtree(base, ignore_errors=True)
        print("clone failed: %s" % e, file=sys.stderr)
        return 1
    settings, run = write_run_files(os.path.realpath(base), clone, a.task, a.setup, parent, commits)
    print("clone:    %s (branch bench at %s)\nsettings: %s\nrun.json: %s" % (clone, parent[:12], settings, run))
    return 0


if __name__ == "__main__":
    sys.exit(main())
