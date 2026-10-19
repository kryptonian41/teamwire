//! The AFK part of the status view: the budget and the decisions logged.

use std::path::Path;

use teamwire_core::Timestamp;

use crate::AfkView;

pub(crate) fn view(team_dir: &Path, ends: Timestamp, now: Timestamp) -> AfkView {
    AfkView {
        budget_ends_at: ends,
        seconds_left: ends.saturating_since(now).as_secs(),
        spent: now >= ends,
        decisions: decisions(team_dir),
    }
}

/// The entries of `afk/decisions.md`: lines that start with `D`, a number and
/// whitespace. A file that cannot be read counts as none, because status
/// must still describe the team.
fn decisions(team_dir: &Path) -> u32 {
    let text = std::fs::read_to_string(team_dir.join("afk/decisions.md")).unwrap_or_default();
    let entries = text.lines().filter(|line| {
        line.strip_prefix('D').is_some_and(|rest| {
            let after = rest.trim_start_matches(|c: char| c.is_ascii_digit());
            after.len() < rest.len() && after.starts_with(char::is_whitespace)
        })
    });
    u32::try_from(entries.count()).unwrap_or(u32::MAX)
}

#[cfg(test)]
#[expect(
    clippy::disallowed_methods,
    reason = "the test stands in for the reporter who writes the file, outside the platform"
)]
mod tests {
    use super::*;

    #[test]
    fn only_lines_that_start_with_d_number_and_space_count() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("afk")).unwrap();
        std::fs::write(
            dir.path().join("afk/decisions.md"),
            "# Decisions\nD1 roster\nD2\tmodel\nD10 more\n D3 indented\nD4\nDx 5 no\nD no number\n- D6 listed\n",
        )
        .unwrap();
        assert_eq!(decisions(dir.path()), 3);
    }

    #[test]
    fn a_missing_file_is_no_decisions() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(decisions(dir.path()), 0);
    }
}
