//! Shell snippets for the `run` steps of a member's `fake-claude` script.
//! A member's identity (`TEAMWIRE_TEAM`, `TEAMWIRE_ROLE`, `TEAMWIRE_HOME`) is
//! in its environment only because launch wrote it to `settings.json`.

/// How many 0.1 s waits `wait_for` makes before it gives up: 120 s. The
/// tests' own waits for a member are 20 to 60 s, so a step that waits longer
/// belongs to a test that has already failed.
const WAIT_TICKS: u32 = 1200;

/// Blocks until the test creates `$TEAMWIRE_HOME/<name>`, so that a scenario
/// controls when a member acts. Exits 1 after 120 s, so a step whose test
/// ended early does not wait for ever.
pub fn wait_for(name: &str) -> String {
    wait_for_ticks(name, WAIT_TICKS)
}

fn wait_for_ticks(name: &str, ticks: u32) -> String {
    format!(
        "n=0; while [ ! -f \"$TEAMWIRE_HOME/{name}\" ]; do \
         n=$((n+1)); [ $n -gt {ticks} ] && exit 1; sleep 0.1; done\n"
    )
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
    let (line, args) = if progress {
        (
            "- %s work of %s | left: nothing\\n\\n",
            " \"$sha\" \"$TEAMWIRE_ROLE\"",
        )
    } else {
        ("", "")
    };
    format!(
        "mkdir -p \"$TEAMWIRE_HOME/teams/$TEAMWIRE_TEAM/reports\"\n\
         printf '# report\\n\\n{line}Decisions I made: none\\n'{args} \\\n\
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

#[cfg(test)]
mod tests {
    use super::*;

    fn run(script: &str, home: &std::path::Path) -> Option<i32> {
        std::process::Command::new("/bin/sh")
            .args(["-c", script])
            .env("TEAMWIRE_HOME", home)
            .status()
            .unwrap()
            .code()
    }

    #[test]
    fn wait_for_returns_when_the_file_exists_and_gives_up_when_it_never_does() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(home.path().join("go"), "").unwrap();
        assert_eq!(run(&wait_for_ticks("go", 3), home.path()), Some(0));
        assert_eq!(run(&wait_for_ticks("never", 3), home.path()), Some(1));
        assert!(wait_for("go").contains("1200"));
    }
}
