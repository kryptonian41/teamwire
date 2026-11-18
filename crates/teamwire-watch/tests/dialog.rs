//! The watch's lines for a member at a dialog (ADR 0052): one line per
//! permission dialog the member's hook recorded, a kind for any other
//! dialog, and a pending dialog that waits for the lead is never a stall.
#![expect(
    clippy::indexing_slicing,
    reason = "a test that indexes past the end panics and fails, which is the intended outcome"
)]

#[path = "support/fixture.rs"]
pub mod fixture;

use std::time::Duration;

use fixture::{Bed, count, events, role};
use teamwire_core::{
    Clock as _, DialogKind, DialogVerdict, Host as _, HostCaps, PermissionAnswer, clock_time,
};
use teamwire_watch::WatchMemory;

/// Every event of every line, in order.
fn all(lines: &[String]) -> Vec<String> {
    lines.iter().flat_map(|l| events(l)).collect()
}

fn polls(bed: &mut Bed, ticks: usize) -> Vec<String> {
    let mut lines: Vec<String> = bed.poll().into_iter().collect();
    lines.extend(bed.ticks(ticks));
    lines
}

#[test]
fn a_pending_dialog_prints_one_line_with_its_id_and_time() {
    let mut bed = Bed::new();
    bed.add("api");
    bed.blocked("api");
    let expires = bed
        .clock
        .now()
        .checked_add(Duration::from_secs(100))
        .unwrap();
    let id = bed.dialog("api", "rmdir /usr", Duration::from_secs(100));
    let lines = polls(&mut bed, 3);
    let want = format!(
        "BLOCKED api permission Bash: rmdir /usr (dialog {id}, answer by {})",
        clock_time(expires)
    );
    let events = all(&lines);
    assert_eq!(
        events.iter().filter(|e| e.starts_with("BLOCKED")).count(),
        1,
        "{events:?}"
    );
    assert!(events.contains(&want), "{events:?}");
}

#[test]
fn two_dialogs_in_one_episode_print_two_lines() {
    let mut bed = Bed::new();
    bed.add("api");
    bed.blocked("api");
    let first = bed.dialog("api", "rmdir /usr", Duration::from_secs(100));
    let mut lines = polls(&mut bed, 1);
    let now = bed.clock.now();
    let answer = PermissionAnswer {
        verdict: DialogVerdict::Deny,
        reason: "no".to_owned(),
    };
    bed.store
        .write(|tx| {
            tx.answer_dialog(now, &role("api"), &first, &answer)?;
            tx.close_dialog(now, &first).map(drop)
        })
        .unwrap();
    let second = bed.dialog("api", "rmdir /opt", Duration::from_secs(100));
    lines.extend(bed.ticks(2));
    let events = all(&lines);
    let dialogs: Vec<&String> = events
        .iter()
        .filter(|e| e.starts_with("BLOCKED api permission Bash:"))
        .collect();
    assert_eq!(dialogs.len(), 2, "{events:?}");
    assert!(
        dialogs[0].contains(&format!("(dialog {first},")),
        "{events:?}"
    );
    assert!(
        dialogs[1].contains(&format!("(dialog {second},")),
        "{events:?}"
    );
}

#[test]
fn a_trust_dialog_prints_trust_and_reads_the_screen_once() {
    let mut bed = Bed::new();
    bed.add("api");
    bed.blocked("api");
    bed.screen_kind("api", DialogKind::Trust);
    let lines = polls(&mut bed, 5);
    let events = all(&lines);
    assert!(
        events.contains(&"BLOCKED api trust".to_owned()),
        "{events:?}"
    );
    assert_eq!(count(&lines, "BLOCKED"), 1, "{lines:?}");
    assert_eq!(bed.kind_reads(), 1);
}

#[test]
fn a_pending_dialog_on_a_host_with_no_state_still_prints() {
    let mut bed = Bed::new();
    bed.add("api");
    let mut caps: HostCaps = bed.host.capabilities();
    caps.reports_state = false;
    bed.host.set_caps(caps);
    bed.idle("api");
    let id = bed.dialog("api", "rmdir /usr", Duration::from_secs(100));
    let events = all(&polls(&mut bed, 1));
    assert!(
        events
            .iter()
            .any(|e| e.starts_with("BLOCKED api permission Bash: rmdir /usr")
                && e.contains(&id.to_string())),
        "{events:?}"
    );
}

