//! The member hooks run in-process against two fake harnesses, so a test can
//! tell which harness a hook asked. The lead's hooks run here too, and the
//! tool hook also through the real harnesses' parsers.

use std::sync::Arc;

use serde_json::json;
use teamwire_core::harness::WatchStart;
use teamwire_core::{
    Clock, CompletionStatus, Harness, HarnessId, HostId, LeadRoute, MemberKind, MemberState, Mode,
    Registry, SessionId,
};
use teamwire_deliver::{WatchText, watch_text};
use teamwire_fakes::{FakeClock, FakeHarness, TestHome};
use teamwire_store::{NewMember, NewTeam};

use super::lead::{lead_harness_of, lead_stop_in, lead_tool_in, lead_watch_start};
use super::*;
use crate::wiring::{WiringEnv, wire};
use teamwire_core::SessionKind;

const TOKEN: &str = "member-token";

struct Bed {
    home: TestHome,
    team: TeamName,
    role: Role,
    wiring: Wiring,
    a: Arc<FakeHarness>,
    b: Arc<FakeHarness>,
}

/// The wiring of a command run with no environment, with every real harness.
fn real_wiring() -> Wiring {
    wire(&WiringEnv {
        herdr_env: None,
        tmux: None,
        user_home: None,
        claude_dir: None,
        claude_config_dir: None,
        codex_test_home: None,
        codex_home: None,
        pi_agent_dir: None,
        lead_harness: None,
        claude_session: None,
        codex_thread: None,
        pi_marker: None,
    })
    .unwrap()
}

/// A team whose only member `api` runs on `fake-b`, with a token and, when
/// `session` is given, a recorded session.
fn bed(session: Option<&str>) -> Bed {
    let home = TestHome::new().unwrap();
    let team = TeamName::parse("alpha").unwrap();
    let role = Role::parse("api").unwrap();
    let (a, b) = (
        Arc::new(FakeHarness::with_id(HarnessId::from_static("fake-a"))),
        Arc::new(FakeHarness::with_id(HarnessId::from_static("fake-b"))),
    );
    let mut wiring = real_wiring();
    let mut registry = Registry::new();
    registry.add_harness(a.clone()).unwrap();
    registry.add_harness(b.clone()).unwrap();
    wiring.registry = Arc::new(registry);
    let mut store = Store::open(home.path(), &team).unwrap();
    let now = FakeClock::new().now();
    store
        .write(|tx| {
            tx.create_team(
                now,
                &NewTeam {
                    name: team.clone(),
                    title: "Alpha".to_owned(),
                    lead_harness: HarnessId::from_static("fake-a"),
                    lead_host: HostId::from_static("none"),
                    lead_route: LeadRoute::Sink,
                    lead_session: Some((
                        SessionId::parse("lead-1").unwrap(),
                        SessionKind::Interactive,
                    )),
                },
            )?;
            tx.insert_member(
                now,
                &NewMember {
                    role: role.clone(),
                    title: "the api".to_owned(),
                    kind: MemberKind::Investigator,
                    mode: Mode::Pane,
                    harness: HarnessId::from_static("fake-b"),
                    host: HostId::from_static("none"),
                    cwd: std::env::temp_dir(),
                    repo: None,
                    branch: None,
                    base: None,
                    base_sha: None,
                    check: None,
                    owns: Vec::new(),
                    deadline_s: 1800,
                },
            )?;
            tx.set_member_state(now, &role, MemberState::Started, &json!({}))?;
            tx.set_member_token_hash(&role, &teamwire_msg::token_hash(TOKEN))?;
            if let Some(s) = session {
                tx.set_member_session(&role, &SessionId::parse(s).unwrap())?;
            }
            Ok(())
        })
        .unwrap();
    Bed {
        home,
        team,
        role,
        wiring,
        a,
        b,
    }
}

