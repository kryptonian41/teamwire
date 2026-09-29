//! Shell snippets for the `run` steps of a member's `fake-claude` script.
//! A member's identity (`TEAMWIRE_TEAM`, `TEAMWIRE_ROLE`, `TEAMWIRE_HOME`) is
//! in its environment only because launch wrote it to `settings.json`.

/// Blocks until the test creates `$TEAMWIRE_HOME/<name>`, so that a scenario
/// controls when a member acts.
pub fn wait_for(name: &str) -> String {
    format!("while [ ! -f \"$TEAMWIRE_HOME/{name}\" ]; do sleep 0.1; done\n")
}

/// Commits one file in the member's worktree and sets `$sha` to the short
/// hash of the commit.
pub fn commit() -> &'static str {
    "echo \"$TEAMWIRE_ROLE\" >> \"$TEAMWIRE_ROLE.txt\"; git add -A\n\
     git -c user.name=t -c user.email=t@example.com commit -q -m \"work of $TEAMWIRE_ROLE\"\n\
     sha=$(git rev-parse --short=7 HEAD)\n"
}

/// Writes the member's report file. With `progress`, it holds a line for
/// `$sha`; without, only the decisions list.
pub fn report(progress: bool) -> String {
    let line = if progress {
        "- $sha work of $TEAMWIRE_ROLE | left: nothing\\n\\n"
    } else {
        ""
    };
    format!(
        "mkdir -p \"$TEAMWIRE_HOME/teams/$TEAMWIRE_TEAM/reports\"\n\
         printf '# report\\n\\n{line}Decisions I made: none\\n' \\\n\
         > \"$TEAMWIRE_HOME/teams/$TEAMWIRE_TEAM/reports/$TEAMWIRE_ROLE.md\"\n"
    )
}

/// Appends a progress line for `$sha` to the member's report file.
pub fn add_progress_line() -> &'static str {
    "sha=$(git rev-parse --short=7 HEAD)\n\
     printf -- '- %s work | left: nothing\\n' \"$sha\" \\\n\
     >> \"$TEAMWIRE_HOME/teams/$TEAMWIRE_TEAM/reports/$TEAMWIRE_ROLE.md\"\n"
}

/// `teamwire done` with a summary, printing its exit code.
pub fn done(summary: &str) -> String {
    format!("teamwire done --status done --summary \"{summary}\"; echo done-exit=$?\n")
}
