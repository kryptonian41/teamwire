//! An AFK team closes only with a complete `afk/REPORT.md`; an ordinary team
//! and a single-member removal never look at it.
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "helper functions in an integration-test file unwrap and index; a failure there fails the test"
)]

#[path = "support/rig.rs"]
pub mod rig;

use rig::Rig;
use teamwire_close::{CloseOpts, CloseTarget, close, plan_close};
use teamwire_core::{EventType, MemberKind, Role, TeamState};

const FULL: &str = "# Report\n\n## 1. What you asked\n## What I understood\n## THE RESULT\n\
## Verified\n## Decisions made for you\n## Not done, and why\n## Risks\n## Cost\n";

/// An AFK team with one member, so it has a run to report on.
fn afk_rig() -> Rig {
    let mut rig = empty_afk_rig();
    rig.add_pane_member("scout", MemberKind::Investigator);
    rig
}

fn empty_afk_rig() -> Rig {
    let mut rig = Rig::without_repo();
    rig.store
        .write(|tx| {
            tx.connection()
                .execute(
                    "UPDATE team SET afk_budget_ends_at = '2999-01-01T00:00:00.000Z'",
                    [],
                )
                .map(|_| ())
                .map_err(Into::into)
        })
        .unwrap();
    rig
}

fn refusals(rig: &mut Rig) -> Vec<String> {
    plan_close(&rig.ctx(), CloseTarget::Team).refusals
}

#[test]
fn an_afk_team_with_no_report_is_refused_and_the_dry_run_says_so() {
    let mut rig = afk_rig();
    let why = refusals(&mut rig);
    assert_eq!(why.len(), 1, "{why:?}");
    assert!(why[0].starts_with("afk_report_incomplete: "), "{why:?}");
    assert!(why[0].contains("REPORT.md is missing"), "{why:?}");

    let report = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert_eq!(report.refusals, why);
    assert_eq!(rig.team().state, TeamState::Open);
    assert!(rig.team_dir().is_dir(), "nothing was archived or deleted");
    assert_eq!(rig.events(EventType::CloseRefused).len(), 1);
}

#[test]
fn a_report_missing_any_one_heading_is_refused_and_names_it() {
    for (i, name) in [
        "What you asked",
        "What I understood",
        "The result",
        "Verified",
        "Decisions made for you",
        "Not done, and why",
        "Risks",
        "Cost",
    ]
    .iter()
    .enumerate()
    {
        let mut rig = afk_rig();
        let text = FULL
            .lines()
            .filter(|l| !l.to_lowercase().contains(&name.to_lowercase()))
            .collect::<Vec<_>>()
            .join("\n");
        // The text mentions the name outside a heading: that does not count.
        rig.team_file("afk/REPORT.md", &format!("{text}\nWe talk about {name}.\n"));
        let why = refusals(&mut rig);
        assert_eq!(why.len(), 1, "{i} {name}: {why:?}");
        assert!(why[0].ends_with(&format!("for: {name}")), "{name}: {why:?}");
    }
}

#[test]
fn a_report_with_all_eight_headings_lets_an_afk_team_close() {
    let mut rig = afk_rig();
    rig.team_file("afk/REPORT.md", FULL);
    assert!(refusals(&mut rig).is_empty());
    let report = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert!(
        report.refusals.is_empty() && report.stopped_at.is_none(),
        "{report:?}"
    );
    assert!(!rig.team_dir().exists());
}

#[test]
fn an_ordinary_team_closes_with_no_report() {
    let mut rig = Rig::without_repo();
    assert!(refusals(&mut rig).is_empty());
}

#[test]
fn removing_one_member_of_an_afk_team_needs_no_report() {
    let mut rig = afk_rig();
    let role = Role::parse("scout").unwrap();
    let plan = plan_close(&rig.ctx(), CloseTarget::Member(role));
    assert!(plan.refusals.is_empty(), "{:?}", plan.refusals);
}

#[test]
fn an_afk_team_that_never_had_a_member_closes_with_no_report() {
    let mut rig = empty_afk_rig();
    assert!(refusals(&mut rig).is_empty());
    let report = close(&mut rig.ctx(), CloseTarget::Team, CloseOpts::default());
    assert!(
        report.refusals.is_empty() && report.stopped_at.is_none(),
        "{report:?}"
    );
}

#[test]
fn a_removed_member_still_needs_the_report() {
    let mut rig = afk_rig();
    rig.store
        .write(|tx| {
            tx.connection()
                .execute("UPDATE members SET state = 'removed'", [])
                .map(|_| ())
                .map_err(Into::into)
        })
        .unwrap();
    let why = refusals(&mut rig);
    assert_eq!(why.len(), 1, "{why:?}");
    assert!(why[0].starts_with("afk_report_incomplete: "), "{why:?}");
}

#[test]
fn a_report_heading_may_leave_out_the_comma() {
    let mut rig = afk_rig();
    rig.team_file(
        "afk/REPORT.md",
        &FULL.replace("Not done, and why", "Not done and why"),
    );
    assert!(refusals(&mut rig).is_empty());
}