fn run(bed: &Bed, kind: MemberHook, token: Option<&str>, input: &str) -> Option<Vec<u8>> {
    member_hook_in(
        bed.home.path(),
        &bed.team,
        &bed.role,
        kind,
        token,
        &bed.wiring,
        &mut input.as_bytes(),
    )
}

fn input(session: &str) -> String {
    json!({ "session_id": session, "stop_hook_active": false }).to_string()
}

fn recorded(bed: &Bed) -> Option<SessionId> {
    let store = Store::open_existing(bed.home.path(), &bed.team).unwrap();
    store
        .read(|tx| tx.member(&bed.role))
        .unwrap()
        .unwrap()
        .session_id
}

/// A completion with no report file holds the member's first stop.
fn complete(bed: &Bed) {
    let mut store = Store::open_existing(bed.home.path(), &bed.team).unwrap();
    let config = Config::default();
    let clock = FakeClock::new();
    let mut ctx = Ctx {
        home: bed.home.path(),
        store: &mut store,
        registry: &bed.wiring.registry,
        platform: bed.wiring.platform.as_ref(),
        clock: &clock,
        config: &config,
    };
    teamwire_report::done(&mut ctx, &bed.role, CompletionStatus::Done, "finished").unwrap();
}

#[test]
fn a_turn_end_asks_the_harness_the_member_row_names() {
    let bed = bed(Some("s-1"));
    complete(&bed);
    let reply = run(&bed, MemberHook::TurnEnd, None, &input("s-1")).unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&reply)
            .unwrap()
            .get("decision"),
        Some(&json!("block"))
    );
    assert!(bed.b.log().count_named("parse_turn_end") >= 1);
    assert_eq!(bed.a.log().count_named("parse_turn_end"), 0);
    assert_eq!(bed.a.log().count_named("hold_reply"), 0);
}

#[test]
fn a_harness_without_an_in_turn_hook_gets_no_post_tool_reply() {
    let bed = bed(Some("s-1"));
    let mut caps = bed.b.capabilities();
    caps.in_turn_hook = false;
    bed.b.set_caps(caps);
    assert_eq!(run(&bed, MemberHook::PostTool, None, &input("s-1")), None);
    assert_eq!(bed.b.log().count_named("parse_turn_end"), 0);
}

#[test]
fn the_right_token_records_the_session() {
    let bed = bed(None);
    assert_eq!(
        run(&bed, MemberHook::SessionStart, Some(TOKEN), &input("s-1")),
        None
    );
    assert_eq!(recorded(&bed), Some(SessionId::parse("s-1").unwrap()));
    assert_eq!(bed.a.log().count_named("parse_turn_end"), 0);
}

#[test]
fn a_wrong_or_missing_token_records_nothing() {
    let bed = bed(None);
    run(
        &bed,
        MemberHook::SessionStart,
        Some("not-the-token"),
        &input("s-1"),
    );
    run(&bed, MemberHook::SessionStart, None, &input("s-1"));
    assert_eq!(recorded(&bed), None);
}

#[test]
fn a_removed_member_is_ignored() {
    let bed = bed(None);
    let mut store = Store::open_existing(bed.home.path(), &bed.team).unwrap();
    let now = FakeClock::new().now();
    store
        .write(|tx| {
            tx.remove_member(now, &bed.role, None, &json!({}))
                .map(|_| ())
        })
        .unwrap();
    run(&bed, MemberHook::SessionStart, Some(TOKEN), &input("s-1"));
    assert_eq!(recorded(&bed), None);
}

#[test]
fn a_second_call_replaces_the_first_session() {
    let bed = bed(None);
    run(&bed, MemberHook::SessionStart, Some(TOKEN), &input("s-1"));
    run(&bed, MemberHook::SessionStart, Some(TOKEN), &input("s-2"));
    assert_eq!(recorded(&bed), Some(SessionId::parse("s-2").unwrap()));
}

