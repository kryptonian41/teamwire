//! `Herdr` as a `Host`, over `fake-herdr`.
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "helpers in an integration-test file unwrap and index; a failure there fails the test"
)]

#[path = "support/helpers.rs"]
mod helpers;

use helpers::{T, World};
use teamwire_core::{Address, Host, HostErrorCode, Key, Place, PlaceState, SubmitAck, one_line};
use teamwire_fakes::HerdrFaults;

fn started(world: &World, name: &str, title: &str) -> Place {
    world
        .herdr
        .spawn(&world.request(name, title), T)
        .expect("spawn")
}

fn name(text: &str) -> Address {
    Address::parse(text).expect("address")
}

fn pane_closes(world: &World) -> usize {
    calls(world)
        .iter()
        .filter(|c| c.starts_with(&["pane".to_owned(), "close".to_owned()]))
        .count()
}

fn calls(world: &World) -> Vec<Vec<String>> {
    world.env.calls().into_iter().map(|c| c.argv).collect()
}

#[test]
fn capabilities_follow_the_brief() {
    let caps = World::new().herdr.capabilities();
    assert!(caps.can_type && caps.reports_state && caps.knows_session_of_place);
    assert!(caps.guards_dialogs);
    assert!(!caps.sets_env_at_spawn);
    assert_eq!(caps.submit_ack, SubmitAck::Submitted);
    assert_eq!(World::new().herdr.place_env_var(), Some("HERDR_PANE_ID"));
}

#[test]
fn first_member_makes_the_team_workspace_and_labels_its_tab() {
    let world = World::new();
    started(&world, "tw_demo_api", "API");
    assert_eq!(world.calls_of("workspace create").len(), 1);
    assert_eq!(world.calls_of("tab create").len(), 0);
    let all = calls(&world);
    let create = all
        .iter()
        .find(|c| c[..2] == ["workspace", "create"])
        .unwrap();
    assert!(
        create.windows(2).any(|w| w == ["--label", "demo"]),
        "{create:?}"
    );
    let rename = all.iter().find(|c| c[..2] == ["tab", "rename"]).unwrap();
    assert_eq!(rename.last().map(String::as_str), Some("API"));
}

#[test]
fn second_member_gets_a_tab_in_the_same_workspace() {
    let world = World::new();
    started(&world, "tw_demo_api", "API");
    started(&world, "tw_demo_web", "Web");
    assert_eq!(world.calls_of("workspace create").len(), 1);
    assert_eq!(world.calls_of("tab create").len(), 1);
    // Labels are set once: one rename, for the first member's tab only.
    assert_eq!(world.calls_of("tab rename").len(), 1);
    let all = calls(&world);
    let tab = all.iter().find(|c| c[..2] == ["tab", "create"]).unwrap();
    assert!(tab.windows(2).any(|w| w == ["--label", "Web"]), "{tab:?}");
}

#[test]
fn another_team_gets_its_own_workspace() {
    let world = World::new();
    started(&world, "tw_demo_api", "API");
    started(&world, "tw_other_api", "API");
    assert_eq!(world.calls_of("workspace create").len(), 2);
}

#[test]
fn agent_start_passes_kind_a_valid_timeout_and_only_the_arguments() {
    let world = World::new();
    started(&world, "tw_demo_api", "API");
    let all = calls(&world);
    let start = all.iter().find(|c| c[..2] == ["agent", "start"]).unwrap();
    assert_eq!(start[2], "tw_demo_api");
    assert!(start.windows(2).any(|w| w == ["--kind", "claude"]));
    let ms: u64 = start
        .windows(2)
        .find(|w| w[0] == "--timeout")
        .map(|w| w[1].parse().unwrap())
        .unwrap();
    assert!((3_001..=300_000).contains(&ms), "{ms}");
    let after: Vec<&String> = start.iter().skip_while(|a| *a != "--").skip(1).collect();
    assert_eq!(
        after.first().map(|a| a.as_str()),
        Some("-n"),
        "program word dropped: {start:?}"
    );
}

#[test]
fn a_name_already_live_is_bad_argument_naming_the_code() {
    let world = World::new();
    started(&world, "tw_demo_api", "API");
    let e = world
        .herdr
        .spawn(&world.request("tw_demo_api", "Again"), T)
        .expect_err("name is live");
    assert_eq!(e.code, HostErrorCode::BadArgument);
    assert!(e.detail.starts_with("agent_name_taken:"), "{}", e.detail);
}

