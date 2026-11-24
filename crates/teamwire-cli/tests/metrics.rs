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
use teamwire_store::{NewMember, NewMessage, NewTeam, Store, Waiver, database_path};

#[path = "support/schema_1.rs"]
mod schema_1;

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
        .env_remove("NO_COLOR")
        .env_remove("TEAMWIRE_LOG")
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
    std::fs::write(dir.join("SHA256SUMS"), b"").unwrap();
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
fn a_closed_team_is_read_from_its_archive_by_the_name_in_its_team_row() {
    let home = TestHome::new().unwrap();
    let other = TestHome::new().unwrap();
    let alpha = busy_team(&other, "alpha");
    // The folder name does not start with the team name: only the row can say.
    let dir = home.path().join("archive").join("zz-2026-09-30");
    std::fs::create_dir_all(&dir).unwrap();
    alpha.backup_to(&dir.join("team.db")).unwrap();
    std::fs::write(dir.join("SHA256SUMS"), b"").unwrap();
    let run = teamwire(&home, &[], &["metrics", "alpha"]);
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(
        run.out.contains("teamwire.watch.lines{team=alpha} 2"),
        "{}",
        run.out
    );
    let never = teamwire(&home, &[], &["metrics", "zz"]);
    assert_eq!(never.code, 1);
    assert!(
        never.err.contains("no database for team zz"),
        "{}",
        never.err
    );
}

