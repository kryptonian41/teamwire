//! The lead's tool hook (ADR 0035): a note when the session that leads an
//! open team does work its members should do. That work is an edit inside
//! the team's repository, a build or test run, a git command that commits,
//! merges or pushes, or a pull request. The note never blocks the call,
//! because the user may have asked the lead for exactly that.

use std::path::{Path, PathBuf};

use teamwire_core::{Member, TeamName, ToolAction, ToolCall};

use crate::lead_stop::led_by;

/// The note for `call`, or `None` when the call is no member's work or the
/// session leads no open team. A team that cannot be read is left out and
/// logged, so a damaged team never adds a note.
#[must_use]
#[expect(
    clippy::redundant_closure_for_method_calls,
    reason = "`ReadTx::members` as a path does not infer the transaction's lifetime for `Store::read`"
)]
pub fn lead_tool_note(home: &Path, call: &ToolCall) -> Option<String> {
    if call.subagent {
        return None;
    }
    // A read, or a command that builds and writes nothing, leaves here
    // before any store is opened: this hook runs on most of a lead's calls.
    let wanted = match &call.action {
        ToolAction::Edit(paths) => {
            let paths = edits_outside(home, paths, call.cwd.as_deref());
            if paths.is_empty() {
                return None;
            }
            Wanted::Edit(paths)
        }
        ToolAction::Shell(line) => Wanted::Command(member_command(line)?),
        ToolAction::Other => return None,
    };
    let mut teams = Vec::new();
    let mut what = None;
    for (name, read) in led_by(home, &call.session) {
        let team = match read {
            Ok(Some(team)) => team,
            Ok(None) => continue,
            Err(e) => {
                tracing::warn!(team = %name, error = %e, "a team is left out of the tool check");
                continue;
            }
        };
        let found = match &wanted {
            Wanted::Command(command) => Some(format!("this {command}")),
            Wanted::Edit(paths) => match team.store.read(|tx| tx.members()) {
                Ok(members) => {
                    let roots = roots(&members.items);
                    paths
                        .iter()
                        .find(|path| roots.iter().any(|root| path.starts_with(root)))
                        .map(|path| format!("this edit to {}", path.display()))
                }
                Err(e) => {
                    tracing::warn!(team = %name, error = %e, "a team is left out of the tool check");
                    None
                }
            },
        };
        if let Some(found) = found {
            what.get_or_insert(found);
            teams.push(name);
        }
    }
    Some(note(&teams, &what?))
}

/// What the call does that a member should do.
enum Wanted {
    Edit(Vec<PathBuf>),
    Command(&'static str),
}

/// The edited paths, made absolute against `cwd`, without those under
/// teamwire's own state directory, where briefs and specs belong. A relative
/// path with no `cwd` to read it against is dropped.
// shortcut: paths compare as written, so a repository reached through a
// symlink (/tmp against /private/tmp) is missed; resolve both sides if a
// lead's edits slip past that way.
fn edits_outside(home: &Path, paths: &[PathBuf], cwd: Option<&Path>) -> Vec<PathBuf> {
    paths
        .iter()
        .filter_map(|path| {
            if path.is_absolute() {
                Some(path.clone())
            } else {
                cwd.map(|cwd| cwd.join(path))
            }
        })
        .filter(|path| !path.starts_with(home))
        .collect()
}

/// The folders a team's members work in: each worktree member's repository
/// and every member's own directory.
fn roots(members: &[Member]) -> Vec<PathBuf> {
    members
        .iter()
        .flat_map(|m| m.repo.iter().cloned().chain(std::iter::once(m.cwd.clone())))
        .collect()
}

fn note(teams: &[TeamName], what: &str) -> String {
    let names: Vec<&str> = teams.iter().map(TeamName::as_str).collect();
    let first = names.first().copied().unwrap_or_default();
    let (word, whose) = if names.len() == 1 {
        ("team", "its")
    } else {
        ("teams", "their")
    };
    format!(
        "[teamwire:{first}] You lead {word} {}, so leave {what} to {whose} members: reprompt the member that owns the work, or brief a new one. If the user asked you to do it yourself, carry on.",
        names.join(", ")
    )
}

/// What `line` does that is a member's work, named for the note: the first
/// simple command in it that builds, tests, writes to git or opens a pull
/// request.
// shortcut: a fixed list of tools and no parsing of quotes; add a tool when
// a lead's transcript shows a build or test command this list misses.
fn member_command(line: &str) -> Option<&'static str> {
    line.split([';', '&', '|', '\n', '(', ')', '`', '{', '}'])
        .find_map(|part| simple_command(&part.split_whitespace().collect::<Vec<_>>()))
}