fn lead_stop(bed: &Bed, harness: Option<&str>) -> Option<Vec<u8>> {
    lead_stop_in(
        harness,
        bed.home.path(),
        &bed.wiring,
        &mut input("lead-1").as_bytes(),
    )
}

#[test]
fn the_lead_stop_hook_parses_and_holds_with_the_named_harness() {
    let bed = bed(None);
    let reply = lead_stop(&bed, Some("fake-b")).unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&reply)
            .unwrap()
            .get("decision"),
        Some(&json!("block"))
    );
    assert_eq!(bed.b.log().count_named("hold_reply"), 1);
    assert_eq!(bed.a.log().count_named("hold_reply"), 0);
    assert_eq!(bed.a.log().count_named("parse_turn_end"), 0);
}

#[test]
fn the_lead_stop_hook_names_the_background_watch_for_a_lead_without_watch_lines() {
    let bed = bed(None);
    // The team's recorded lead harness is `fake-a`.
    let mut caps = bed.a.capabilities();
    caps.reads_watch_lines = false;
    caps.watch_start = WatchStart::BackgroundCommand;
    bed.a.set_caps(caps);
    let reply = lead_stop(&bed, Some("fake-a")).unwrap();
    let doc: serde_json::Value = serde_json::from_slice(&reply).unwrap();
    let note = doc.get("reason").and_then(|n| n.as_str()).unwrap();
    assert!(
        note.contains("nohup teamwire watch alpha --deliver"),
        "{note}"
    );
}

#[test]
fn the_lead_stop_hook_names_the_watch_tool_for_a_lead_that_calls_one() {
    let bed = bed(None);
    // The team's recorded lead harness is `fake-a`.
    let mut caps = bed.a.capabilities();
    caps.watch_start = WatchStart::ExtensionTool;
    bed.a.set_caps(caps);
    let reply = lead_stop(&bed, Some("fake-a")).unwrap();
    let doc: serde_json::Value = serde_json::from_slice(&reply).unwrap();
    let note = doc.get("reason").and_then(|n| n.as_str()).unwrap();
    assert!(
        note.contains("start it with the teamwire_watch tool (team \"alpha\")"),
        "{note}"
    );
}

#[test]
fn the_lead_stop_hook_with_an_unknown_harness_says_nothing() {
    let bed = bed(None);
    assert_eq!(lead_stop(&bed, Some("no-such-harness")), None);
}

#[test]
fn a_lead_harness_that_cannot_be_found_gets_the_monitor_wording() {
    let mut bed = bed(None);
    let lead = lead_harness_of(bed.home.path(), &bed.team);
    assert_eq!(lead.as_ref().map(HarnessId::as_str), Some("fake-a"));
    bed.wiring.registry = Arc::new(Registry::new());
    assert_eq!(
        lead_watch_start(lead.as_ref(), &bed.wiring),
        WatchStart::MonitoredCommand
    );
    // A team that could not be read gives no harness, and the same wording.
    let gone = TeamName::parse("gone").unwrap();
    assert_eq!(lead_harness_of(bed.home.path(), &gone), None);
    assert_eq!(
        lead_watch_start(None, &bed.wiring),
        WatchStart::MonitoredCommand
    );
}

/// The four texts that tell a lead how to start its watch, for the lead
/// harness `id` of this binary: the Stop hold, the end of a result line, the
/// line after a reprompt, and launch's `watch:` value.
fn watch_texts_of(id: &str) -> [String; 4] {
    let start = lead_watch_start(Some(&HarnessId::parse(id).unwrap()), &real_wiring());
    let team = TeamName::parse("alpha").unwrap();
    [
        teamwire_report::watch_missing_note(&team, start),
        watch_text(start, WatchText::ResultSuffix, &team),
        watch_text(start, WatchText::AfterReprompt, &team),
        watch_text(start, WatchText::Launch, &team),
    ]
}

