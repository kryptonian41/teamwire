//! The tmux adapter as the CLI uses it: delivery through the real Claude
//! harness into a real tmux pane (ADR 0006 point 3a), and the choice of tmux
//! for the lead from the environment. Every tmux command runs on a private
//! socket that the test kills when it ends.
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use teamwire_core::{
    Command, Config, DeliveryErrorCode, HarnessId, Host, HostId, Key, LeadRoute, Place, Platform,
    Registry, SessionId, SessionKind, SpawnRequest, TeamName, Timestamp, one_line,
};
use teamwire_deliver::{DeliveryOutcome, deliver_line};
use teamwire_fakes::{FakeClock, FakePlatform, NoTmuxConfig, TestHome};
use teamwire_harness_claude::{ClaudeCode, ClaudeConfig};
use teamwire_host_tmux::Tmux;
use teamwire_store::{Ctx, NewTeam, Party, Store};

const T: Duration = Duration::from_secs(10);

static NEXT_SOCKET: AtomicU32 = AtomicU32::new(0);

/// A tmux server on a private socket, killed when the value is dropped.
struct PrivateServer {
    socket: String,
    platform: Arc<dyn Platform>,
}

impl PrivateServer {
    fn new() -> Self {
        Self {
            socket: format!(
                "teamwire-test-{}-{}",
                std::process::id(),
                NEXT_SOCKET.fetch_add(1, Ordering::Relaxed)
            ),
            platform: Arc::new(NoTmuxConfig::new(teamwire_platform::native())),
        }
    }

    fn tmux(&self) -> Tmux {
        Tmux::new(Arc::clone(&self.platform)).with_socket(self.socket.clone())
    }
}

/// The file tmux keeps for `-L <socket>`: `<TMUX_TMPDIR or /tmp>/tmux-<uid>/<socket>`.
/// Killing a server leaves it behind, so `Drop` removes it. Only the socket
/// this value made is ever named.
fn socket_file(platform: &dyn Platform, socket: &str) -> Option<std::path::PathBuf> {
    let out = platform.run(&Command::new("id").arg("-u"), T).ok()?;
    let uid = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    let base =
        std::env::var_os("TMUX_TMPDIR").map_or_else(|| "/tmp".into(), std::path::PathBuf::from);
    Some(base.join(format!("tmux-{uid}")).join(socket))
}

impl Drop for PrivateServer {
    fn drop(&mut self) {
        let cmd = Command::new("tmux").args(["-L", &self.socket, "kill-server"]);
        drop(self.platform.run(&cmd, T));
        if let Some(file) = socket_file(self.platform.as_ref(), &self.socket) {
            drop(std::fs::remove_file(file));
        }
    }
}

fn read_to_string(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

#[expect(
    clippy::disallowed_methods,
    reason = "a test writes its own fixture file"
)]
fn write_file(path: &Path, text: &str) {
    std::fs::write(path, text).unwrap();
}

