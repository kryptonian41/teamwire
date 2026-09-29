//! `teamwire metrics`, `teamwire metrics export` and `teamwire export`, run as
//! the real binary against teams in a temporary `$TEAMWIRE_HOME`.
#![expect(
    clippy::disallowed_methods,
    reason = "the test starts the teamwire binary"
)]
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::get_unwrap,
    clippy::collapsible_if,
    reason = "helper functions in an integration-test file unwrap and index; a failure there fails the test"
)]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::process::Output;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use teamwire_core::{
    Actor, EventType, HarnessId, HostId, LeadRoute, MemberKind, MessageType, Mode, Recipient, Role,
    TeamName, Timestamp, Ulid,
};
use teamwire_fakes::TestHome;
use teamwire_store::{NewMember, NewMessage, NewTeam, Store, Waiver};

struct Run {
    code: i32,
    out: String,
    err: String,
}

fn teamwire(home: &TestHome, envs: &[(&str, &str)], args: &[&str]) -> Run {
    let Output {
        status,
        stdout,
        stderr,
    } = std::process::Command::new(env!("CARGO_BIN_EXE_teamwire"))
        .arg("--home")
        .arg(home.path())
        .args(args)
        .env_remove("HERDR_ENV")
        .env_remove("OTEL_EXPORTER_OTLP_ENDPOINT")
        .env_remove("OTEL_EXPORTER_OTLP_HEADERS")
        .env_remove("OTEL_EXPORTER_OTLP_TIMEOUT")
        .env("TEAMWIRE_HOME", home.path())
        .envs(envs.iter().copied())
        .output()
        .unwrap();
    Run {
        code: status.code().unwrap(),
        out: String::from_utf8(stdout).unwrap(),
        err: String::from_utf8(stderr).unwrap(),
    }
}

fn at(secs: i64) -> Timestamp {
    Timestamp::from_unix_millis(1_790_000_000_000 + secs * 1000).unwrap()
}

fn open(home: &TestHome, team: &str) -> Store {
    let name = TeamName::parse(team).unwrap();
    let mut store = Store::open(home.path(), &name).unwrap();
    store
        .write(|tx| {
            tx.create_team(
                at(0),
                &NewTeam {
                    name: name.clone(),
                    title: team.to_owned(),
                    lead_harness: HarnessId::from_static("claude-code"),
                    lead_host: HostId::from_static("herdr"),
                    lead_route: LeadRoute::Sink,
                    lead_session: None,
                },
            )
        })
        .unwrap();
    store
}

fn ev(store: &mut Store, secs: i64, kind: EventType, subject: Option<&Ulid>, data: &Value) {
    store
        .write(|tx| tx.record_event(at(secs), kind, None, subject, data))
        .unwrap();
}

/// A team with two watch lines and one message that was handed off and read.
fn busy_team(home: &TestHome, team: &str) -> Store {
    let mut s = open(home, team);
    ev(
        &mut s,
        5,
        EventType::WatchLine,
        None,
        &json!({"events": ["SILENT"]}),
    );
    ev(
        &mut s,
        6,
        EventType::WatchLine,
        None,
        &json!({"events": []}),
    );
    let m = Ulid::from_parts(at(10), 3);
    ev(
        &mut s,
        10,
        EventType::MessageSent,
        Some(&m),
        &json!({"type": "status", "sender_kind": "member", "recipients": ["lead"]}),
    );
    ev(
        &mut s,
        11,
        EventType::DeliveryHandedOff,
        Some(&m),
        &json!({"route": "inbox", "attempts": 1, "seconds": 1}),
    );
    s
}

/// A receiver that records the path and headers of each request.
struct Receiver {
    addr: SocketAddr,
    seen: Arc<Mutex<Vec<(String, String)>>>,
}

fn receiver(status: u16) -> Receiver {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&seen);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let log = Arc::clone(&log);
            std::thread::spawn(move || {
                // Read the whole request before answering: closing with
                // unread bytes resets the connection under the client.
                let mut raw = Vec::new();
                let mut chunk = [0_u8; 4096];
                let mut wanted = None;
                loop {
                    if wanted.is_none() {
                        if let Some(end) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                            let head = String::from_utf8_lossy(raw.get(..end).unwrap())
                                .to_ascii_lowercase();
                            let length: usize = head
                                .lines()
                                .find_map(|l| l.strip_prefix("content-length:"))
                                .and_then(|v| v.trim().parse().ok())
                                .unwrap_or(0);
                            wanted = Some(end + 4 + length);
                        }
                    }
                    if wanted.is_some_and(|total| raw.len() >= total) {
                        break;
                    }
                    match stream.read(&mut chunk) {
                        Ok(0) | Err(_) => return,
                        Ok(n) => raw.extend_from_slice(chunk.get(..n).unwrap()),
                    }
                }
                let head = String::from_utf8_lossy(&raw).into_owned();
                let path = head
                    .lines()
                    .next()
                    .unwrap()
                    .split(' ')
                    .nth(1)
                    .unwrap()
                    .to_owned();
                log.lock().unwrap().push((path, head.to_ascii_lowercase()));
                write!(
                    stream,
                    "HTTP/1.1 {status} X\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                )
                .ok();
            });
        }
    });
    Receiver { addr, seen }
}

