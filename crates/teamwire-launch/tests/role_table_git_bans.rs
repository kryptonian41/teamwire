//! The lead skill's role table lists, for each kind, every git command that
//! kind's seat rules ban, and no other (F15). A lead that reads the table
//! must not write a brief the member's own rules forbid.
#![expect(
    clippy::unwrap_used,
    reason = "an integration-test file unwraps; a failure there fails the test"
)]

use std::collections::BTreeSet;
use std::path::Path;

use teamwire_core::{MemberKind, Role, TeamName};
use teamwire_launch::{ContractParts, render_contract};

/// The words that open a ban in the seat rules. `Do not pass` is left out on
/// purpose: the closer's "Do not pass `-x` to `git clean`" bans a flag of a
/// command the closer must run, not the command.
const BAN_WORDS: [&str; 4] = ["Never run", "Never use", "Do not use", "The same goes for"];

/// The backticked spans of `text` that start with `git `.
fn git_spans(text: &str) -> BTreeSet<String> {
    text.split('`')
        .skip(1)
        .step_by(2)
        .filter(|span| span.starts_with("git "))
        .map(str::to_owned)
        .collect()
}

/// The git commands a kind's seat rules ban: each backticked `git ` span in
/// a sentence that holds one of `BAN_WORDS`.
fn seat_bans(kind: MemberKind) -> BTreeSet<String> {
    let text = render_contract(&ContractParts {
        team: &TeamName::parse("demo").unwrap(),
        role: &Role::parse("api").unwrap(),
        kind,
        report_file: Path::new("/h/teams/demo/reports/api.md"),
        outbox_dir: Path::new("/h/teams/demo/outbox"),
        teamwire_bin: Path::new("/opt/x/teamwire"),
        check: None,
        owns: &[],
    });
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    flat.split(". ")
        .flat_map(|s| s.split(": "))
        .filter(|sentence| BAN_WORDS.iter().any(|w| sentence.contains(w)))
        .flat_map(git_spans)
        .collect()
}

/// The role table's row for `kind`: the rows after the header line
/// ``| Kind | Does | May not | Set `mode` |`` and its separator, up to the
/// first line that does not start with `|`. No other table is read.
fn role_row<'a>(skill: &'a str, kind: &str) -> Option<&'a str> {
    let mut lines = skill.lines();
    lines.find(|l| l.starts_with("| Kind | Does | May not |"))?;
    lines.next()?;
    let start = format!("| `{kind}` |");
    lines
        .take_while(|l| l.starts_with('|'))
        .find(|l| l.starts_with(&start))
}

/// The "May not" cell of a role table row.
fn may_not(row: &str) -> &str {
    row.split(" | ").nth(2).unwrap_or_default()
}

/// What differs between the bans and a row's "May not" cell, naming the
/// kind and each command, or `None` when they match.
fn compare(kind: &str, bans: &BTreeSet<String>, row: &str) -> Option<String> {
    let listed = git_spans(may_not(row));
    let missing: Vec<&String> = bans.difference(&listed).collect();
    let extra: Vec<&String> = listed.difference(bans).collect();
    (!missing.is_empty() || !extra.is_empty()).then(|| {
        format!(
            "{kind}: the role table misses {missing:?} that its seat rules ban, and lists \
             {extra:?} that they do not"
        )
    })
}

fn skill() -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/skills/teamwire/SKILL.md"),
    )
    .unwrap()
}

#[test]
fn the_role_table_lists_every_git_ban_of_each_seat() {
    let skill = skill();
    let problems: Vec<String> = MemberKind::ALL
        .iter()
        .filter_map(|kind| {
            let name = kind.as_str();
            let Some(row) = role_row(&skill, name) else {
                return Some(format!("{name}: no row in the role table"));
            };
            compare(name, &seat_bans(*kind), row)
        })
        .collect();
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn a_row_that_names_a_git_command_no_seat_bans_fails() {
    let bans = seat_bans(MemberKind::Implementer);
    let mut cell: Vec<String> = bans.iter().map(|b| format!("`{b}`")).collect();
    cell.push("`git push`".to_owned());
    let row = format!(
        "| `implementer` | Builds. | Run {}. | `worktree` |",
        cell.join(", ")
    );
    let problem = compare("implementer", &bans, &row).unwrap();
    assert!(problem.starts_with("implementer:"), "{problem}");
    assert!(problem.contains("\"git push\""), "{problem}");
}

#[test]
fn a_closer_row_in_another_table_is_not_read() {
    let text = "| Shape | Members | Use |\n| --- | --- | --- |\n\
        | `closer` | one closer | `git worktree add` |\n\n\
        | Kind | Does | May not | Set `mode` |\n| --- | --- | --- | --- |\n\
        | `closer` | Checks. | Run `git worktree prune`. | `pane` |\n";
    let row = role_row(text, "closer").unwrap();
    assert_eq!(
        row,
        "| `closer` | Checks. | Run `git worktree prune`. | `pane` |"
    );
}