#[test]
fn state_maps_status_and_a_gone_agent() {
    let world = World::new();
    let place = started(&world, "tw_demo_api", "API");
    assert_eq!(world.herdr.state(&place, T).unwrap(), PlaceState::Idle);
    world
        .herdr
        .submit(&place, &one_line("work", 20), T)
        .unwrap();
    // After a turn herdr reports `done`, which counts as idle.
    let mut state = PlaceState::Working;
    for _ in 0..100 {
        state = world.herdr.state(&place, T).unwrap();
        if state == PlaceState::Idle {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert_eq!(state, PlaceState::Idle);
    world.herdr.close(&place, &name("tw_demo_api"), T).unwrap();
    assert_eq!(world.herdr.state(&place, T).unwrap(), PlaceState::Gone);
}

#[test]
fn occupant_reports_the_session_from_agent_get() {
    let world = World::new();
    let place = started(&world, "tw_demo_api", "API");
    let occupant = world.herdr.occupant(&place, T).unwrap();
    assert!(occupant.session.is_some(), "{occupant:?}");
    assert!(occupant.pids.is_empty());
}

#[test]
fn read_tail_returns_the_screen_lines() {
    let world = World::new();
    let place = started(&world, "tw_demo_api", "API");
    world
        .herdr
        .submit(&place, &one_line("hello there", 40), T)
        .unwrap();
    // A whole screen line, so text wrapped in a JSON envelope does not pass.
    let mut seen = false;
    for _ in 0..100 {
        let tail = world.herdr.read_tail(&place, 50, T).unwrap();
        if tail.lines().any(|l| l.trim() == "> hello there") {
            seen = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(seen, "the typed text shows as its own line in the tail");
}

#[test]
fn send_keys_maps_each_key_to_herdrs_name() {
    let world = World::new();
    let place = started(&world, "tw_demo_api", "API");
    world
        .herdr
        .send_keys(
            &place,
            &[
                Key::Down,
                Key::Enter,
                Key::Escape,
                Key::CtrlC,
                Key::Char('1'),
            ],
            T,
        )
        .unwrap();
    let all = calls(&world);
    let keys = all
        .iter()
        .find(|c| c[..2] == ["agent", "send-keys"])
        .unwrap();
    assert_eq!(keys[3..], ["down", "enter", "esc", "ctrl+c", "1"]);
    let before = world.calls_of("agent send-keys").len();
    world.herdr.send_keys(&place, &[], T).unwrap();
    assert_eq!(
        world.calls_of("agent send-keys").len(),
        before,
        "no keys, no call"
    );
}

#[test]
fn unnamed_agents_are_skipped_not_fatal() {
    let world = World::new();
    started(&world, "tw_demo_api", "API");
    let unnamed = world.env.add_unnamed_agent().unwrap();
    let names = world
        .herdr
        .live_agent_names(T)
        .expect("the list still parses");
    assert_eq!(names, ["tw_demo_api"]);
    let places = world.herdr.list(T).unwrap();
    assert_eq!(places.len(), 1);
    assert!(places.iter().all(|p| p.id != unnamed));
}

#[test]
fn submit_uses_no_wait_and_reports_a_blocked_agent_by_code() {
    let world = World::new();
    let place = started(&world, "tw_demo_api", "API");
    world.herdr.submit(&place, &one_line("hi", 20), T).unwrap();
    let all = calls(&world);
    let prompt = all.iter().find(|c| c[..2] == ["agent", "prompt"]).unwrap();
    assert!(!prompt.iter().any(|a| a == "--wait"), "{prompt:?}");
}

#[test]
fn a_missing_place_is_not_found_for_reads_and_gone_for_state() {
    let world = World::new();
    let missing = Place {
        host: world.herdr.id(),
        id: "w9:p9".to_owned(),
    };
    assert_eq!(
        world.herdr.occupant(&missing, T).unwrap_err().code,
        HostErrorCode::NotFound
    );
    assert_eq!(world.herdr.state(&missing, T).unwrap(), PlaceState::Gone);
}

#[test]
fn a_foreign_workspace_with_the_team_label_gets_no_tabs() {
    let world = World::new();
    let out = world
        .env
        .run(&["workspace", "create", "--label", "demo"])
        .unwrap();
    let foreign = out.result().unwrap()["workspace"]["workspace_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let first = started(&world, "tw_demo_api", "API");
    let own = first.id.split(':').next().unwrap().to_owned();
    assert_ne!(own, foreign, "the first member gets a workspace of its own");
    let second = started(&world, "tw_demo_web", "Web");
    assert_eq!(second.id.split(':').next().unwrap(), own);
    assert_eq!(world.calls_of("tab create").len(), 1);
}

#[test]
fn a_failed_agent_start_closes_the_pane_it_made() {
    let faults = HerdrFaults::new().error("agent start", None, None, "agent_start_failed");
    let world = World::with_faults(Some(&faults));
    let e = world
        .herdr
        .spawn(&world.request("tw_demo_api", "API"), T)
        .expect_err("start fails");
    assert_ne!(e.code, HostErrorCode::NotReady);
    let all = calls(&world);
    let made = all
        .iter()
        .any(|c| c.starts_with(&["workspace".to_owned(), "create".to_owned()]));
    let closed = all
        .iter()
        .any(|c| c.starts_with(&["pane".to_owned(), "close".to_owned()]));
    assert!(made && closed, "{all:?}");
}

#[test]
fn close_leaves_a_pane_that_holds_another_agent() {
    let world = World::new();
    started(&world, "tw_demo_api", "API");
    let reused = started(&world, "tw_other_web", "Web");
    let e = world
        .herdr
        .close(&reused, &name("tw_demo_api"), T)
        .expect_err("another agent's pane");
    assert_eq!(e.code, HostErrorCode::NotFound);
    assert_eq!(pane_closes(&world), 0);
    assert_eq!(world.herdr.list(T).expect("list").len(), 2);
}

#[test]
fn close_leaves_a_pane_with_no_agent() {
    let world = World::new();
    let unnamed = Place {
        host: world.herdr.id(),
        id: world.env.add_unnamed_agent().expect("unnamed agent"),
    };
    let out = world
        .env
        .run(&["workspace", "create", "--label", "mine"])
        .expect("workspace create");
    let plain = Place {
        host: world.herdr.id(),
        id: out.result().expect("result")["root_pane"]["pane_id"]
            .as_str()
            .expect("root pane id")
            .to_owned(),
    };
    for place in [unnamed, plain] {
        let e = world
            .herdr
            .close(&place, &name("tw_demo_api"), T)
            .expect_err("no such agent");
        assert_eq!(e.code, HostErrorCode::NotFound, "{place:?}");
    }
    assert_eq!(pane_closes(&world), 0);
}
