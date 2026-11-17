//! The lead's own handoff note and held stop (ADR 0051): each condition that
//! is false on its own means no text, and each text is given once.
#![expect(
    clippy::unwrap_used,
    reason = "a failing unwrap in an integration test fails the test"
)]

use teamwire_core::{
    Clock, Fill, HandoffFiles, HarnessError, HarnessErrorCode, HarnessId, HostId,
    LeadHandoffSetting, LeadRoute, PlatformError, PlatformErrorCode, SessionId, SessionKind,
    TeamName, ToolAction, ToolCall,
};
use teamwire_fakes::{FakeClock, FakeHarness, FakePlatform, TestHome};
use teamwire_report::{lead_handoff_hold, lead_handoff_note};
use teamwire_store::{NewTeam, Store};

const LEAD: &str = "46e0fa45-9b35-4ded-b055-b86c0b68cbc4";

struct Rig {
    home: TestHome,
    clock: FakeClock,
    platform: FakePlatform,
    harness: FakeHarness,
}

fn sid(text: &str) -> SessionId {
    SessionId::parse(text).unwrap()
}

fn call(session: &str, subagent: bool) -> ToolCall {
    ToolCall {
        session: sid(session),
        subagent,
        cwd: None,
        action: ToolAction::Other,
    }
}

impl Rig {
    fn new() -> Self {
        Self {
            home: TestHome::new().unwrap(),
            clock: FakeClock::new(),
            platform: FakePlatform::new(),
            harness: FakeHarness::new(),
        }
    }

    /// An open team led from `lead`.
    fn team(&self, name: &str, lead: &str) {
        let team = TeamName::parse(name).unwrap();
        let mut store = Store::open(self.home.path(), &team).unwrap();
        let now = self.clock.now();
        store
            .write(|tx| {
                tx.create_team(
                    now,
                    &NewTeam {
                        name: team.clone(),
                        title: name.to_owned(),
                        lead_harness: HarnessId::from_static("fake"),
                        lead_host: HostId::from_static("fake"),
                        lead_route: LeadRoute::Sink,
                        lead_session: Some((sid(lead), SessionKind::Interactive)),
                    },
                )
                .map(|_| ())
            })
            .unwrap();
    }

    fn fill(&self, tokens: u64, window: u64) {
        self.harness
            .set_fill(sid(LEAD), Ok(Fill { tokens, window }));
    }

    fn note(&self, call: &ToolCall, setting: &LeadHandoffSetting) -> Option<String> {
        lead_handoff_note(
            self.home.path(),
            call,
            &self.harness,
            &self.platform,
            setting,
        )
    }

    fn hold(&self, setting: &LeadHandoffSetting) -> Option<String> {
        lead_handoff_hold(self.home.path(), &sid(LEAD), &self.platform, setting)
    }

    fn files(&self) -> HandoffFiles {
        HandoffFiles::of(self.home.path(), &sid(LEAD)).unwrap()
    }
}

const UNSET: LeadHandoffSetting = LeadHandoffSetting::Unset;

#[test]
fn a_lead_past_its_point_is_told_once_with_its_numbers() {
    let rig = Rig::new();
    rig.team("alpha", LEAD);
    rig.fill(412_000, 1_000_000);
    let note = rig.note(&call(LEAD, false), &UNSET).unwrap();
    assert!(
        note.starts_with(
            "[teamwire] Your context holds 412k tokens, 41% of a 1000k window. Your handoff point is 400k."
        ),
        "{note}"
    );
    assert!(rig.files().noted.is_file(), "the note was recorded");
    assert_eq!(rig.note(&call(LEAD, false), &UNSET), None, "once only");
}

#[test]
fn a_lead_below_its_point_gets_no_note_and_no_record() {
    let rig = Rig::new();
    rig.team("alpha", LEAD);
    rig.fill(399_999, 1_000_000);
    assert_eq!(rig.note(&call(LEAD, false), &UNSET), None);
    assert!(!rig.files().noted.exists());
}

#[test]
fn the_floor_keeps_a_small_window_or_a_low_setting_from_firing_early() {
    let rig = Rig::new();
    rig.team("alpha", LEAD);
    // 40% of a 200k window is 80k, but the floor is 200k.
    rig.fill(150_000, 200_000);
    assert_eq!(rig.note(&call(LEAD, false), &UNSET), None);
    let low = LeadHandoffSetting::Tokens(0);
    assert_eq!(rig.note(&call(LEAD, false), &low), None);
    rig.fill(200_000, 1_000_000);
    let note = rig.note(&call(LEAD, false), &low).unwrap();
    assert!(note.contains("Your handoff point is 200k."), "{note}");
}