impl Receiver {
    fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn paths(&self) -> Vec<String> {
        self.seen
            .lock()
            .unwrap()
            .iter()
            .map(|(p, _)| p.clone())
            .collect()
    }

    fn heads(&self) -> Vec<String> {
        self.seen
            .lock()
            .unwrap()
            .iter()
            .map(|(_, h)| h.clone())
            .collect()
    }
}

fn mark(home: &TestHome, team: &str) -> u64 {
    let store = Store::open_read_only(home.path(), &TeamName::parse(team).unwrap()).unwrap();
    #[expect(
        clippy::redundant_closure_for_method_calls,
        reason = "a method path does not satisfy `Store::read`'s closure bound"
    )]
    store.read(|tx| tx.export_mark()).unwrap().last_event_seq
}

#[test]
fn metrics_for_a_team_prints_one_line_per_value() {
    let home = TestHome::new().unwrap();
    busy_team(&home, "alpha");
    let run = teamwire(&home, &[], &["metrics", "alpha"]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert!(
        run.out
            .contains("teamwire.watch.lines{team=alpha} 2 {line}\n"),
        "{}",
        run.out
    );
    assert!(
        run.out.contains(
            "teamwire.messages.sent{team=alpha,type=status,sender_kind=member} 1 {message}"
        ),
        "{}",
        run.out
    );
    // The derived lines the lead asks for most.
    assert!(
        run.out.contains("teamwire.messages.read_share{team=alpha}"),
        "{}",
        run.out
    );
}

#[test]
fn json_output_is_the_view() {
    let home = TestHome::new().unwrap();
    busy_team(&home, "alpha");
    let run = teamwire(&home, &[], &["metrics", "alpha", "--json"]);
    assert_eq!(run.code, 0, "{}", run.err);
    let view: Value = serde_json::from_str(&run.out).unwrap();
    let lines = view["metrics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["name"] == "teamwire.watch.lines")
        .unwrap();
    assert_eq!(lines["value"], 2.0);
    assert_eq!(lines["unit"], "{line}");
    assert_eq!(view["errors"], json!([]));
}

#[test]
fn without_a_team_every_team_and_archive_is_read() {
    let home = TestHome::new().unwrap();
    busy_team(&home, "alpha");
    busy_team(&home, "beta");
    let other = TestHome::new().unwrap();
    let gamma = busy_team(&other, "gamma");
    let dir = home.path().join("archive").join("gamma-2026-09-30");
    std::fs::create_dir_all(&dir).unwrap();
    gamma.backup_to(&dir.join("team.db")).unwrap();
    let run = teamwire(&home, &[], &["metrics"]);
    assert_eq!(run.code, 0, "{}", run.err);
    for team in ["alpha", "beta", "gamma"] {
        assert!(
            run.out
                .contains(&format!("teamwire.watch.lines{{team={team}}} 2")),
            "{team}: {}",
            run.out
        );
    }
}

#[test]
fn since_leaves_out_older_events() {
    let home = TestHome::new().unwrap();
    // The events are years in the past, so a short window holds none.
    busy_team(&home, "alpha");
    let run = teamwire(&home, &[], &["metrics", "alpha", "--since", "1h"]);
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(!run.out.contains("teamwire.watch.lines"), "{}", run.out);
}

#[test]
fn an_unknown_team_is_refused_and_a_bad_duration_is_a_usage_error() {
    let home = TestHome::new().unwrap();
    let unknown = teamwire(&home, &[], &["metrics", "nope"]);
    assert_eq!(unknown.code, 1);
    assert!(unknown.err.contains("nope"), "{}", unknown.err);
    busy_team(&home, "alpha");
    let bad = teamwire(&home, &[], &["metrics", "alpha", "--since", "soon"]);
    assert_eq!(bad.code, 3);
}

#[test]
fn export_sends_new_events_once_and_moves_the_mark() {
    let home = TestHome::new().unwrap();
    busy_team(&home, "alpha");
    let rx = receiver(200);
    let envs = [
        ("OTEL_EXPORTER_OTLP_ENDPOINT", rx.url()),
        (
            "OTEL_EXPORTER_OTLP_HEADERS",
            "authorization=Bearer sk-cli-secret".to_owned(),
        ),
    ];
    let envs: Vec<(&str, &str)> = envs.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let run = teamwire(&home, &envs, &["metrics", "export", "--once"]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert!(run.out.contains("alpha: sent events 1 to 5"), "{}", run.out);
    assert!(rx.paths().contains(&"/v1/traces".to_owned()));
    assert!(rx.paths().contains(&"/v1/metrics".to_owned()));
    assert!(
        rx.heads()
            .iter()
            .all(|h| h.contains("authorization: bearer sk-cli-secret"))
    );
    assert!(!run.out.contains("sk-cli-secret") && !run.err.contains("sk-cli-secret"));
    assert_eq!(mark(&home, "alpha"), 5);
    // Nothing new the second time.
    let before = rx.paths().len();
    let again = teamwire(&home, &envs, &["metrics", "export"]);
    assert_eq!(again.code, 0);
    assert!(
        again.out.contains("alpha: nothing new to send"),
        "{}",
        again.out
    );
    assert_eq!(rx.paths().len(), before);
}

#[test]
fn a_collector_that_refuses_does_not_change_the_exit_code_or_the_mark() {
    let home = TestHome::new().unwrap();
    busy_team(&home, "alpha");
    let rx = receiver(500);
    let url = rx.url();
    let run = teamwire(
        &home,
        &[("OTEL_EXPORTER_OTLP_ENDPOINT", &url)],
        &["metrics", "export", "--once"],
    );
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert!(run.err.contains("export failed, will retry"), "{}", run.err);
    assert_eq!(mark(&home, "alpha"), 0);
}

#[test]
fn the_endpoint_can_come_from_the_config_file() {
    let home = TestHome::new().unwrap();
    busy_team(&home, "alpha");
    let rx = receiver(200);
    std::fs::write(
        home.path().join("config.toml"),
        format!("OTEL_EXPORTER_OTLP_ENDPOINT = \"{}\"\n", rx.url()),
    )
    .unwrap();
    let run = teamwire(&home, &[], &["metrics", "export", "--once"]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert!(
        rx.paths().contains(&"/v1/metrics".to_owned()),
        "{}{}",
        run.out,
        run.err
    );
}

#[test]
fn export_is_off_without_an_endpoint() {
    let home = TestHome::new().unwrap();
    busy_team(&home, "alpha");
    let run = teamwire(&home, &[], &["metrics", "export", "--once"]);
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(run.err.contains("export is off"), "{}", run.err);
    assert_eq!(mark(&home, "alpha"), 0);
}

#[test]
fn export_writes_every_record_as_json_lines_with_cloudevents_names() {
    let home = TestHome::new().unwrap();
    let mut s = busy_team(&home, "alpha");
    s.write(|tx| {
        tx.insert_member(
            at(1),
            &NewMember {
                role: Role::parse("api").unwrap(),
                title: "the api".to_owned(),
                kind: MemberKind::Implementer,
                mode: Mode::Pane,
                harness: HarnessId::from_static("claude-code"),
                host: HostId::from_static("herdr"),
                cwd: "/work".into(),
                repo: None,
                branch: None,
                base: None,
                base_sha: None,
                check: None,
                owns: vec!["src/".to_owned()],
                deadline_s: 1800,
            },
        )?;
        tx.insert_message(
            at(2),
            &NewMessage {
                sender: Actor::Member(Role::parse("api").unwrap()),
                recipients: vec![Recipient::Lead],
                kind: MessageType::Note,
                body: "halfway".to_owned(),
                commit: None,
                re: None,
                client_key: None,
            },
        )?;
        tx.announce_once(at(20), "silent:api")?;
        tx.insert_waiver(
            at(21),
            &Waiver {
                role: Role::parse("api").unwrap(),
                flag_id: "tests-deleted".to_owned(),
                head_sha: "abc1234".to_owned(),
                reason: "moved to another crate".to_owned(),
                created_at: at(21),
            },
        )?;
        Ok(())
    })
    .unwrap();
    let run = teamwire(&home, &[], &["export", "alpha"]);
    assert_eq!(run.code, 0, "{}", run.err);
    let records: Vec<Value> = run
        .out
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    for r in &records {
        for key in ["specversion", "id", "source", "type", "time", "data"] {
            assert!(r.get(key).is_some(), "{key} missing in {r}");
        }
        assert_eq!(r["source"], "teamwire/alpha");
    }
    let kinds: Vec<&str> = records
        .iter()
        .map(|r| r["type"].as_str().unwrap())
        .collect();
    for expected in [
        "teamwire.team.created",
        "teamwire.message.sent",
        "teamwire.message",
        "teamwire.announcement",
        "teamwire.waiver",
        "teamwire.member",
    ] {
        assert!(kinds.contains(&expected), "{expected} not in {kinds:?}");
    }
    let waiver = records
        .iter()
        .find(|r| r["type"] == "teamwire.waiver")
        .unwrap();
    assert_eq!(waiver["data"]["reason"], "moved to another crate");
    // `--out` writes the same lines to a file.
    let file = home.path().join("alpha.jsonl");
    let out = teamwire(
        &home,
        &[],
        &["export", "alpha", "--out", file.to_str().unwrap()],
    );
    assert_eq!(out.code, 0, "{}", out.err);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), run.out);
}

#[test]
fn export_of_an_unknown_team_is_refused() {
    let home = TestHome::new().unwrap();
    let run = teamwire(&home, &[], &["export", "nope"]);
    assert_eq!(run.code, 1);
}
