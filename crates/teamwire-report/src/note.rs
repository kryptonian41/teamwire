//! The words a member is told when it is held or reminded. They state facts
//! (which file, which commits, what is missing) and never an instruction
//! from another agent (INV-18).

use std::path::Path;

use teamwire_core::ReportGaps;

/// The gaps as one sentence list, naming the report file.
pub(crate) fn gap_note(gaps: &ReportGaps, report: &Path) -> String {
    let mut facts = Vec::new();
    if gaps.report_missing {
        facts.push(format!(
            "the report file {} does not exist",
            report.display()
        ));
    }
    if !gaps.commits_without_line.is_empty() {
        facts.push(format!(
            "{} has no progress line for commit(s) {}",
            report.display(),
            gaps.commits_without_line.join(", ")
        ));
    }
    if gaps.decisions_missing && !gaps.report_missing {
        facts.push(format!(
            "{} has no 'Decisions I made' list (write 'Decisions I made: none' if there were none)",
            report.display()
        ));
    }
    if gaps.decisions_missing && gaps.report_missing {
        facts.push("the report needs a 'Decisions I made' list".to_owned());
    }
    format!(
        "Team report check: {}. Fix the report file; the result goes to the lead when you stop again.",
        facts.join("; ")
    )
}

/// The reminder of unread messages, with the command that reads them.
pub(crate) fn unread_note(ids: &[String]) -> String {
    format!(
        "You have unread teamwire message(s): {}. Read them with: teamwire inbox",
        ids.join(", ")
    )
}