#[test]
fn a_pending_dialog_of_the_only_working_member_prints_no_team_stalled() {
    let mut bed = Bed::new();
    bed.add("api");
    bed.blocked("api");
    bed.screen_kind("api", DialogKind::Permission);
    bed.dialog("api", "rmdir /usr", Duration::from_secs(100));
    // 80 s of polls, all inside the dialog's 100 s.
    let lines = polls(&mut bed, 4);
    let events = all(&lines);
    assert!(
        events
            .iter()
            .any(|e| e.starts_with("BLOCKED api permission Bash: rmdir /usr")),
        "{events:?}"
    );
    assert_eq!(count(&lines, "TEAM-STALLED"), 0, "{lines:?}");
    assert_eq!(count(&lines, "SILENT"), 0, "{lines:?}");
}

#[test]
fn after_the_hook_gives_up_the_member_counts_as_stuck_again() {
    let mut bed = Bed::new();
    bed.add("api");
    bed.blocked("api");
    bed.screen_kind("api", DialogKind::Permission);
    bed.dialog("api", "rmdir /usr", Duration::from_secs(100));
    let mut lines = polls(&mut bed, 4);
    assert_eq!(count(&lines, "TEAM-STALLED"), 0, "{lines:?}");
    // The hook gives up at 100 s; the dialog is still on the screen.
    lines.extend(bed.ticks(3));
    let events = all(&lines);
    assert!(
        events.contains(&"BLOCKED api permission (teamwire cannot answer it)".to_owned()),
        "{events:?}"
    );
    assert!(
        events.contains(&"TEAM-STALLED api".to_owned()),
        "{events:?}"
    );
}

#[test]
fn a_restarted_watch_inside_an_announced_episode_reads_no_screen() {
    let mut bed = Bed::new();
    bed.add("api");
    bed.blocked("api");
    bed.screen_kind("api", DialogKind::Trust);
    let mut lines = polls(&mut bed, 1);
    assert_eq!(bed.kind_reads(), 1);
    // A new watch process: its memory starts from the store.
    bed.memory = WatchMemory::default();
    lines.extend(bed.ticks(3));
    assert_eq!(bed.kind_reads(), 1, "the restarted watch read the screen");
    assert_eq!(count(&lines, "BLOCKED"), 1, "{lines:?}");
}

#[test]
fn a_pending_dialog_leaves_the_stall_to_the_other_members() {
    // The member at the dialog waits for the lead, as in a question round,
    // so the only other member, stuck at a trust dialog, is the whole stall.
    let mut bed = Bed::new();
    bed.add("api");
    bed.add("web");
    bed.blocked("api");
    bed.blocked("web");
    bed.screen_kind("web", DialogKind::Trust);
    bed.dialog("api", "rmdir /usr", Duration::from_secs(100));
    let lines = polls(&mut bed, 2);
    let events = all(&lines);
    assert!(
        events.contains(&"TEAM-STALLED web".to_owned()),
        "{events:?}"
    );
}

#[test]
fn a_summary_with_hidden_characters_prints_them_escaped_on_one_line() {
    let mut bed = Bed::new();
    bed.add("api");
    bed.blocked("api");
    // A bell, a screen-clearing ESC sequence, a right-to-left override and a
    // newline, all in the command the member's model wrote.
    let id = bed.dialog(
        "api",
        "echo \u{7}a\u{1b}[2Jb\u{202e}c\nrm -rf /",
        Duration::from_secs(100),
    );
    let lines = polls(&mut bed, 3);
    let events = all(&lines);
    let mine: Vec<&String> = events
        .iter()
        .filter(|e| e.contains(&id.to_string()))
        .collect();
    assert_eq!(mine.len(), 1, "{events:?}");
    let line = mine[0];
    assert!(
        line.contains(r"echo \u{7}a\u{1b}[2Jb\u{202e}c rm -rf /"),
        "{line}"
    );
    assert!(
        !line.chars().any(|c| c.is_control() || c == '\u{202e}'),
        "{line:?}"
    );
}