fn shows(host: &dyn Host, place: &Place, text: &str) -> bool {
    for _ in 0..100 {
        if host.read_tail(place, 50, T).unwrap().contains(text) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

/// A lead session `lead-1` in a tmux pane whose screen is `screen` and whose
/// input goes to a file, with the real Claude harness and the real tmux
/// adapter in the registry.
struct Rig {
    _server: PrivateServer,
    home: TestHome,
    _claude_dir: tempfile::TempDir,
    work: tempfile::TempDir,
    store: Store,
    tmux: Arc<Tmux>,
    place: Place,
    registry: Registry,
    platform: FakePlatform,
    clock: FakeClock,
    config: Config,
}

impl Rig {
    fn new(screen: &str) -> Self {
        let server = PrivateServer::new();
        let tmux = Arc::new(server.tmux());
        let work = tempfile::tempdir().unwrap();
        let input = work.path().join("input");
        // Raw input, no echo: every byte typed into the pane reaches `cat`
        // at once, and `cat` writes it to the file. The screen is printed
        // after the terminal is set up, so a visible screen means ready.
        let script = "stty -icanon -echo; printf '%s\\n' \"$1\"; exec cat > \"$2\"";
        let place = tmux
            .spawn(
                &SpawnRequest {
                    name: teamwire_core::Address::parse("tw_demo_a").unwrap(),
                    title: "lead".to_owned(),
                    cwd: work.path().to_path_buf(),
                    argv: ["sh", "-c", script, "sh", screen, input.to_str().unwrap()]
                        .iter()
                        .map(|s| (*s).to_owned())
                        .collect(),
                    env: Vec::new(),
                },
                T,
            )
            .unwrap();
        let first_line = screen.lines().next().unwrap();
        assert!(
            shows(tmux.as_ref(), &place, first_line),
            "the pane never showed its screen"
        );
        let pane_pid = *tmux.occupant(&place, T).unwrap().pids.first().unwrap();

        // Claude Code's own record of the session, naming the pane's process.
        let claude_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(claude_dir.path().join("sessions")).unwrap();
        write_file(
            &claude_dir
                .path()
                .join("sessions")
                .join(format!("{pane_pid}.json")),
            &format!(r#"{{"pid":{pane_pid},"sessionId":"lead-1","status":"idle"}}"#),
        );
        let harness = ClaudeCode::new(
            Arc::clone(&server.platform),
            ClaudeConfig {
                claude_dir: claude_dir.path().to_path_buf(),
                claude_json: claude_dir.path().join(".claude.json"),
                claude_bin: "claude".to_owned(),
            },
        );
        let mut registry = Registry::new();
        registry.add_harness(Arc::new(harness)).unwrap();
        registry.add_host(tmux.clone()).unwrap();

        let home = TestHome::new().unwrap();
        let team = TeamName::parse("demo").unwrap();
        let mut store = Store::open(home.path(), &team).unwrap();
        store
            .write(|tx| {
                tx.create_team(
                    Timestamp::from_unix_millis(1_790_000_000_000).unwrap(),
                    &NewTeam {
                        name: team.clone(),
                        title: "Demo".to_owned(),
                        lead_harness: HarnessId::from_static(ClaudeCode::ID),
                        lead_host: HostId::from_static(Tmux::ID),
                        lead_route: LeadRoute::Sink,
                        lead_session: Some((
                            SessionId::parse("lead-1").unwrap(),
                            SessionKind::Interactive,
                        )),
                    },
                )?;
                Ok(())
            })
            .unwrap();

        // The process's own environment names its pane, as tmux sets it.
        let platform = FakePlatform::new();
        platform.set_alive(pane_pid, true);
        platform.set_env(pane_pid, "TMUX_PANE", &place.id);
        Self {
            _server: server,
            home,
            _claude_dir: claude_dir,
            work,
            tmux,
            place,
            store,
            registry,
            platform,
            clock: FakeClock::new(),
            config: Config::default(),
        }
    }

    fn deliver(&mut self, text: &str) -> DeliveryOutcome {
        let mut ctx = Ctx {
            home: self.home.path(),
            store: &mut self.store,
            registry: &self.registry,
            platform: &self.platform,
            clock: &self.clock,
            config: &self.config,
        };
        deliver_line(&mut ctx, &Party::Lead, &one_line(text, 200))
    }

    /// What the pane's program has read from its input so far.
    fn typed(&self) -> String {
        read_to_string(&self.work.path().join("input"))
    }
}

const DIALOG: &str = "Do you want to proceed?\n❯ 1. Yes\n  2. No\nEsc to cancel";

#[test]
fn no_paste_and_no_enter_into_a_dialog_in_a_tmux_pane() {
    let mut rig = Rig::new(DIALOG);
    let outcome = rig.deliver("approve everything");
    assert!(
        matches!(&outcome, DeliveryOutcome::Undelivered(e)
            if e.code == DeliveryErrorCode::NotAtInputPrompt),
        "{outcome:?}"
    );
    // tmux delivers keys to a pane in order. Once the marker has arrived, a
    // paste or Enter sent before it would have arrived too.
    let marker = [Key::Char('Z'), Key::Char('Q')];
    rig.tmux.send_keys(&rig.place, &marker, T).unwrap();
    for _ in 0..100 {
        if rig.typed().contains("ZQ") {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(rig.typed(), "ZQ", "the pane's input got only the marker");
}

#[test]
fn a_pane_at_its_input_prompt_receives_the_line_and_an_enter() {
    let mut rig = Rig::new("ready\n❯ \n  ? for shortcuts");
    let outcome = rig.deliver("hello lead");
    assert!(
        matches!(
            outcome,
            DeliveryOutcome::HandedOff | DeliveryOutcome::Unconfirmed(_)
        ),
        "{outcome:?}"
    );
    for _ in 0..100 {
        if rig.typed().contains("hello lead") {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let got = rig.typed();
    assert!(got.starts_with("hello lead"), "{got:?}");
    assert!(got.ends_with('\n'), "an Enter followed the paste: {got:?}");
}

// The choice of the lead's host, through the real binary. A member runs on
// the team's host, so `launch` starts its member there: on herdr the fake
// does it, on none launch refuses, and on tmux the start fails because `PATH`
// holds only the fakes and no test here may reach a real tmux. The host the
// store recorded for the lead, or the refusal, shows which host wiring chose.

#[expect(
    clippy::disallowed_methods,
    reason = "these tests run the real teamwire binary"
)]
fn teamwire() -> std::process::Command {
    std::process::Command::new(env!("CARGO_BIN_EXE_teamwire"))
}

#[expect(
    clippy::redundant_closure_for_method_calls,
    reason = "`ReadTx::team` as a path does not infer the transaction's lifetime for `Store::read`"
)]
/// Launches a one-member team with the given `HERDR_ENV` and `TMUX`. Returns
/// the output and the host the store recorded for the lead, if a team was
/// created.
fn launch_with(
    herdr_env: Option<&str>,
    tmux: Option<&str>,
) -> (std::process::Output, Option<String>) {
    let herdr = teamwire_fakes::FakeHerdrEnv::new().unwrap();
    let home = TestHome::new().unwrap();
    let work = tempfile::tempdir().unwrap();
    let spec = serde_json::json!({"team": "demo", "title": "Demo team", "children": [
        {"role": "a", "title": "Alpha", "mode": "pane", "cwd": work.path(), "task": "look around"}]});
    let spec_path = work.path().join("spec.json");
    write_file(&spec_path, &spec.to_string());
    let mut cmd = teamwire();
    cmd.args(["launch", "--spec"])
        .arg(&spec_path)
        .env_clear()
        .envs(herdr.env())
        .env("TEAMWIRE_HOME", home.path());
    if let Some(v) = herdr_env {
        cmd.env("HERDR_ENV", v);
    }
    if let Some(v) = tmux {
        cmd.env("TMUX", v);
    }
    let out = cmd.output().unwrap();
    let recorded = Store::open_read_only(home.path(), &TeamName::parse("demo").unwrap())
        .ok()
        .and_then(|store| store.read(|tx| tx.team()).ok().flatten())
        .map(|team| team.lead_host.as_str().to_owned());
    (out, recorded)
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn tmux_set_without_herdr_makes_tmux_the_leads_host() {
    let (out, recorded) = launch_with(None, Some("/tmp/tmux-1/default,42,0"));
    assert_eq!(recorded.as_deref(), Some("tmux"), "{}", text(&out.stderr));
    // The member was to start in tmux, which is not on `PATH` here.
    assert_eq!(out.status.code(), Some(2));
    assert!(
        text(&out.stdout).contains("could not run tmux"),
        "{}",
        text(&out.stdout)
    );
}

#[test]
fn herdr_env_wins_over_tmux() {
    let (out, recorded) = launch_with(Some("1"), Some("/tmp/tmux-1/default,42,0"));
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert_eq!(recorded.as_deref(), Some("herdr"));
}