#[test]
fn a_setting_moves_the_point_and_a_bad_one_counts_as_none() {
    let rig = Rig::new();
    rig.team("alpha", LEAD);
    rig.fill(300_000, 1_000_000);
    let bad = LeadHandoffSetting::parse(Some("40%"));
    assert_eq!(
        rig.note(&call(LEAD, false), &bad),
        None,
        "40% of 1M is 400k"
    );
    let set = LeadHandoffSetting::Tokens(250_000);
    let note = rig.note(&call(LEAD, false), &set).unwrap();
    assert!(note.contains("Your handoff point is 250k."), "{note}");
}

#[test]
fn off_a_subagent_or_no_open_team_means_no_note() {
    let rig = Rig::new();
    rig.fill(900_000, 1_000_000);
    assert_eq!(rig.note(&call(LEAD, false), &UNSET), None, "no team at all");
    rig.team("beta", "someone-else");
    assert_eq!(
        rig.note(&call(LEAD, false), &UNSET),
        None,
        "a team another session leads"
    );
    rig.team("alpha", LEAD);
    assert_eq!(rig.note(&call(LEAD, false), &LeadHandoffSetting::Off), None);
    assert_eq!(rig.note(&call(LEAD, true), &UNSET), None, "a subagent");
    assert!(!rig.files().noted.exists());
    assert!(rig.note(&call(LEAD, false), &UNSET).is_some());
}

#[test]
fn an_unknown_fill_or_a_failed_record_means_no_note() {
    let rig = Rig::new();
    rig.team("alpha", LEAD);
    assert_eq!(rig.note(&call(LEAD, false), &UNSET), None, "no reading");
    rig.harness.set_fill(
        sid(LEAD),
        Err(HarnessError::new(HarnessErrorCode::Io, "unreadable")),
    );
    assert_eq!(rig.note(&call(LEAD, false), &UNSET), None, "a failed read");
    rig.fill(900_000, 1_000_000);
    rig.platform
        .fail_writes(Some(PlatformError::new(PlatformErrorCode::Io, "disk full")));
    assert_eq!(rig.note(&call(LEAD, false), &UNSET), None);
    rig.platform.fail_writes(None);
    assert!(rig.note(&call(LEAD, false), &UNSET).is_some());
}

#[test]
fn a_session_id_that_is_no_safe_folder_name_gets_no_note() {
    let rig = Rig::new();
    rig.team("alpha", "../evil");
    rig.harness.set_fill(
        sid("../evil"),
        Ok(Fill {
            tokens: 900_000,
            window: 1_000_000,
        }),
    );
    assert_eq!(rig.note(&call("../evil", false), &UNSET), None);
    assert!(!rig.home.path().join("evil").exists());
}

#[test]
fn the_stop_is_held_once_after_the_note_while_the_lead_leads() {
    let rig = Rig::new();
    rig.team("alpha", LEAD);
    assert_eq!(rig.hold(&UNSET), None, "no note yet");
    rig.fill(900_000, 1_000_000);
    rig.note(&call(LEAD, false), &UNSET).unwrap();
    let held = rig.hold(&UNSET).unwrap();
    assert_eq!(
        held,
        "[teamwire] Your context passed its handoff point. If you are not waiting for the user's answer, run `teamwire lead --handoff-guide` now and follow it. If you are waiting for the user, stop again and hand off after the answer."
    );
    assert!(rig.files().held.is_file());
    assert_eq!(rig.hold(&UNSET), None, "once only");
}

#[test]
fn no_hold_once_the_lead_leads_nothing_or_the_setting_is_off() {
    let rig = Rig::new();
    rig.team("alpha", LEAD);
    rig.fill(900_000, 1_000_000);
    rig.note(&call(LEAD, false), &UNSET).unwrap();
    assert_eq!(rig.hold(&LeadHandoffSetting::Off), None);
    // The team now has another lead, as after a handoff.
    let team = TeamName::parse("alpha").unwrap();
    let mut store = Store::open_existing(rig.home.path(), &team).unwrap();
    let now = rig.clock.now();
    store
        .write(|tx| {
            tx.adopt_lead(now, &sid("new-lead"), SessionKind::Interactive)
                .map(|_| ())
        })
        .unwrap();
    assert_eq!(rig.hold(&UNSET), None);
    assert!(!rig.files().held.exists());
}
