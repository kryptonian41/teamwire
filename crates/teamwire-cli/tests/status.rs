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
    teamwire_with(bed, args, &[])
}

/// `teamwire` with the identity variables removed, then `vars` set.
fn teamwire_with(bed: &Bed, args: &[&str], vars: &[(&str, &str)]) -> Run {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_teamwire"))
        .arg("--home")
        .arg(bed.home.path())
        .args(args)
        .env_remove("HERDR_ENV")
        .env_remove("TEAMWIRE_TOKEN")
        .env_remove("TEAMWIRE_TOKEN_FILE")
        .env_remove("TEAMWIRE_ROLE")
        .env_remove("TEAMWIRE_TEAM")
        .env("TEAMWIRE_HOME", bed.home.path())
        .env("TEAMWIRE_CLAUDE_DIR", bed.home.path().join("claude"))
        .envs(vars.iter().copied())
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
    assert!(run.out.contains("  ctx -  branch"), "{}", run.out);
    assert!(run.out.contains("last messages:"), "{}", run.out);
    assert!(run.out.contains("which branch?"), "{}", run.out);
    assert!(
        run.out.contains("deliveries that need attention:"),
        "{}",
        run.out
    );
    assert!(
        run.out.contains("questions without an answer:") && run.out.contains("from web to lead"),
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
fn an_empty_identity_variable_counts_as_unset_for_every_command() {
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
    for vars in [
        &[("TEAMWIRE_ROLE", "")][..],
        &[("TEAMWIRE_TEAM", "")],
        &[
            ("TEAMWIRE_TEAM", ""),
            ("TEAMWIRE_ROLE", ""),
            ("TEAMWIRE_TOKEN", ""),
        ],
    ] {
        let run = teamwire_with(&bed, &["status", "alpha", "--json"], vars);
        assert_eq!(run.code, 0, "{vars:?}: {}", run.err);
        let json: serde_json::Value = serde_json::from_str(&run.out).unwrap();
        assert_eq!(json["lead"]["session_id"], "sess-lead", "{vars:?}: {json}");
        // `done` agrees: this is not a member's session.
        let done = teamwire_with(&bed, &["done", "--status", "done", "--summary", "x"], vars);
        assert_eq!(done.code, 1, "{vars:?}: {}", done.err);
        assert!(
            done.err.contains("are not all set"),
            "{vars:?}: {}",
            done.err
        );
    }
    // A non-empty role still makes a member, which never sees the lead.
    let run = teamwire_with(
        &bed,
        &["status", "alpha", "--json"],
        &[("TEAMWIRE_ROLE", "api")],
    );
    let json: serde_json::Value = serde_json::from_str(&run.out).unwrap();
    assert!(json["lead"].is_null(), "{json}");
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
    assert_eq!(view["watch"]["state"], "none");
    assert_eq!(view["members"][0]["older_build"], false);
    assert!(view["members"][0]["started_by"].is_string(), "{view}");
    assert!(view["members"][0]["context"].is_null(), "{view}");
    assert_eq!(view["unread_deliveries"], serde_json::json!([]));
    assert_eq!(view["unanswered_questions"][0]["from"], "web");
}

#[test]
fn a_watch_whose_heartbeat_is_old_is_stale_in_text_and_json() {
    let mut bed = team();
    // The test's fake clock is far behind the binary's real one.
    bed.start_watch(std::process::id());
    let json = teamwire(&bed, &["status", "alpha", "--json"]);
    let view: serde_json::Value = serde_json::from_str(&json.out).unwrap();
    assert_eq!(view["watch"]["state"], "stale", "{}", json.out);
    assert_eq!(view["watch"]["pid"], std::process::id());
    let text = teamwire(&bed, &["status", "alpha"]);
    assert!(
        text.out
            .contains(&format!("watch: stale (pid {}", std::process::id())),
        "{}",
        text.out
    );
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
fn a_directory_without_a_database_is_skipped_in_the_listing() {
    let bed = team();
    std::fs::create_dir_all(bed.home.path().join("teams").join("ghost")).unwrap();
    let run = teamwire(&bed, &["status"]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert!(
        run.out.contains("alpha") && !run.out.contains("ghost"),
        "{}",
        run.out
    );
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
    assert!(run.err.contains("$TEAMWIRE_HOME/archive/"), "{}", run.err);
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

#[test]
fn role_shows_the_model_and_effort_the_member_was_launched_with() {
    let mut bed = team();
    let now = teamwire_core::Clock::now(&bed.clock);
    let role = teamwire_core::Role::parse("cc").unwrap();
    let new = teamwire_store::NewMember {
        role: role.clone(),
        title: "the cc".to_owned(),
        kind: teamwire_core::MemberKind::Implementer,
        mode: teamwire_core::Mode::Pane,
        harness: teamwire_core::HarnessId::from_static("claude-code"),
        host: teamwire_core::HostId::from_static("herdr"),
        cwd: bed.home.path().to_path_buf(),
        repo: None,
        branch: None,
        base: None,
        base_sha: None,
        check: None,
        owns: vec![],
        deadline_s: 1800,
    };
    bed.store
        .write(|tx| {
            tx.insert_member(now, &new)?;
            tx.set_member_session(&role, &Bed::session("cc"))?;
            tx.set_member_state(
                now,
                &role,
                teamwire_core::MemberState::Started,
                &serde_json::json!({ "model": "opus", "effort": "high" }),
            )
        })
        .unwrap();
    let run = teamwire(&bed, &["status", "alpha", "--role", "cc"]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert!(run.out.contains("--model opus"), "{}", run.out);
    assert!(run.out.contains("--effort high"), "{}", run.out);
}

#[test]
fn received_clears_a_result_from_status_and_a_second_run_changes_nothing() {
    let mut bed = team();
    let id = bed.complete("web", CompletionStatus::Done);
    bed.finish(&id, teamwire_core::ReportState::Undelivered, None);
    let shown = teamwire(&bed, &["status", "alpha"]).out;
    assert!(
        shown.contains("results the lead has not received:"),
        "{shown}"
    );

    let run = teamwire(&bed, &["received", "alpha", "web"]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert_eq!(run.out, "web #1 received\n");
    let shown = teamwire(&bed, &["status", "alpha"]).out;
    assert!(
        !shown.contains("results the lead has not received:"),
        "{shown}"
    );

    let again = teamwire(&bed, &["received", "alpha", "web", "api"]);
    assert_eq!(again.code, 0, "{}{}", again.out, again.err);
    assert_eq!(
        again.out,
        "web unchanged: its newest result is delivered\napi unchanged: its newest result is delivered\n"
    );

    let unknown = teamwire(&bed, &["received", "alpha", "nobody"]);
    assert_eq!(unknown.code, 1, "{}{}", unknown.out, unknown.err);
    assert_eq!(unknown.out, "nobody refused: no member has this role\n");
}

/// Two refs on different providers.
fn two_refs() -> [teamwire_core::TicketRef; 2] {
    let make = |provider: &str, id: &str| teamwire_core::TicketRef {
        provider: provider.to_owned(),
        id: id.to_owned(),
    };
    [make("linear", "GKA-580"), make("github", "#1")]
}

fn two_refs_json() -> serde_json::Value {
    serde_json::json!([
        {"provider": "linear", "id": "GKA-580"},
        {"provider": "github", "id": "#1"},
    ])
}

#[test]
fn refs_show_on_the_team_line_and_in_the_json_and_are_empty_without_any() {
    let mut bed = team();
    let plain = teamwire(&bed, &["status", "alpha"]);
    assert!(!plain.out.contains("refs"), "{}", plain.out);
    let json = teamwire(&bed, &["status", "alpha", "--json"]);
    let view: serde_json::Value = serde_json::from_str(&json.out).unwrap();
    assert_eq!(view["refs"], serde_json::json!([]));

    bed.store.write(|tx| tx.set_refs(&two_refs())).unwrap();
    let run = teamwire(&bed, &["status", "alpha"]);
    assert_eq!(run.code, 0, "{}", run.err);
    let first = run.out.lines().next().unwrap();
    assert!(
        first.ends_with("(refs linear:GKA-580, github:#1)"),
        "{first}"
    );
    let json = teamwire(&bed, &["status", "alpha", "--json"]);
    let view: serde_json::Value = serde_json::from_str(&json.out).unwrap();
    assert_eq!(view["refs"], two_refs_json());
    let all = teamwire(&bed, &["status", "--json"]);
    let all: serde_json::Value = serde_json::from_str(&all.out).unwrap();
    assert_eq!(all[0]["refs"], two_refs_json());
}

#[test]
fn a_member_session_cannot_mark_a_result_received() {
    let mut bed = team();
    let id = bed.complete("web", CompletionStatus::Done);
    bed.finish(&id, teamwire_core::ReportState::Undelivered, None);
    let before = sizes(&bed);
    for vars in [
        &[("TEAMWIRE_TOKEN", "not-a-real-token")][..],
        &[("TEAMWIRE_TEAM", "alpha"), ("TEAMWIRE_ROLE", "api")],
    ] {
        let run = teamwire_with(&bed, &["received", "alpha", "web"], vars);
        assert_eq!(run.code, 1, "{vars:?}: {}{}", run.out, run.err);
        assert!(run.err.contains("refused.identity"), "{}", run.err);
    }
    assert_eq!(sizes(&bed), before, "a refused received changed a file");
    let shown = teamwire(&bed, &["status", "alpha"]).out;
    assert!(
        shown.contains("results the lead has not received:"),
        "{shown}"
    );
}

fn make_afk(bed: &mut Bed, ends: &str) {
    bed.store
        .write(|tx| {
            tx.connection()
                .execute("UPDATE team SET afk_budget_ends_at = ?1", [ends])
                .map(|_| ())
                .map_err(Into::into)
        })
        .unwrap();
    let dir = bed.home.path().join("teams/alpha/afk");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("decisions.md"), "D1 roster\nD2 model\n").unwrap();
}

#[test]
fn an_afk_team_shows_its_line_after_the_header_and_an_afk_object() {
    let mut bed = team();
    make_afk(&mut bed, "2999-01-01T00:00:00.000Z");
    let run = teamwire(&bed, &["status", "alpha"]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    let lines: Vec<&str> = run.out.lines().collect();
    assert!(lines[0].starts_with("team alpha"), "{}", run.out);
    assert!(
        lines[1].starts_with("afk: budget ends 2999-01-01T00:00:00.000Z, ")
            && lines[1].ends_with(" left, 2 decisions"),
        "{}",
        run.out
    );

    let json = teamwire(&bed, &["status", "alpha", "--json"]);
    let view: serde_json::Value = serde_json::from_str(&json.out).unwrap();
    let afk = &view["afk"];
    assert_eq!(afk["budget_ends_at"], "2999-01-01T00:00:00.000Z");
    assert_eq!(
        (&afk["spent"], &afk["decisions"]),
        (&false.into(), &2.into())
    );
    assert!(afk["seconds_left"].as_u64().unwrap() > 1_000_000_000);
}

#[test]
fn a_spent_budget_reads_spent_with_zero_seconds_left() {
    let mut bed = team();
    make_afk(&mut bed, "2020-01-01T00:00:00.000Z");
    let run = teamwire(&bed, &["status", "alpha"]);
    assert!(
        run.out
            .contains("\nafk: budget spent at 2020-01-01T00:00:00.000Z, 2 decisions\n"),
        "{}",
        run.out
    );
    let json = teamwire(&bed, &["status", "alpha", "--json"]);
    let view: serde_json::Value = serde_json::from_str(&json.out).unwrap();
    assert_eq!(view["afk"]["seconds_left"], 0);
    assert_eq!(view["afk"]["spent"], true);
}

#[test]
fn an_ordinary_team_has_a_null_afk_and_no_afk_line() {
    let bed = team();
    let run = teamwire(&bed, &["status", "alpha"]);
    assert!(!run.out.contains("afk:"), "{}", run.out);
    let json = teamwire(&bed, &["status", "alpha", "--json"]);
    let view: serde_json::Value = serde_json::from_str(&json.out).unwrap();
    assert!(view["afk"].is_null() && view.get("afk").is_some(), "{view}");
}

#[test]
fn an_afk_team_with_no_members_still_shows_its_line() {
    let mut bed = Bed::new();
    make_afk(&mut bed, "2999-01-01T00:00:00.000Z");
    let run = teamwire(&bed, &["status", "alpha"]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert!(run.out.contains("\nafk: budget ends "), "{}", run.out);
}

/// The lead's delivery row of message `id` (a display id).
fn lead_row(bed: &Bed, id: &str) -> DeliveryState {
    let id = teamwire_core::DisplayId::parse(id).unwrap();
    bed.store
        .read(|tx| {
            let m = tx.message_by_display_id(&id)?.unwrap();
            Ok(tx
                .delivery(&m.ulid, &teamwire_core::Recipient::Lead)?
                .unwrap()
                .delivery
                .state)
        })
        .unwrap()
}

/// The event types written after `seq`.
fn events_after(bed: &Bed, seq: u64) -> Vec<teamwire_core::EventType> {
    bed.store
        .read(|tx| Ok(tx.events_after(seq, None, None)?.items))
        .unwrap()
        .into_iter()
        .map(|e| e.kind)
        .collect()
}

#[expect(
    clippy::redundant_closure_for_method_calls,
    reason = "`ReadTx::last_event_seq` as a path does not infer the transaction's lifetime for `Store::read`"
)]
fn last_seq(bed: &Bed) -> u64 {
    bed.store.read(|tx| tx.last_event_seq()).unwrap()
}

/// ADR 0043 decision 2: the lead records a message it read from its file,
/// and nothing is typed again.
#[test]
fn received_with_a_message_id_clears_it_from_status_without_typing() {
    let bed = team();
    let shown = teamwire(&bed, &["status", "alpha"]).out;
    assert!(shown.contains("m001 -> lead undelivered"), "{shown}");
    let seq = last_seq(&bed);

    let run = teamwire(&bed, &["received", "alpha", "m001"]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert_eq!(run.out, "m001 read\n");
    assert_eq!(lead_row(&bed, "m001"), DeliveryState::Acked);
    // Only the ack is written: no attempt, resend or hand-off of the message.
    assert_eq!(
        events_after(&bed, seq),
        [teamwire_core::EventType::DeliveryAcked]
    );
    let shown = teamwire(&bed, &["status", "alpha"]).out;
    assert!(!shown.contains("m001 -> lead"), "{shown}");
}

#[test]
fn received_refuses_an_unknown_id_and_a_message_not_to_the_lead() {
    let mut bed = team();
    bed.send(teamwire_core::MessageType::Note, "api", None);
    let before = sizes(&bed);
    let unknown = teamwire(&bed, &["received", "alpha", "m099"]);
    assert_eq!(unknown.code, 1, "{}{}", unknown.out, unknown.err);
    assert_eq!(unknown.out, "m099: no such message\n");
    let elsewhere = teamwire(&bed, &["received", "alpha", "m002"]);
    assert_eq!(elsewhere.code, 1, "{}{}", elsewhere.out, elsewhere.err);
    assert_eq!(elsewhere.out, "m002: not a message to the lead\n");
    assert_eq!(sizes(&bed), before, "a refused received changed a file");
}

#[test]
fn received_refuses_a_name_that_is_both_a_role_and_a_message_id() {
    let mut bed = team();
    bed.add("m001");
    let run = teamwire(&bed, &["received", "alpha", "m001"]);
    assert_eq!(run.code, 1, "{}{}", run.out, run.err);
    assert_eq!(
        run.out,
        "m001: both a role and a message; nothing recorded\n"
    );
    assert_eq!(lead_row(&bed, "m001"), DeliveryState::Undelivered);
}

#[test]
fn received_leaves_a_delivered_or_read_message_as_it_is() {
    let mut bed = team();
    let handed = bed.question("api", DeliveryState::HandedOff);
    let read = bed.question("api", DeliveryState::Unconfirmed);
    let at = teamwire_core::Clock::now(&bed.clock);
    let read_id = teamwire_core::DisplayId::parse(&read).unwrap();
    bed.store
        .write(|tx| {
            let m = tx.reader().message_by_display_id(&read_id)?.unwrap();
            tx.ack_delivery(at, &m.ulid, &teamwire_core::Recipient::Lead)
                .map(|_| ())
        })
        .unwrap();
    let run = teamwire(&bed, &["received", "alpha", &handed, &read]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert_eq!(
        run.out,
        format!("{handed} already delivered\n{read} already read\n")
    );
    assert_eq!(lead_row(&bed, &handed), DeliveryState::HandedOff);
    assert_eq!(lead_row(&bed, &read), DeliveryState::Acked);
}
