//! `teamwire send`, `resend`, `log`, `members` and `lead --adopt` through the
//! real binary, with `fake-herdr` and `fake-claude` first on `PATH`, a
//! temporary home, and members that are live fake sessions. The exit codes
//! are those of `docs/api/cli.md`.

#![cfg(test)]
#![expect(
    clippy::disallowed_methods,
    reason = "these tests run the real teamwire binary"
)]

use std::io::{BufRead as _, BufReader};
use std::process::{Child, Command, Output, Stdio};

use serde_json::Value;
use teamwire_core::{
    Actor, HarnessId, HostId, LeadRoute, MemberKind, MemberState, MessageType, Mode, Place,
    Recipient, Role, SessionId, SessionKind, TeamName, Timestamp,
};
use teamwire_fakes::{ClaudeScript, FakeHerdrEnv, HerdrFaults, TestHome};
use teamwire_store::{NewMember, NewMessage, NewTeam, Store};

struct Rig {
    herdr: FakeHerdrEnv,
    home: TestHome,
}

fn at() -> Timestamp {
    Timestamp::from_unix_millis(1_790_000_000_000).unwrap()
}

fn token_of(role: &str) -> String {
    format!("token-of-{role}")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

impl Rig {
    /// Team `alpha`; the lead's session is `lead-1`.
    fn new() -> Self {
        let mut herdr = FakeHerdrEnv::new().unwrap();
        herdr.set_script(&ClaudeScript::default()).unwrap();
        let home = TestHome::new().unwrap();
        let mut store = Store::open(home.path(), &TeamName::parse("alpha").unwrap()).unwrap();
        store
            .write(|tx| {
                tx.create_team(
                    at(),
                    &NewTeam {
                        name: TeamName::parse("alpha").unwrap(),
                        title: "Alpha".to_owned(),
                        lead_harness: HarnessId::from_static("claude-code"),
                        lead_host: HostId::from_static("herdr"),
                        lead_route: LeadRoute::Sink,
                        lead_session: Some((
                            SessionId::parse("lead-1").unwrap(),
                            SessionKind::Interactive,
                        )),
                    },
                )
                .map(|_| ())
            })
            .unwrap();
        Self { herdr, home }
    }

    fn store(&self) -> Store {
        Store::open(self.home.path(), &TeamName::parse("alpha").unwrap()).unwrap()
    }

    /// A member with a live `fake-claude` session in its own herdr pane.
    /// `live` false records the member and leaves no process behind it.
    fn add_member(&self, role: &str, kind: MemberKind, live: bool) {
        let session = format!("session-{role}");
        let mut place = None;
        if live {
            let ws = self
                .herdr
                .run(&[
                    "workspace",
                    "create",
                    "--cwd",
                    "/tmp",
                    "--label",
                    role,
                    "--no-focus",
                ])
                .unwrap();
            let pane = ws
                .result()
                .and_then(|r| r.pointer("/root_pane/pane_id")?.as_str().map(str::to_owned))
                .unwrap();
            let settings = self
                .herdr
                .write_file(&format!("{role}-settings.json"), r#"{"env":{},"hooks":{}}"#)
                .unwrap();
            let name = format!("tw_alpha_{role}");
            let started = self
                .herdr
                .run(&[
                    "agent",
                    "start",
                    &name,
                    "--kind",
                    "claude",
                    "--pane",
                    &pane,
                    "--timeout",
                    "20000",
                    "--",
                    "-n",
                    &name,
                    "--session-id",
                    &session,
                    "--settings",
                    &settings.display().to_string(),
                ])
                .unwrap();
            assert_eq!(started.code, Some(0), "{started:?}");
            place = Some(Place {
                host: HostId::from_static("herdr"),
                id: pane,
            });
        }
        let r = Role::parse(role).unwrap();
        let hash = teamwire_msg::token_hash(&token_of(role));
        let mut store = self.store();
        store
            .write(|tx| {
                tx.insert_member(
                    at(),
                    &NewMember {
                        role: r.clone(),
                        title: role.to_owned(),
                        kind,
                        mode: Mode::Pane,
                        harness: HarnessId::from_static("claude-code"),
                        host: HostId::from_static("herdr"),
                        cwd: "/tmp".into(),
                        repo: None,
                        branch: None,
                        base: None,
                        base_sha: None,
                        check: None,
                        owns: vec![],
                        deadline_s: 1800,
                    },
                )?;
                tx.set_member_session(&r, &SessionId::parse(&session).unwrap())?;
                if let Some(place) = &place {
                    tx.set_member_place(&r, place)?;
                }
                tx.set_member_token_hash(&r, &hash)?;
                tx.set_member_state(at(), &r, MemberState::Started, &serde_json::json!({}))
            })
            .unwrap();
    }

    /// The command for `teamwire <args>` with a cleared environment: the
    /// fakes' `PATH`, a temporary home, and the caller's identity.
    fn command(&self, args: &[&str], identity: &Identity) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_teamwire"));
        cmd.args(args)
            .env_clear()
            .envs(self.herdr.env())
            .env("TEAMWIRE_HOME", self.home.path())
            .env("HERDR_ENV", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        match identity {
            Identity::Member(role) => {
                cmd.env("TEAMWIRE_TEAM", "alpha")
                    .env("TEAMWIRE_ROLE", role)
                    .env("TEAMWIRE_TOKEN", token_of(role));
            }
            Identity::TokenFile(role) => {
                let file = self.home.path().join(format!("token-{role}"));
                std::fs::write(&file, format!("{}\n", token_of(role))).unwrap();
                cmd.env("TEAMWIRE_TEAM", "alpha")
                    .env("TEAMWIRE_ROLE", role)
                    .env("TEAMWIRE_TOKEN_FILE", file);
            }
            Identity::WrongToken(role) => {
                cmd.env("TEAMWIRE_TEAM", "alpha")
                    .env("TEAMWIRE_ROLE", role)
                    .env("TEAMWIRE_TOKEN", "not-the-token");
            }
            Identity::TokenlessMember(role) => {
                cmd.env("TEAMWIRE_TEAM", "alpha").env("TEAMWIRE_ROLE", role);
            }
            Identity::Lead(session) => {
                cmd.env("CLAUDE_CODE_SESSION_ID", session);
            }
            Identity::User => {
                cmd.env("TEAMWIRE_USER_KEY", self.user_key());
            }
            Identity::UserWith(name, value) => {
                cmd.env("TEAMWIRE_USER_KEY", self.user_key())
                    .env(name, value);
            }
            Identity::Nobody => {}
        }
        cmd
    }

    fn run(&self, args: &[&str], identity: &Identity) -> Output {
        self.command(args, identity).output().unwrap()
    }

    /// The user key, made by `teamwire user-key` on first use.
    fn user_key(&self) -> String {
        let out = self.run(&["user-key"], &Identity::Nobody);
        assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
        std::fs::read_to_string(text(&out.stdout).trim()).unwrap()
    }
}

enum Identity {
    Member(&'static str),
    WrongToken(&'static str),
    /// A member that has only `TEAMWIRE_TOKEN_FILE` (ADR 0023 point 4).
    TokenFile(&'static str),
    /// A member's session after `env -u TEAMWIRE_TOKEN`.
    TokenlessMember(&'static str),
    Lead(&'static str),
    /// The human user, with the key `user-key` made (ADR 0040).
    User,
    /// The user's key, and one more variable set to a value.
    UserWith(&'static str, &'static str),
    Nobody,
}

const LEAD: Identity = Identity::Lead("lead-1");

fn live_rig() -> Rig {
    let rig = Rig::new();
    rig.add_member("scout", MemberKind::Investigator, true);
    rig.add_member("api", MemberKind::Implementer, true);
    rig
}

#[test]
fn a_member_with_only_a_token_file_can_send() {
    let rig = live_rig();
    let out = rig.run(
        &[
            "send", "alpha", "--to", "api", "--type", "finding", "--text", "via file",
        ],
        &Identity::TokenFile("scout"),
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}{}",
        text(&out.stdout),
        text(&out.stderr)
    );
}

#[test]
fn a_finding_to_a_live_member_is_handed_off_with_exit_0() {
    let rig = live_rig();
    let out = rig.run(
        &[
            "send",
            "alpha",
            "--to",
            "api",
            "--type",
            "finding",
            "--text",
            "the answer is 42",
        ],
        &Identity::Member("scout"),
    );
    let (stdout, stderr) = (text(&out.stdout), text(&out.stderr));
    assert_eq!(out.status.code(), Some(0), "{stdout} / {stderr}");
    assert!(
        stdout.starts_with("m001\n") && stdout.contains("api: handed_off"),
        "{stdout}"
    );
    assert!(stderr.contains("stored m001 (key "), "{stderr}");
    assert!(!stderr.contains("NOT DELIVERED"), "{stderr}");
    let typed: Vec<String> = rig
        .herdr
        .calls()
        .into_iter()
        .filter(|c| c.command().starts_with("agent prompt"))
        .map(|c| c.command())
        .collect();
    assert_eq!(typed.len(), 1, "{typed:?}");
}

#[test]
fn id_and_key_printed_before_host_call() {
    let mut rig = live_rig();
    // Every host call the locator or the submit makes blocks, so the command
    // is still inside its first host call when the id and key are read.
    let mut faults = HerdrFaults::new();
    for cmd in [
        "agent list",
        "agent get",
        "agent prompt",
        "agent read",
        "pane get",
        "pane list",
    ] {
        faults = faults.hang(cmd, None, None);
    }
    rig.herdr.set_faults(&faults).unwrap();
    let mut child: Child = rig
        .command(
            &[
                "send", "alpha", "--to", "api", "--type", "finding", "--text", "x", "--key",
                "k-777",
            ],
            &Identity::Member("scout"),
        )
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stderr.take().unwrap()).lines();
    let first = lines.next().unwrap().unwrap();
    assert!(first.contains("stored m001 (key k-777)"), "{first}");
    assert!(
        child.try_wait().unwrap().is_none(),
        "the command had already finished when the key was printed"
    );
    child.kill().unwrap();
    child.wait().unwrap();
    // Killed mid-delivery: the message is stored under its key, still pending.
    let store = rig.store();
    let message = store
        .read(|tx| {
            tx.message_by_client_key(&teamwire_core::Actor::parse("scout").unwrap(), "k-777")
        })
        .unwrap()
        .unwrap();
    let delivery = store
        .read(|tx| {
            tx.delivery(
                &message.ulid,
                &teamwire_core::Recipient::parse("api").unwrap(),
            )
        })
        .unwrap()
        .unwrap();
    assert_eq!(
        delivery.delivery.state,
        teamwire_core::DeliveryState::Pending
    );
    // A resume within the minute cannot tell the killed sender from one
    // still typing, so it types nothing and says so: exit 5, as for a line
    // that may or may not be on the screen.
    rig.herdr.set_faults(&HerdrFaults::new()).unwrap();
    let out = rig.run(
        &[
            "send", "alpha", "--to", "api", "--type", "finding", "--text", "x", "--key", "k-777",
        ],
        &Identity::Member("scout"),
    );
    let stderr = text(&out.stderr);
    assert_eq!(out.status.code(), Some(5), "{stderr}");
    assert!(text(&out.stdout).contains("api: skipped"), "{out:?}");
    assert!(
        stderr.contains(
            "SKIPPED for api: another command may still be delivering it; if it does not arrive, run teamwire resend alpha m001 after 60 s"
        ),
        "{stderr}"
    );
}

#[test]
fn an_undelivered_recipient_exits_4_with_the_reason_on_stderr() {
    let rig = live_rig();
    // The lead has no live process in this rig.
    let out = rig.run(
        &[
            "send",
            "alpha",
            "--to",
            "lead",
            "--type",
            "question",
            "--text",
            "which branch?",
        ],
        &Identity::Member("api"),
    );
    let (stdout, stderr) = (text(&out.stdout), text(&out.stderr));
    assert_eq!(out.status.code(), Some(4), "{stdout} / {stderr}");
    assert!(
        stderr.contains("NOT DELIVERED to lead: no_live_process"),
        "{stderr}"
    );
    assert!(
        stdout.contains("lead: undelivered (no_live_process)"),
        "{stdout}"
    );
}

#[test]
fn a_prompt_that_timed_out_after_typing_exits_5_and_a_resend_by_the_lead_finishes_it() {
    let mut rig = live_rig();
    rig.herdr
        .set_faults(&HerdrFaults::new().timed_out_prompt(None, Some(1)))
        .unwrap();
    let out = rig.run(
        &[
            "send", "alpha", "--to", "api", "--type", "finding", "--text", "x",
        ],
        &Identity::Member("scout"),
    );
    let (stdout, stderr) = (text(&out.stdout), text(&out.stderr));
    assert_eq!(out.status.code(), Some(5), "{stdout} / {stderr}");
    assert!(stderr.contains("UNCONFIRMED to api"), "{stderr}");
    // Only the lead or the sender may resend; a resend delivers it.
    let other = rig.run(&["resend", "alpha", "m001"], &Identity::Member("api"));
    assert_eq!(other.status.code(), Some(1), "{}", text(&other.stderr));
    assert!(text(&other.stderr).contains("refused.not_resendable"));
    let again = rig.run(&["resend", "alpha", "m001"], &LEAD);
    let (stdout, stderr) = (text(&again.stdout), text(&again.stderr));
    assert_eq!(again.status.code(), Some(0), "{stdout} / {stderr}");
    assert!(stdout.contains("api: handed_off"), "{stdout}");
    // A message that was reached has nothing to resend.
    let nothing = rig.run(&["resend", "alpha", "m001"], &LEAD);
    assert_eq!(nothing.status.code(), Some(1));
}

/// The host call that finds who holds a member's place (`agent get`) starts
/// `kill -0` while `fake-herdr` holds its state lock. A `kill` that sleeps
/// once makes that call run past the 5 s limit of a delivery, which kills it
/// at that point. The next send must still find the member: a lock the killed
/// call left behind would make every later call of the world fail with
/// `host_listing_failed`.
#[test]
fn a_host_call_killed_on_its_timeout_does_not_break_the_next_send() {
    let rig = live_rig();
    let spy = rig.home.path().join("slow-kill");
    std::fs::create_dir_all(&spy).unwrap();
    let marker = spy.join("slept");
    let script = format!(
        "#!/bin/sh\nif [ ! -e '{m}' ]; then : > '{m}'; sleep 9; fi\nexec /bin/kill \"$@\"\n",
        m = marker.display()
    );
    let kill = spy.join("kill");
    std::fs::write(&kill, script).unwrap();
    std::fs::set_permissions(&kill, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let path = rig
        .herdr
        .env()
        .into_iter()
        .find_map(|(k, v)| (k == "PATH").then_some(v))
        .unwrap();
    let send = |text: &str| {
        let mut cmd = rig.command(
            &[
                "send", "alpha", "--to", "api", "--type", "finding", "--text", text,
            ],
            &Identity::Member("scout"),
        );
        cmd.env("PATH", format!("{}:{path}", spy.display()));
        cmd.output().unwrap()
    };
    let first = send("slow host call");
    let (stdout, stderr) = (text(&first.stdout), text(&first.stderr));
    assert_ne!(first.status.code(), Some(0), "{stdout} / {stderr}");
    assert!(
        marker.exists(),
        "the slow kill never ran: {stdout} / {stderr}"
    );
    let second = send("after the kill");
    let (stdout, stderr) = (text(&second.stdout), text(&second.stderr));
    assert_eq!(second.status.code(), Some(0), "{stdout} / {stderr}");
}

#[test]
fn a_refusal_exits_1_names_its_code_and_stores_nothing() {
    let rig = live_rig();
    let out = rig.run(
        &[
            "send", "alpha", "--to", "scout", "--type", "reprompt", "--text", "x",
        ],
        &Identity::Member("api"),
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stderr).contains("refused.type_not_allowed"),
        "{}",
        text(&out.stderr)
    );
    let store = rig.store();
    let stored = store
        .read(|tx| {
            Ok(tx
                .messages(&teamwire_store::MessageFilter::default())?
                .items
                .len())
        })
        .unwrap();
    assert_eq!(stored, 0);
    // A text over the limit says both numbers.
    let long = "y".repeat(900);
    let out = rig.run(
        &[
            "send", "alpha", "--to", "api", "--type", "finding", "--text", &long,
        ],
        &Identity::Member("scout"),
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stderr)
            .contains("refused.too_long: the text is 900 characters; the limit is 800"),
        "{}",
        text(&out.stderr)
    );
}

#[test]
fn a_caller_that_cannot_prove_who_it_is_is_refused() {
    let rig = live_rig();
    let args = [
        "send", "alpha", "--to", "api", "--type", "note", "--text", "x",
    ];
    // A wrong token, nobody at all, the lead's claim from a plain terminal,
    // and the lead's claim from another session.
    for (identity, from) in [
        (Identity::WrongToken("scout"), None),
        (Identity::Nobody, Some("lead")),
        (Identity::Nobody, None),
        (Identity::Lead("someone-else"), Some("lead")),
    ] {
        let mut all: Vec<&str> = args.to_vec();
        if let Some(from) = from {
            all.extend(["--from", from]);
        }
        let out = rig.run(&all, &identity);
        assert_eq!(out.status.code(), Some(1), "{}", text(&out.stderr));
        assert!(
            text(&out.stderr).contains("refused.identity"),
            "{}",
            text(&out.stderr)
        );
    }
    let store = rig.store();
    let refused = store
        .read(|tx| {
            tx.events_of_type(teamwire_core::EventType::MessageRefused)?
                .all_or_corrupt()
        })
        .unwrap();
    assert_eq!(refused.len(), 4);
    assert!(
        rig.herdr
            .calls()
            .iter()
            .all(|c| !c.command().starts_with("agent prompt"))
    );
}

#[test]
fn the_lead_reprompts_and_is_told_whether_a_watch_runs() {
    let rig = live_rig();
    let out = rig.run(
        &[
            "send",
            "alpha",
            "--from",
            "lead",
            "--to",
            "api",
            "--type",
            "reprompt",
            "--text",
            "fix the findings",
        ],
        &LEAD,
    );
    let (stdout, stderr) = (text(&out.stdout), text(&out.stderr));
    assert_eq!(out.status.code(), Some(0), "{stdout} / {stderr}");
    assert!(
        stdout.contains("no watch is running: start one with `teamwire watch alpha`"),
        "{stdout}"
    );
    let api = rig
        .store()
        .read(|tx| tx.member(&Role::parse("api").unwrap()))
        .unwrap()
        .unwrap();
    assert_eq!(api.generation, 2);
}

#[test]
fn the_lead_sends_without_naming_itself() {
    let rig = live_rig();
    let out = rig.run(
        &[
            "send", "alpha", "--to", "api", "--type", "note", "--text", "merged",
        ],
        &LEAD,
    );
    let (stdout, stderr) = (text(&out.stdout), text(&out.stderr));
    assert_eq!(out.status.code(), Some(0), "{stdout} / {stderr}");
    assert!(stdout.contains("api"), "{stdout}");
}

#[test]
fn usage_errors_exit_3_and_store_nothing() {
    let rig = live_rig();
    let out = rig.run(
        &[
            "send", "alpha", "--to", "Bad-Name", "--type", "finding", "--text", "x",
        ],
        &Identity::Member("scout"),
    );
    assert_eq!(out.status.code(), Some(3), "{}", text(&out.stderr));
    let out = rig.run(
        &[
            "send",
            "alpha",
            "--to",
            "api",
            "--type",
            "finding",
            "--file",
            "/no/such/file",
        ],
        &Identity::Member("scout"),
    );
    assert_eq!(out.status.code(), Some(3), "{}", text(&out.stderr));
    // An unknown type is a refusal that is logged, not a usage error.
    let out = rig.run(
        &[
            "send", "alpha", "--to", "api", "--type", "gossip", "--text", "x",
        ],
        &Identity::Member("scout"),
    );
    assert_eq!(out.status.code(), Some(1), "{}", text(&out.stderr));
    assert!(text(&out.stderr).contains("refused.type_not_allowed"));
    // A team with no database is a refusal too.
    let out = rig.run(
        &[
            "send",
            "nosuchteam",
            "--from",
            "lead",
            "--to",
            "api",
            "--type",
            "note",
            "--text",
            "x",
        ],
        &LEAD,
    );
    assert_eq!(out.status.code(), Some(1), "{}", text(&out.stderr));
}

#[test]
fn the_text_can_come_from_a_file() {
    let rig = live_rig();
    let path = rig
        .herdr
        .write_file("finding.txt", "long text from a file")
        .unwrap();
    let out = rig.run(
        &[
            "send",
            "alpha",
            "--to",
            "api",
            "--type",
            "finding",
            "--file",
            path.to_str().unwrap(),
        ],
        &Identity::Member("scout"),
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let store = rig.store();
    let stored = store
        .read(|tx| tx.message_by_display_id(&teamwire_core::DisplayId::parse("m001").unwrap()))
        .unwrap()
        .unwrap();
    assert_eq!(stored.body, "long text from a file");
}

#[test]
fn a_member_session_without_its_token_cannot_read_the_whole_log() {
    let rig = live_rig();
    rig.run(
        &[
            "send", "alpha", "--to", "api", "--type", "finding", "--text", "to api",
        ],
        &Identity::Member("scout"),
    );
    let out = rig.run(&["log", "alpha"], &Identity::TokenlessMember("api"));
    assert_eq!(out.status.code(), Some(1), "{}", text(&out.stdout));
    assert!(!text(&out.stdout).contains("m001"), "{}", text(&out.stdout));
    assert!(text(&out.stderr).contains("refused.identity"));
}

#[test]
fn log_shows_states_and_a_member_sees_only_its_own_messages() {
    let rig = live_rig();
    rig.add_member("web", MemberKind::Implementer, true);
    rig.run(
        &[
            "send", "alpha", "--to", "api", "--type", "finding", "--text", "to api",
        ],
        &Identity::Member("scout"),
    );
    rig.run(
        &[
            "send", "alpha", "--to", "web", "--type", "finding", "--text", "to web",
        ],
        &Identity::Member("scout"),
    );
    // No token: the person at the terminal sees everything.
    let all = rig.run(&["log", "alpha"], &Identity::Nobody);
    let shown = text(&all.stdout);
    assert_eq!(all.status.code(), Some(0), "{}", text(&all.stderr));
    assert!(shown.contains("m001") && shown.contains("m002"), "{shown}");
    assert!(
        shown.contains("api: handed_off") && shown.contains("scout -> "),
        "{shown}"
    );
    // A member sees what it sent or received.
    let mine = rig.run(&["log", "alpha"], &Identity::Member("api"));
    let shown = text(&mine.stdout);
    assert!(shown.contains("m001") && !shown.contains("m002"), "{shown}");
    // A wrong token is refused, not shown everything.
    let bad = rig.run(&["log", "alpha"], &Identity::WrongToken("api"));
    assert_eq!(bad.status.code(), Some(1));
    // JSON and the role filter.
    let json = rig.run(
        &["log", "alpha", "--role", "web", "--json"],
        &Identity::Nobody,
    );
    let value: Value = serde_json::from_slice(&json.stdout).unwrap();
    let rows = value.get("messages").and_then(Value::as_array).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows.first().unwrap().get("display_id"),
        Some(&Value::from("m002"))
    );
}

#[test]
fn log_json_rows_carry_re_and_commit() {
    let rig = live_rig();
    for (who, args) in [
        (
            Identity::Member("scout"),
            &[
                "send", "alpha", "--to", "lead", "--type", "question", "--text", "which?",
            ][..],
        ),
        (
            Identity::Lead("lead-1"),
            &[
                "send", "alpha", "--to", "scout", "--type", "answer", "--re", "m001", "--text",
                "this",
            ][..],
        ),
        (
            Identity::Lead("lead-1"),
            &[
                "send", "alpha", "--to", "scout", "--type", "note", "--text", "plain",
            ][..],
        ),
    ] {
        // The lead has no live process here, so a send to it exits 4; the
        // message is stored all the same, and the log below proves it.
        rig.run(args, &who);
    }
    // A notice is checked against git in the sender's checkout, which this
    // rig has none of, so the notice is stored directly.
    let notice = NewMessage {
        sender: Actor::parse("api").unwrap(),
        kind: MessageType::Notice,
        re: None,
        commit: Some("3f2a9c1e5b7d40a8c6d2e1f0a9b8c7d6e5f40312".to_owned()),
        body: "renamed".to_owned(),
        client_key: None,
        recipients: vec![Recipient::Member(Role::parse("scout").unwrap())],
    };
    rig.store()
        .write(|tx| tx.insert_message(at(), &notice).map(|_| ()))
        .unwrap();
    let out = rig.run(&["log", "alpha", "--json"], &Identity::Nobody);
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    let rows = value.get("messages").and_then(Value::as_array).unwrap();
    let keys: Vec<(Value, Value)> = rows
        .iter()
        .map(|r| (r["re"].clone(), r["commit"].clone()))
        .collect();
    assert_eq!(
        keys,
        [
            (Value::Null, Value::Null),
            (Value::from("m001"), Value::Null),
            (Value::Null, Value::Null),
            (
                Value::Null,
                Value::from("3f2a9c1e5b7d40a8c6d2e1f0a9b8c7d6e5f40312")
            ),
        ]
    );
}

#[test]
fn members_lists_the_roster_without_secrets() {
    let rig = live_rig();
    let out = rig.run(&["members", "alpha", "--json"], &Identity::Nobody);
    let shown = text(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let value: Value = serde_json::from_str(&shown).unwrap();
    let members = value.get("members").and_then(Value::as_array).unwrap();
    assert_eq!(members.len(), 2);
    for secret in ["token", "session-scout", "hash"] {
        assert!(!shown.contains(secret), "{secret} in {shown}");
    }
    let plain = rig.run(&["members"], &Identity::Member("api"));
    assert_eq!(plain.status.code(), Some(0), "{}", text(&plain.stderr));
    assert!(text(&plain.stdout).contains("scout"));
}

#[test]
fn lead_adopt_records_the_new_session_and_prints_both() {
    let rig = live_rig();
    let out = rig.run(&["lead", "--adopt", "alpha"], &Identity::Lead("lead-2"));
    let stdout = text(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{stdout} / {}",
        text(&out.stderr)
    );
    assert!(stdout.contains("from lead-1 to lead-2"), "{stdout}");
    let team = rig
        .store()
        .read(|tx| {
            let team = tx.team()?;
            Ok(team)
        })
        .unwrap()
        .unwrap();
    assert_eq!(team.lead_session.unwrap().as_str(), "lead-2");
    // A member, and a caller with no session, cannot adopt.
    for identity in [Identity::Member("api"), Identity::Nobody] {
        let refused = rig.run(&["lead", "--adopt", "alpha"], &identity);
        assert_eq!(refused.status.code(), Some(1), "{}", text(&refused.stderr));
    }
    let team = rig
        .store()
        .read(|tx| {
            let team = tx.team()?;
            Ok(team)
        })
        .unwrap()
        .unwrap();
    assert_eq!(team.lead_session.unwrap().as_str(), "lead-2");
}

#[test]
fn a_users_note_exits_0_when_only_the_lead_copy_fails() {
    let rig = live_rig();
    // The lead has no live process in this rig, so its copy is not delivered.
    let out = rig.run(
        &[
            "send", "alpha", "--from", "user", "--to", "api", "--type", "note", "--text", "hi",
        ],
        &Identity::User,
    );
    let (stdout, stderr) = (text(&out.stdout), text(&out.stderr));
    assert_eq!(out.status.code(), Some(0), "{stdout} / {stderr}");
    assert!(stdout.contains("api: handed_off"), "{stdout}");
    assert!(stdout.contains("lead: undelivered"), "{stdout}");
    assert!(
        stderr.lines().any(|l| l
            == "lead copy not delivered: m001; run teamwire resend alpha m001 --to lead"),
        "{stderr}"
    );
    let log = rig.run(&["log", "alpha", "--json"], &Identity::Nobody);
    let value: Value = serde_json::from_slice(&log.stdout).unwrap();
    let row = value.pointer("/messages/0").unwrap();
    assert_eq!(row.get("from"), Some(&Value::from("user")));
    assert_eq!(row.get("to"), Some(&serde_json::json!(["api", "lead"])));
}

#[test]
fn a_users_note_to_the_lead_counts_the_lead_in_the_exit_code() {
    let rig = live_rig();
    let out = rig.run(
        &[
            "send", "alpha", "--from", "user", "--to", "lead", "--type", "note", "--text", "hi",
        ],
        &Identity::User,
    );
    let (stdout, stderr) = (text(&out.stdout), text(&out.stderr));
    assert_eq!(out.status.code(), Some(4), "{stdout} / {stderr}");
    assert!(!stderr.contains("lead copy"), "{stderr}");
}

#[test]
fn any_member_variable_or_a_wrong_key_refuses_the_user() {
    let rig = live_rig();
    let send = [
        "send", "alpha", "--from", "user", "--to", "api", "--type", "note", "--text", "hi",
    ];
    for identity in [
        Identity::UserWith("TEAMWIRE_TOKEN", "token-of-api"),
        // A token file that cannot be read still marks a member's session.
        Identity::UserWith("TEAMWIRE_TOKEN_FILE", "/nonexistent/token"),
        Identity::UserWith("TEAMWIRE_ROLE", "api"),
        Identity::UserWith("TEAMWIRE_TEAM", "alpha"),
        Identity::UserWith("TEAMWIRE_USER_KEY", "not-the-key"),
    ] {
        let out = rig.run(&send, &identity);
        let stderr = text(&out.stderr);
        assert_eq!(out.status.code(), Some(1), "{stderr}");
        assert!(stderr.contains("refused.identity"), "{stderr}");
    }
    // Without the key file nothing proves the user.
    let mut no_file = rig.command(&send, &Identity::Nobody);
    std::fs::remove_file(rig.home.path().join("user.key")).unwrap();
    let out = no_file
        .env("TEAMWIRE_USER_KEY", "anything")
        .output()
        .unwrap();
    let stderr = text(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("refused.identity"), "{stderr}");
}

#[test]
fn the_user_resends_its_own_lead_copy_and_nothing_else() {
    let rig = live_rig();
    rig.run(
        &[
            "send",
            "alpha",
            "--to",
            "api",
            "--type",
            "finding",
            "--text",
            "from scout",
        ],
        &Identity::Member("scout"),
    );
    let out = rig.run(
        &[
            "send", "alpha", "--from", "user", "--to", "api", "--type", "note", "--text", "hi",
        ],
        &Identity::User,
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let retry = rig.run(
        &["resend", "alpha", "m002", "--to", "lead"],
        &Identity::User,
    );
    let (stdout, stderr) = (text(&retry.stdout), text(&retry.stderr));
    // Proven and retried; the lead is still not live, so it exits 4.
    assert_eq!(retry.status.code(), Some(4), "{stdout} / {stderr}");
    assert!(stdout.contains("lead: undelivered"), "{stdout}");
    let theirs = rig.run(&["resend", "alpha", "m001"], &Identity::User);
    let stderr = text(&theirs.stderr);
    assert_eq!(theirs.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("refused.not_resendable"), "{stderr}");
}

#[test]
fn status_events_and_export_name_the_user_as_the_sender() {
    let rig = live_rig();
    let out = rig.run(
        &[
            "send",
            "alpha",
            "--from",
            "user",
            "--to",
            "api",
            "--type",
            "reprompt",
            "--text",
            "start over",
        ],
        &Identity::User,
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let status = rig.run(&["status", "alpha", "--json"], &Identity::Nobody);
    let view: Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(view.pointer("/messages/0/from"), Some(&Value::from("user")));
    let events = rig.run(&["events", "alpha", "--json"], &Identity::Nobody);
    let events: Value = serde_json::from_slice(&events.stdout).unwrap();
    let sent: Vec<&Value> = events
        .get("events")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .filter(|e| e.get("type") == Some(&Value::from("message.sent")))
        .collect();
    assert_eq!(sent.len(), 1, "{events}");
    assert_eq!(
        sent.first().and_then(|e| e.pointer("/data/sender_kind")),
        Some(&Value::from("user"))
    );
    let export = rig.run(&["export", "alpha"], &Identity::Nobody);
    let message = text(&export.stdout)
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .find(|r| r.get("type") == Some(&Value::from("teamwire.message")))
        .unwrap();
    assert_eq!(
        message.pointer("/data/message/sender"),
        Some(&Value::from("user")),
        "{message}"
    );
}