#[test]
fn a_claude_code_lead_is_told_to_run_the_watch_under_monitor() {
    assert_eq!(
        watch_texts_of("claude-code"),
        [
            "[teamwire:alpha] no watch is running; start `teamwire watch alpha` with Monitor before you stop.",
            "(no watch is running: teamwire watch alpha)",
            "no watch is running: start one with `teamwire watch alpha`",
            "teamwire watch alpha",
        ]
    );
}

#[test]
fn a_codex_lead_is_told_to_run_the_watch_in_the_background() {
    assert_eq!(
        watch_texts_of("codex"),
        [
            "[teamwire:alpha] no watch is running; start it in the background with `nohup teamwire watch alpha --deliver >/dev/null 2>&1 &` before you stop.",
            "(no watch is running: nohup teamwire watch alpha --deliver >/dev/null 2>&1 &)",
            "no watch is running: start one with `nohup teamwire watch alpha --deliver >/dev/null 2>&1 &`",
            "nohup teamwire watch alpha --deliver >/dev/null 2>&1 &",
        ]
    );
}

#[test]
fn a_pi_lead_is_told_to_call_the_watch_tool() {
    assert_eq!(
        watch_texts_of("pi"),
        [
            "[teamwire:alpha] no watch is running; start it with the teamwire_watch tool (team \"alpha\") before you stop.",
            "(no watch is running: call the teamwire_watch tool with team alpha)",
            "no watch is running: start one with the teamwire_watch tool (team alpha)",
            "call the teamwire_watch tool with team alpha",
        ]
    );
}

fn lead_tool(
    bed: &Bed,
    wiring: &Wiring,
    harness: Option<&str>,
    input: &serde_json::Value,
) -> Option<String> {
    let reply = lead_tool_in(
        harness,
        bed.home.path(),
        wiring,
        &mut input.to_string().as_bytes(),
    )?;
    let doc: serde_json::Value = serde_json::from_slice(&reply).unwrap();
    let note = doc
        .get("hookSpecificOutput")
        .and_then(|o| o.get("additionalContext"))
        .and_then(serde_json::Value::as_str);
    Some(note.unwrap().to_owned())
}

#[test]
fn the_lead_tool_hook_notes_a_build_through_every_harness_that_can_lead() {
    let bed = bed(None);
    let real = real_wiring();
    let shared = json!({
        "session_id": "lead-1", "tool_name": "Bash", "tool_input": {"command": "cargo test"},
    });
    let pi = json!({
        "session_id": "lead-1", "tool_name": "bash", "tool_input": {"command": "cargo test"},
    });
    for (harness, input) in [
        (None, &shared),
        (Some("claude-code"), &shared),
        (Some("codex"), &shared),
        (Some("pi"), &pi),
    ] {
        let note = lead_tool(&bed, &real, harness, input).unwrap();
        assert!(
            note.starts_with("[teamwire:alpha] You lead team alpha, so leave this build"),
            "{harness:?}: {note}"
        );
    }
}

#[test]
fn the_lead_tool_hook_says_nothing_for_a_read_another_session_or_no_tool_parser() {
    let bed = bed(None);
    let real = real_wiring();
    let read = json!({
        "session_id": "lead-1", "tool_name": "Read", "tool_input": {"file_path": "/x"},
    });
    assert_eq!(lead_tool(&bed, &real, None, &read), None);
    let build = json!({
        "session_id": "lead-1", "tool_name": "Bash", "tool_input": {"command": "cargo test"},
    });
    let elsewhere = json!({
        "session_id": "lead-2", "tool_name": "Bash", "tool_input": {"command": "cargo test"},
    });
    assert_eq!(lead_tool(&bed, &real, None, &elsewhere), None);
    // The fakes keep the default parser, which refuses every input.
    assert_eq!(lead_tool(&bed, &bed.wiring, Some("fake-a"), &build), None);
    assert_eq!(
        lead_tool(&bed, &real, Some("no-such-harness"), &build),
        None
    );
}
