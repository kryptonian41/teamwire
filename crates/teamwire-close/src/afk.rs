//! An AFK run closes only after its reporter wrote `afk/REPORT.md`, because
//! the person who started the run reads that file instead of the team.

use teamwire_store::{Ctx, TeamRow};

/// The report's headings. Each must be the whole text of some Markdown heading
/// line, compared without regard to case, commas, trailing spaces or a leading
/// heading number (ADR 0025 point 9), so `## 1. What you asked` and
/// `## Not done and why` count, and `## Not verified` is no `Verified`.
const HEADINGS: [&str; 8] = [
    "What you asked",
    "What I understood",
    "The result",
    "Verified",
    "Decisions made for you",
    "Not done, and why",
    "Risks",
    "Cost",
];

/// Why an AFK team's report is not ready, or `None` for an ordinary team and
/// for a report that has every heading.
pub(crate) fn report_refusal(ctx: &Ctx<'_>, team: &TeamRow) -> Option<String> {
    team.afk_budget_ends_at?;
    // A run that never had a member has nothing to report on. A removed member
    // still counts, and so does a member list that cannot be read.
    if ctx
        .store
        .read(|tx| tx.members()?.all_or_corrupt())
        .is_ok_and(|members| members.is_empty())
    {
        return None;
    }
    let path = ctx
        .home
        .join("teams")
        .join(team.name.as_str())
        .join("afk/REPORT.md");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Some(format!(
            "afk_report_incomplete: {} is missing; write it with a heading for each of: {}",
            path.display(),
            HEADINGS.join("; ")
        ));
    };
    let headings: Vec<String> = text
        .lines()
        .filter(|line| line.starts_with('#'))
        .filter_map(|line| line.trim_start_matches('#').strip_prefix(' '))
        .map(|h| comparable(without_number(h.trim_end())))
        .collect();
    let lacking: Vec<&str> = HEADINGS
        .into_iter()
        .filter(|name| {
            let name = comparable(name);
            !headings.iter().any(|h| *h == name)
        })
        .collect();
    (!lacking.is_empty()).then(|| {
        format!(
            "afk_report_incomplete: {} has no heading for: {}",
            path.display(),
            lacking.join("; ")
        )
    })
}

/// A heading's text as the check compares it: lowercase, with no commas.
fn comparable(text: &str) -> String {
    text.to_lowercase().replace(',', "")
}

/// `text` without a leading heading number such as `1. ` or `8) `.
fn without_number(text: &str) -> &str {
    let rest = text.trim_start_matches(|c: char| c.is_ascii_digit());
    if rest.len() == text.len() {
        return text;
    }
    rest.strip_prefix(['.', ')']).map_or(text, str::trim_start)
}