fn simple_command(words: &[&str]) -> Option<&'static str> {
    let words = unwrapped(words);
    let (tool, rest) = words.split_first()?;
    let tool = tool.rsplit('/').next().unwrap_or(tool);
    let sub = rest.first().copied().map(unquoted).unwrap_or_default();
    let build = Some("build or test run");
    match tool {
        "git" => git_write(rest),
        "gh" => (sub == "pr"
            && matches!(rest.get(1).copied().map(unquoted), Some("create" | "merge")))
        .then_some("pull request"),
        "cargo" => matches!(
            sub,
            "build" | "test" | "nextest" | "clippy" | "check" | "run" | "bench" | "fmt" | "install"
        )
        .then_some("build or test run"),
        "npm" | "pnpm" | "yarn" | "bun" => (rest.is_empty()
            || matches!(
                sub,
                "install" | "i" | "ci" | "test" | "t" | "run" | "build" | "exec" | "x"
            ))
        .then_some("build or test run"),
        "go" | "deno" | "swift" | "dotnet" => matches!(
            sub,
            "build" | "test" | "vet" | "run" | "generate" | "task" | "compile"
        )
        .then_some("build or test run"),
        "python" | "python3" => (sub == "-m"
            && matches!(
                rest.get(1).copied().map(unquoted),
                Some("pytest" | "unittest")
            ))
        .then_some("build or test run"),
        "npx" | "bunx" | "pytest" | "tox" | "nox" | "make" | "gmake" | "mvn" | "gradle"
        | "gradlew" | "tsc" | "vitest" | "jest" | "rspec" | "xcodebuild" => build,
        script
            if Path::new(script)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("sh"))
                && ["test", "gate", "build", "check"]
                    .iter()
                    .any(|word| script.contains(word)) =>
        {
            build
        }
        _ => None,
    }
}

/// The words after any variable settings and any wrapper that runs the rest
/// as its own command, such as `env`, `timeout 60` or `bash -c`.
fn unwrapped<'a>(words: &'a [&'a str]) -> Vec<&'a str> {
    let mut rest = words;
    loop {
        let Some((first, after)) = rest.split_first() else {
            return Vec::new();
        };
        let first = unquoted(first);
        rest = match first {
            _ if is_assignment(first) => after,
            "env" | "time" | "nohup" | "sudo" | "command" | "exec" | "nice" | "caffeinate" => after,
            "timeout" | "gtimeout" => after.get(1..).unwrap_or_default(),
            "rtk" if after.first() == Some(&"proxy") => after.get(1..).unwrap_or_default(),
            "bash" | "sh" | "zsh" if after.first().is_some_and(|w| matches!(*w, "-c" | "-lc")) => {
                after.get(1..).unwrap_or_default()
            }
            _ => return rest.iter().map(|w| unquoted(w)).collect(),
        };
    }
}

fn unquoted(word: &str) -> &str {
    word.trim_matches(['\'', '"'])
}

/// `NAME=value`, as a shell reads a variable setting before a command.
fn is_assignment(word: &str) -> bool {
    word.split_once('=').is_some_and(|(name, _)| {
        name.chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

/// The git subcommand after git's own options, when it writes history or a
/// remote, or makes a worktree.
fn git_write(rest: &[&str]) -> Option<&'static str> {
    let mut words = rest.iter().copied();
    while let Some(word) = words.next() {
        match word {
            // These two options take the next word as their value.
            "-C" | "-c" => {
                words.next();
            }
            option if option.starts_with('-') => {}
            "commit" => return Some("git commit"),
            "merge" => return Some("git merge"),
            "rebase" => return Some("git rebase"),
            "cherry-pick" => return Some("git cherry-pick"),
            "revert" => return Some("git revert"),
            "am" | "apply" => return Some("git patch"),
            "pull" => return Some("git pull"),
            "push" => return Some("git push"),
            "worktree" => return (words.next() == Some("add")).then_some("git worktree add"),
            _ => return None,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_tests_and_git_writes_are_member_work() {
        for (line, what) in [
            (
                "cargo test -p teamwire-report --locked",
                "build or test run",
            ),
            ("cd /r && scripts/gate.sh fast", "build or test run"),
            (
                "RUST_LOG=debug gtimeout 600 cargo nextest run",
                "build or test run",
            ),
            ("rtk proxy bun test", "build or test run"),
            ("bash -lc 'pnpm run build'", "build or test run"),
            ("python3 -m pytest tests/", "build or test run"),
            ("git -C /r/.worktrees/api commit -m x", "git commit"),
            ("git merge --no-edit alpha/api", "git merge"),
            ("git push origin develop", "git push"),
            ("git worktree add /tmp/fresh develop", "git worktree add"),
            ("gh pr create --fill", "pull request"),
        ] {
            assert_eq!(member_command(line), Some(what), "{line}");
        }
    }

    #[test]
    fn reads_and_teamwires_own_commands_are_not() {
        for line in [
            "teamwire verify alpha --role api",
            "teamwire launch --add --spec more.json",
            "git log --oneline -5",
            "git diff --stat develop..alpha/api",
            "git worktree list",
            "grep -rn cargo crates",
            "cat reports/api.md",
            "gh pr view 12",
            "npm view left-pad",
        ] {
            assert_eq!(member_command(line), None, "{line}");
        }
    }

    #[test]
    fn edits_under_the_state_directory_or_with_no_place_are_dropped() {
        let home = Path::new("/h/.teamwire");
        let paths = [
            PathBuf::from("/h/.teamwire/teams/alpha/briefs/api.md"),
            PathBuf::from("/r/src/lib.rs"),
            PathBuf::from("src/main.rs"),
        ];
        assert_eq!(
            edits_outside(home, &paths, Some(Path::new("/r"))),
            [
                PathBuf::from("/r/src/lib.rs"),
                PathBuf::from("/r/src/main.rs")
            ]
        );
        assert_eq!(
            edits_outside(home, &paths, None),
            [PathBuf::from("/r/src/lib.rs")]
        );
    }
}
