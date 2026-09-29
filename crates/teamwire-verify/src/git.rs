//! Every git call verify makes. Each one runs through `Platform::run`, and
//! each one only reads. `git diff` and `git status` refresh the index when a
//! file looks changed, which writes `.git/index`. Verify uses `diff-tree`,
//! which never reads the index, and `GIT_OPTIONAL_LOCKS=0` for `status`, so it
//! changes no worktree (VER-8).

use std::path::Path;
use std::time::Duration;

use teamwire_core::{Command, Platform, RunOutput};

use crate::VerifyError;

/// Git on a small repository answers in milliseconds; two minutes is the
/// bound Python used for a hung network filesystem.
const GIT_TIMEOUT: Duration = Duration::from_secs(120);

/// One repository, addressed by a directory inside it.
pub(crate) struct Git<'a> {
    platform: &'a dyn Platform,
    dir: &'a Path,
    timeout: Duration,
}

impl<'a> Git<'a> {
    pub(crate) fn new(platform: &'a dyn Platform, dir: &'a Path) -> Self {
        Self {
            platform,
            dir,
            timeout: GIT_TIMEOUT,
        }
    }

    /// The same, with a shorter bound on each call. The report check runs
    /// inside a Stop hook that has seconds, not minutes.
    pub(crate) fn with_timeout(
        platform: &'a dyn Platform,
        dir: &'a Path,
        timeout: Duration,
    ) -> Self {
        Self {
            platform,
            dir,
            timeout,
        }
    }

    /// Runs git and returns its output whatever the exit code was.
    pub(crate) fn run(&self, args: &[&str]) -> Result<RunOutput, VerifyError> {
        let cmd = Command::new("git")
            .args(["-c", "core.quotepath=off"])
            .args(args.iter().copied())
            .cwd(self.dir)
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_TERMINAL_PROMPT", "0");
        self.platform
            .run(&cmd, self.timeout)
            .map_err(|e| VerifyError::Git(format!("git {}: {e}", args.join(" "))))
    }

    /// Standard output of a git call that must succeed.
    pub(crate) fn stdout(&self, args: &[&str]) -> Result<String, VerifyError> {
        let out = self.run(args)?;
        if !out.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(VerifyError::Git(format!(
                "git {} exited {}: {}",
                args.join(" "),
                out.exit_code,
                stderr.trim()
            )));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// The full sha `HEAD` points at.
    pub(crate) fn head(&self) -> Result<String, VerifyError> {
        Ok(self.stdout(&["rev-parse", "HEAD"])?.trim().to_owned())
    }

    /// Where the member's own changes start (VER-2, INV-30).
    ///
    /// Normally the commit the member started from. A member that merged its
    /// base branch, as a contract amendment tells it to, brought the lead's
    /// commits into its history, and those are not its own. The start then
    /// moves forward to the merge base with the base branch, and only when
    /// that commit descends from the recorded start: it never moves back.
    pub(crate) fn diff_base(&self, base_sha: &str, base_ref: Option<&str>) -> String {
        let Some(base_ref) = base_ref.filter(|r| !r.is_empty()) else {
            return base_sha.to_owned();
        };
        let Ok(merge_base) = self.stdout(&["merge-base", "HEAD", base_ref]) else {
            return base_sha.to_owned();
        };
        let merge_base = merge_base.trim();
        if merge_base.is_empty() || merge_base == base_sha {
            return base_sha.to_owned();
        }
        let descends = self
            .run(&["merge-base", "--is-ancestor", base_sha, merge_base])
            .is_ok_and(|out| out.success());
        if descends {
            merge_base.to_owned()
        } else {
            base_sha.to_owned()
        }
    }

    /// Paths changed between `base` and `HEAD`, with git's one-letter status.
    pub(crate) fn changed_paths(&self, base: &str) -> Result<Vec<(char, String)>, VerifyError> {
        let out = self.stdout(&[
            "diff-tree",
            "-r",
            "--name-status",
            "--no-renames",
            "-z",
            base,
            "HEAD",
        ])?;
        let mut fields = out.split('\0').filter(|f| !f.is_empty());
        let mut paths = Vec::new();
        while let (Some(status), Some(path)) = (fields.next(), fields.next()) {
            paths.push((status.chars().next().unwrap_or('M'), path.to_owned()));
        }
        Ok(paths)
    }

    /// The whole patch between `base` and `HEAD`, with no context lines.
    pub(crate) fn patch(&self, base: &str) -> Result<String, VerifyError> {
        self.stdout(&[
            "diff-tree",
            "-r",
            "-p",
            "--unified=0",
            "--no-color",
            "--no-ext-diff",
            "--no-renames",
            "--src-prefix=a/",
            "--dst-prefix=b/",
            base,
            "HEAD",
        ])
    }

    /// A file as it was at `rev`, or `None` when it did not exist there.
    pub(crate) fn file_at(&self, rev: &str, path: &str) -> Option<String> {
        let spec = format!("{rev}:{path}");
        self.stdout(&["show", &spec]).ok()
    }

    /// Paths with uncommitted changes, `TASK.md` left out.
    pub(crate) fn uncommitted(&self) -> Result<Vec<String>, VerifyError> {
        let out = self.stdout(&["status", "--porcelain", "-z", "--untracked-files=all"])?;
        let mut fields = out.split('\0').filter(|f| !f.is_empty());
        let mut paths = Vec::new();
        while let Some(entry) = fields.next() {
            let (status, path) = entry.split_at_checked(3).unwrap_or(("", entry));
            // A rename or copy is followed by a second field, the old path.
            if status.starts_with(['R', 'C'])
                || status.get(1..2).is_some_and(|y| y == "R" || y == "C")
            {
                fields.next();
            }
            if path != crate::TASK_FILE {
                paths.push(path.to_owned());
            }
        }
        Ok(paths)
    }

    /// The member's own commits, oldest first: first-parent and non-merge,
    /// as `(full sha, author time in unix seconds)`.
    pub(crate) fn own_commits(&self, base: &str) -> Result<Vec<(String, i64)>, VerifyError> {
        let range = format!("{base}..HEAD");
        let out = self.stdout(&[
            "log",
            "--reverse",
            "--no-merges",
            "--first-parent",
            "--format=%H %at",
            &range,
        ])?;
        Ok(out
            .lines()
            .filter_map(|line| {
                let (sha, at) = line.split_once(' ')?;
                Some((sha.to_owned(), at.trim().parse().ok()?))
            })
            .collect())
    }
}
