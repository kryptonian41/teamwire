//! `teamwire status`, run as the real binary. Members name adapters this
//! binary does not have, so nothing real is asked about them.
#![expect(
    clippy::disallowed_methods,
    reason = "the test starts the teamwire binary and lists files on disk"
)]
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

#[path = "../../teamwire-watch/tests/support/fixture.rs"]
pub mod fixture;

use std::collections::BTreeMap;

use fixture::Bed;
use teamwire_core::{CompletionStatus, DeliveryState};

struct Run {
    code: i32,
    out: String,
    err: String,
}

fn teamwire(bed: &Bed, args: &[&str]) -> Run {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_teamwire"))
        .arg("--home")
        .arg(bed.home.path())
        .args(args)
        .env_remove("HERDR_ENV")
        .env_remove("TEAMWIRE_TOKEN")
        .env_remove("TEAMWIRE_ROLE")
        .env_remove("TEAMWIRE_TEAM")
        .env("TEAMWIRE_HOME", bed.home.path())
        .env("TEAMWIRE_CLAUDE_DIR", bed.home.path().join("claude"))
        .output()
        .unwrap();
    Run {
        code: out.status.code().unwrap(),
        out: String::from_utf8(out.stdout).unwrap(),
        err: String::from_utf8(out.stderr).unwrap(),
    }
}

fn team() -> Bed {
    let mut bed = Bed::new();
    bed.add("api");
    bed.add("web");
    bed.delivered("api", CompletionStatus::Done);
    bed.question("web", DeliveryState::Undelivered);
    bed
}

/// The size of every file under the home, except `SQLite`'s shared-memory
/// index, which changes whenever any reader attaches.
fn sizes(bed: &Bed) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut pending = vec![bed.home.path().to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else if !path.to_string_lossy().ends_with("-shm") {
                out.insert(path.display().to_string(), std::fs::read(&path).unwrap());
            }
        }
    }
    out
}

#[test]
fn a_team_shows_members_messages_and_stuck_deliveries() {
    let bed = team();
    let before = sizes(&bed);
    let run = teamwire(&bed, &["status", "alpha"]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert!(
        run.out.starts_with("team alpha \"Alpha\" open\n"),
        "{}",
        run.out
    );
    assert!(
        run.out.contains("api") && run.out.contains("done(done)"),
        "{}",
        run.out
    );
    assert!(run.out.contains("last messages:"), "{}", run.out);
    assert!(run.out.contains("which branch?"), "{}", run.out);
    assert!(
        run.out.contains("deliveries that need attention:"),
        "{}",
        run.out
    );
    assert_eq!(sizes(&bed), before, "status changed a file");
}

#[test]
fn a_member_caller_never_sees_the_lead_session() {
    let mut bed = team();
    let now = teamwire_core::Clock::now(&bed.clock);
    bed.store
        .write(|tx| {
            tx.adopt_lead(
                now,
                &Bed::session("lead"),
                teamwire_core::SessionKind::Interactive,
            )
        })
        .unwrap();
    let member = |args: &[&str]| {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_teamwire"))
            .arg("--home")
            .arg(bed.home.path())
            .args(args)
            .env_remove("HERDR_ENV")
            .env("TEAMWIRE_HOME", bed.home.path())
            .env("TEAMWIRE_CLAUDE_DIR", bed.home.path().join("claude"))
            .env("TEAMWIRE_TEAM", "alpha")
            .env("TEAMWIRE_ROLE", "api")
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(0));
        String::from_utf8(out.stdout).unwrap()
    };
    let lead = teamwire(&bed, &["status", "alpha"]);
    assert!(lead.out.contains("lead: session sess-lead"), "{}", lead.out);
    assert!(!member(&["status", "alpha"]).contains("sess-lead"));
    let json: serde_json::Value = serde_json::from_str(&member(&["status", "--json"])).unwrap();
    assert!(json[0]["lead"].is_null(), "{json}");
}

#[test]
fn json_is_the_view() {
    let bed = team();
    let run = teamwire(&bed, &["status", "alpha", "--json"]);
    assert_eq!(run.code, 0, "{}", run.err);
    let view: serde_json::Value = serde_json::from_str(&run.out).unwrap();
    assert_eq!(view["team"], "alpha");
    assert_eq!(view["members"].as_array().unwrap().len(), 2);
    assert_eq!(view["members"][0]["state"], "done(done)");
}

#[test]
fn without_a_team_every_team_is_listed() {
    let bed = team();
    let run = teamwire(&bed, &["status"]);
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(run.out.contains("alpha"), "{}", run.out);
    assert!(run.out.contains("2 members, 1 done"), "{}", run.out);
    let json = teamwire(&bed, &["status", "--json"]);
    let all: serde_json::Value = serde_json::from_str(&json.out).unwrap();
    assert_eq!(all.as_array().unwrap().len(), 1);
}

#[test]
fn an_unreadable_database_exits_6() {
    let bed = team();
    let db = bed.home.path().join("teams").join("alpha").join("team.db");
    let mut bytes = std::fs::read(&db).unwrap();
    for byte in &mut bytes[100..] {
        *byte = 0xA5;
    }
    std::fs::write(&db, bytes).unwrap();
    for suffix in ["-wal", "-shm"] {
        let mut side = db.clone().into_os_string();
        side.push(suffix);
        std::fs::remove_file(side).ok();
    }
    for args in [&["status", "alpha"][..], &["status"], &["status", "--json"]] {
        let run = teamwire(&bed, args);
        assert_eq!(run.code, 6, "{args:?}: {}{}", run.out, run.err);
    }
}

#[test]
fn no_teams_says_so() {
    let home = teamwire_fakes::TestHome::new().unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_teamwire"))
        .arg("--home")
        .arg(home.path())
        .arg("status")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "no teams\n");
}

#[test]
fn an_unknown_team_is_refused() {
    let bed = team();
    let run = teamwire(&bed, &["status", "nosuch"]);
    assert_eq!(run.code, 1);
    assert!(run.err.contains("no team nosuch"), "{}", run.err);
}

#[test]
fn role_needs_a_team_and_a_member() {
    let bed = team();
    assert_eq!(teamwire(&bed, &["status", "--role", "api"]).code, 3);
    let run = teamwire(&bed, &["status", "alpha", "--role", "nobody"]);
    assert_eq!(run.code, 1);
    assert!(run.out.contains("no member nobody"), "{}", run.out);
}

#[test]
fn role_reports_why_a_resume_command_is_missing() {
    // The fake member's harness is not one this binary has.
    let bed = team();
    let run = teamwire(&bed, &["status", "alpha", "--role", "api"]);
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(run.out.contains("no resume command for api"), "{}", run.out);
}
