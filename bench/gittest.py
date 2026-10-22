"""Helpers that build tiny throwaway git repositories for tests."""
import os
import subprocess

ENV = dict(os.environ, GIT_AUTHOR_NAME="t", GIT_AUTHOR_EMAIL="t@t", GIT_COMMITTER_NAME="t",
           GIT_COMMITTER_EMAIL="t@t", GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_SYSTEM=os.devnull)


def git(repo, *args):
    r = subprocess.run(["git", "-C", repo] + list(args), stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                       universal_newlines=True, env=ENV)
    if r.returncode:
        raise RuntimeError("git %s: %s" % (" ".join(args), r.stderr))
    return r.stdout.strip()


def init(path):
    os.makedirs(path)
    git(path, "init", "-q")
    git(path, "checkout", "-q", "-b", "develop")
    git(path, "config", "commit.gpgsign", "false")
    return path


def commit(repo, files, msg="c", remove=()):
    """Write {path: text}, delete `remove`, commit everything, return the full sha."""
    for p, text in files.items():
        full = os.path.join(repo, p)
        os.makedirs(os.path.dirname(full), exist_ok=True)
        with open(full, "w") as f:
            f.write(text)
    for p in remove:
        os.remove(os.path.join(repo, p))
    git(repo, "add", "-A")
    git(repo, "commit", "-q", "-m", msg)
    return git(repo, "rev-parse", "HEAD")


def lines(n, tag="x"):
    return "".join("%s %d\n" % (tag, i) for i in range(n))