/// Launches a one-member pane team on the fakes, waits for its first turn to
/// end (`close` refuses a busy member), then closes it with the archive on.
/// The archive is the one a real close writes, so a change to close's layout
/// fails here.
#[test]
fn a_team_closed_for_real_is_read_from_the_archive_close_wrote() {
    let fakes = teamwire_fakes::FakeHerdrEnv::new().unwrap();
    let home = TestHome::new().unwrap();
    let work = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(work.path())
            .args(args)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}");
    };
    git(&["init", "-q", "-b", "main"]);
    git(&["commit", "-q", "--allow-empty", "-m", "first"]);
    let spec = json!({"team": "alpha", "title": "Alpha", "children": [
        {"role": "scout", "title": "Scout", "mode": "pane", "cwd": work.path(), "task": "look around"}]});
    let real = |args: &[&str], stdin: &str| {
        let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_teamwire"))
            .args(args)
            .env_clear()
            .envs(fakes.env())
            .env("TEAMWIRE_HOME", home.path())
            .env("HERDR_ENV", "1")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert_eq!(
            out.status.code(),
            Some(0),
            "{args:?}: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    };
    real(&["launch"], &spec.to_string());
    let alpha = TeamName::parse("alpha").unwrap();
    let members = Store::open_read_only(home.path(), &alpha)
        .unwrap()
        .read(|tx| Ok(tx.members()?.items))
        .unwrap();
    for member in members {
        let Some(place) = member.place else { continue };
        let file = fakes
            .root()
            .join("fake-herdr-panes")
            .join(place.id.replace(':', "_"))
            .join("status.json");
        // 30 seconds, counted in polls so the test needs no clock.
        let ended = (0..1500).any(|_| {
            let text = std::fs::read_to_string(&file).unwrap_or_default();
            let done = serde_json::from_str::<Value>(&text)
                .is_ok_and(|s| s.get("status").is_some_and(|v| v == "done"));
            if !done {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            done
        });
        assert!(ended, "the first turn did not end");
    }
    real(&["close", "alpha"], "");
    assert!(!database_path(home.path(), &alpha).exists());
    let run = teamwire(&home, &[], &["metrics", "alpha"]);
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(run.out.contains("team=alpha"), "{}", run.out);
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
    let again = teamwire(&home, &envs, &["metrics", "export", "--once"]);
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

/// Credentials in the environment were meant for an endpoint in the
/// environment: an endpoint from the file never gets them, and the run says
/// once that they were left out.
#[test]
fn headers_from_the_environment_never_go_to_an_endpoint_from_the_file() {
    let home = TestHome::new().unwrap();
    busy_team(&home, "alpha");
    let rx = receiver(200);
    std::fs::write(
        home.path().join("config.toml"),
        format!("OTEL_EXPORTER_OTLP_ENDPOINT = \"{}\"\n", rx.url()),
    )
    .unwrap();
    let run = teamwire(
        &home,
        &[("OTEL_EXPORTER_OTLP_HEADERS", "authorization=env-secret")],
        &["metrics", "export", "--once"],
    );
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert!(!rx.heads().is_empty(), "{}{}", run.out, run.err);
    assert!(
        rx.heads().iter().all(|h| !h.contains("env-secret")),
        "{:?}",
        rx.heads()
    );
    assert_eq!(
        run.err.matches("OTEL_EXPORTER_OTLP_HEADERS").count(),
        1,
        "{}",
        run.err
    );
    assert!(!run.err.contains("env-secret"), "{}", run.err);
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
fn metrics_for_a_team_whose_store_cannot_be_read_exits_6() {
    let home = TestHome::new().unwrap();
    let mut s = busy_team(&home, "alpha");
    s.write(|tx| {
        tx.connection().execute_batch("PRAGMA user_version = 99")?;
        Ok(())
    })
    .unwrap();
    drop(s);
    let run = teamwire(&home, &[], &["metrics", "alpha"]);
    assert_eq!(run.code, 6, "{}{}", run.out, run.err);
    assert!(!run.out.contains("no metrics"), "{}", run.out);
    assert!(run.err.contains("teamwire metrics:"), "{}", run.err);
    // Listing every team still reports the damaged one and exits 0.
    let all = teamwire(&home, &[], &["metrics"]);
    assert_eq!(all.code, 0, "{}{}", all.out, all.err);
    assert!(all.err.contains("teamwire metrics:"), "{}", all.err);
}

#[test]
fn metrics_for_a_closed_team_whose_archive_cannot_be_read_exits_6() {
    let home = TestHome::new().unwrap();
    let other = TestHome::new().unwrap();
    let mut s = busy_team(&other, "alpha");
    s.write(|tx| {
        tx.connection().execute_batch("PRAGMA user_version = 99")?;
        Ok(())
    })
    .unwrap();
    drop(s);
    let from = database_path(other.path(), &TeamName::parse("alpha").unwrap());
    let dir = home.path().join("archive").join("alpha-2026-09-30");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(&from, dir.join("team.db")).unwrap();
    std::fs::write(dir.join("SHA256SUMS"), b"").unwrap();
    let run = teamwire(&home, &[], &["metrics", "alpha"]);
    assert_eq!(run.code, 6, "{}{}", run.out, run.err);
    assert!(!run.err.contains("no database"), "{}", run.err);
    assert!(run.err.contains("alpha-2026-09-30"), "{}", run.err);
    // Another team's unreadable archive is not this one's.
    let run = teamwire(&home, &[], &["metrics", "alph"]);
    assert_eq!(run.code, 1, "{}{}", run.out, run.err);
    assert!(run.err.contains("no database for team alph"), "{}", run.err);
}

#[test]
fn export_of_an_unknown_team_is_refused() {
    let home = TestHome::new().unwrap();
    let run = teamwire(&home, &[], &["export", "nope"]);
    assert_eq!(run.code, 1);
}

#[test]
fn export_skips_a_damaged_row_names_it_and_still_exits_zero() {
    let home = TestHome::new().unwrap();
    let mut s = busy_team(&home, "alpha");
    // One event whose time is not a time, as a damaged file might hold.
    s.write(|tx| {
        tx.connection().execute(
            "UPDATE events SET time = 'yesterday' WHERE type = 'watch.line' AND seq = 3",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    let run = teamwire(&home, &[], &["export", "alpha"]);
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(run.err.contains("skipped events row 3"), "{}", run.err);
    // stderr is a pipe here, so the log lines carry no colour codes.
    assert!(!run.err.contains('\x1b'), "{:?}", run.err);
    let ours = run
        .err
        .lines()
        .filter(|l| l.starts_with("teamwire export: skipped"))
        .count();
    assert_eq!(ours, 1, "{}", run.err);
    let records: Vec<Value> = run
        .out
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert!(records.iter().any(|r| r["type"] == "teamwire.team.created"));
    assert!(records.iter().any(|r| r["type"] == "teamwire.message.sent"));
    assert!(
        records.iter().all(|r| r["seq"] != 3),
        "the damaged event is not written"
    );
}

#[test]
fn export_without_once_keeps_running_after_its_first_pass() {
    let home = TestHome::new().unwrap();
    busy_team(&home, "alpha");
    let rx = receiver(200);
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_teamwire"))
        .arg("--home")
        .arg(home.path())
        .args(["metrics", "export"])
        .env("TEAMWIRE_HOME", home.path())
        .env("OTEL_EXPORTER_OTLP_ENDPOINT", rx.url())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !rx.paths().contains(&"/v1/metrics".to_owned()) {
        assert!(std::time::Instant::now() < deadline, "no export arrived");
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    std::thread::sleep(std::time::Duration::from_millis(500));
    let still_running = child.try_wait().unwrap().is_none();
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(still_running, "export without --once exited after one pass");
}

fn user_version(path: &std::path::Path) -> u32 {
    let store = Store::open_read_only_at(path).unwrap();
    store.schema_version().unwrap()
}

/// A team at schema 1 that no read-write open has migrated, and an archive
/// at schema 1, which nothing ever migrates: metrics, status and export read
/// them, and the files keep every byte and their schema version (WAT-11).
#[test]
fn metrics_status_and_export_read_a_schema_1_team_and_archive_and_change_neither() {
    let home = TestHome::new().unwrap();
    let live = schema_1::version_1_team(home.path(), "alpha", 1).unwrap();
    let other = TestHome::new().unwrap();
    let archived_from = schema_1::version_1_team(other.path(), "beta", 2).unwrap();
    let dir = home.path().join("archive").join("beta-2026-09-30");
    std::fs::create_dir_all(&dir).unwrap();
    let archived = dir.join("team.db");
    std::fs::copy(&archived_from, &archived).unwrap();
    std::fs::write(dir.join("SHA256SUMS"), b"").unwrap();
    let files = |paths: &[&std::path::Path]| -> Vec<(String, Vec<u8>)> {
        let mut out = Vec::new();
        for p in paths {
            for e in std::fs::read_dir(p.parent().unwrap()).unwrap() {
                let e = e.unwrap();
                out.push((
                    e.path().display().to_string(),
                    std::fs::read(e.path()).unwrap(),
                ));
            }
        }
        out.sort();
        out
    };
    let before = files(&[&live, &archived]);
    assert_eq!((user_version(&live), user_version(&archived)), (1, 1));

    let all = teamwire(&home, &[], &["metrics"]);
    assert_eq!(all.code, 0, "{}{}", all.out, all.err);
    for team in ["alpha", "beta"] {
        assert!(
            all.out
                .contains(&format!("teamwire.watch.lines{{team={team}}} 2")),
            "{team}: {}{}",
            all.out,
            all.err
        );
    }
    assert!(!all.err.contains("schema"), "{}", all.err);
    let one = teamwire(&home, &[], &["metrics", "alpha"]);
    assert_eq!(one.code, 0, "{}{}", one.out, one.err);
    let status = teamwire(&home, &[], &["status", "alpha"]);
    assert_eq!(status.code, 0, "{}{}", status.out, status.err);
    let export = teamwire(&home, &[], &["export", "alpha"]);
    assert_eq!(export.code, 0, "{}{}", export.out, export.err);
    assert_eq!(
        export
            .out
            .lines()
            .filter(|l| l.contains("watch.line"))
            .count(),
        2,
        "{}",
        export.out
    );

    assert_eq!(files(&[&live, &archived]), before, "a read changed a file");
    assert_eq!((user_version(&live), user_version(&archived)), (1, 1));
}

/// A second Enter that tmux delivery pressed is a durable event. The export
/// lists it with its role and place, and the metrics export sends the team
/// on its way without a metric for it, as for `delivery.resent`.
#[test]
fn a_delivery_reentered_event_is_exported_and_does_not_stop_the_metrics_export() {
    let home = TestHome::new().unwrap();
    let mut s = busy_team(&home, "alpha");
    s.write(|tx| {
        tx.record_event(
            at(12),
            EventType::DeliveryReentered,
            Some(&Role::parse("api").unwrap()),
            None,
            &json!({"place": "%3"}),
        )
        .map(|_| ())
    })
    .unwrap();

    let run = teamwire(&home, &[], &["export", "alpha"]);
    assert_eq!(run.code, 0, "{}", run.err);
    let record: Value = run
        .out
        .lines()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .find(|r| r["type"] == "teamwire.delivery.reentered")
        .unwrap();
    assert_eq!(record["role"], "api");
    assert_eq!(record["data"]["place"], "%3");

    let rx = receiver(200);
    let url = rx.url();
    let sent = teamwire(
        &home,
        &[("OTEL_EXPORTER_OTLP_ENDPOINT", url.as_str())],
        &["metrics", "export", "--once"],
    );
    assert_eq!(sent.code, 0, "{}{}", sent.out, sent.err);
    assert!(
        sent.out.contains("alpha: sent events 1 to 6"),
        "{}",
        sent.out
    );
    assert_eq!(mark(&home, "alpha"), 6);
}

#[test]
fn an_export_that_fails_in_the_store_leaves_one_store_error_row() {
    let home = TestHome::new().unwrap();
    let mut store = busy_team(&home, "alpha");
    // A function that does not exist fails the claim's update with an `io`
    // error, before anything is sent.
    store
        .write(|tx| {
            tx.connection()
                .execute_batch(
                    "CREATE TRIGGER no_claim BEFORE UPDATE ON metric_export
                     BEGIN SELECT no_such_function(); END",
                )
                .map_err(teamwire_store::StoreError::from)
        })
        .unwrap();
    let rx = receiver(200);
    let run = teamwire(
        &home,
        &[("OTEL_EXPORTER_OTLP_ENDPOINT", &rx.url())],
        &["metrics", "export", "--once"],
    );
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert!(run.err.contains("export failed, will retry"), "{}", run.err);
    let errors: Vec<Value> = store
        .read(|tx| tx.events_after(0, None, None)?.all_or_corrupt())
        .unwrap()
        .into_iter()
        .filter(|e| e.kind == EventType::StoreError)
        .map(|e| e.data)
        .collect();
    assert_eq!(errors, [json!({ "op": "metrics", "code": "io" })]);
}

/// Tokens are counted per member whatever its harness: a pi member's and a
/// fake harness member's delivered results carry the reading their harness
/// read from its own session file.
#[test]
fn metrics_print_tokens_for_pi_and_fake_harness_members() {
    let home = TestHome::new().unwrap();
    let mut s = open(&home, "alpha");
    for (role, harness, model) in [("pi", "pi", "claude-opus-5-5"), ("fk", "fake", "gpt-5.5")] {
        let role = Role::parse(role).unwrap();
        s.write(|tx| {
            tx.insert_member(
                at(1),
                &NewMember {
                    role: role.clone(),
                    title: format!("the {role}"),
                    kind: MemberKind::Implementer,
                    mode: Mode::Pane,
                    harness: HarnessId::parse(harness).unwrap(),
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
            tx.record_event(
                at(5),
                EventType::ResultDelivering,
                Some(&role),
                None,
                &json!({"session": "s1", "usage": {model: {"input": 2000, "output": 400, "cache_read": 3000, "cache_creation": 0}}}),
            )
            .map(|_| ())
        })
        .unwrap();
    }
    let run = teamwire(&home, &[], &["metrics", "alpha"]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    for (role, model) in [("pi", "claude-opus-5-5"), ("fk", "gpt-5.5")] {
        let lines: Vec<&str> = run
            .out
            .lines()
            .filter(|l| {
                l.starts_with("teamwire.member.tokens{")
                    && l.contains(&format!("role={role}"))
                    && l.contains(&format!("model={model}"))
            })
            .collect();
        assert!(!lines.is_empty(), "{role}: {}", run.out);
    }
}

/// A pi member's harness records the money of each reply, so its delivered
/// result gives one money line per model, in dollars to 4 places. A Claude
/// Code member records no money and gets no money line, never a line of 0.
#[test]
fn metrics_print_money_for_a_pi_member_and_none_for_a_claude_code_member() {
    let home = TestHome::new().unwrap();
    let mut s = open(&home, "alpha");
    for (role, harness, model, cost) in [
        ("readb", "pi", "openai/gpt-6.2-astra", Some(1_587_800)),
        ("readc", "pi", "google/gemini-4-argon", Some(400)),
        ("reada", "claude-code", "claude-opus-5-5", None),
    ] {
        let role = Role::parse(role).unwrap();
        let mut counts =
            json!({"input": 2000, "output": 400, "cache_read": 3000, "cache_creation": 0});
        if let Some(cost) = cost {
            counts["cost_micro_usd"] = json!(cost);
        }
        s.write(|tx| {
            tx.insert_member(
                at(1),
                &NewMember {
                    role: role.clone(),
                    title: format!("the {role}"),
                    kind: MemberKind::Reviewer,
                    mode: Mode::Pane,
                    harness: HarnessId::parse(harness).unwrap(),
                    host: HostId::from_static("herdr"),
                    cwd: "/work".into(),
                    repo: None,
                    branch: None,
                    base: None,
                    base_sha: None,
                    check: None,
                    owns: Vec::new(),
                    deadline_s: 1800,
                },
            )?;
            tx.record_event(
                at(2),
                EventType::MemberCreating,
                Some(&role),
                None,
                &json!({"kind": "reviewer", "task_class": "judgement"}),
            )?;
            tx.record_event(
                at(3),
                EventType::MemberStarted,
                Some(&role),
                None,
                &json!({"effort": "high"}),
            )?;
            tx.record_event(
                at(5),
                EventType::ResultDelivering,
                Some(&role),
                None,
                &json!({"session": "s1", "usage": {model: counts}}),
            )
            .map(|_| ())
        })
        .unwrap();
    }
    let run = teamwire(&home, &[], &["metrics", "alpha"]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    let money: Vec<&str> = run
        .out
        .lines()
        .filter(|l| l.starts_with("teamwire.member.cost{"))
        .collect();
    assert_eq!(
        money,
        [
            "teamwire.member.cost{team=alpha,role=readb,kind=reviewer,model=openai/gpt-6.2-astra,effort=high,task_class=judgement} 1.5878 usd",
            "teamwire.member.cost{team=alpha,role=readc,kind=reviewer,model=google/gemini-4-argon,effort=high,task_class=judgement} 0.0004 usd",
        ],
        "{}",
        run.out
    );
    // The Claude Code member's tokens still print; only money is missing.
    assert!(
        run.out
            .lines()
            .any(|l| l.starts_with("teamwire.member.tokens{") && l.contains("role=reada")),
        "{}",
        run.out
    );
}
