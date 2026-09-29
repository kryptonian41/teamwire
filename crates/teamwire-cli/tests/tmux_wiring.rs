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
    Command, Config, DeliveryErrorCode, HarnessId, Host, HostId, LeadRoute, Place, Platform,
    Registry, SessionId, SessionKind, SpawnRequest, TeamName, Timestamp, one_line,
};
use teamwire_deliver::{DeliveryOutcome, deliver_line};
use teamwire_fakes::{FakeClock, FakePlatform, TestHome};
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
            platform: teamwire_platform::native(),
        }
    }

    fn tmux(&self) -> Tmux {
        Tmux::new(Arc::clone(&self.platform)).with_socket(self.socket.clone())
    }
}

impl Drop for PrivateServer {
    fn drop(&mut self) {
        let cmd = Command::new("tmux").args(["-L", &self.socket, "kill-server"]);
        drop(self.platform.run(&cmd, T));
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
        registry.add_host(tmux).unwrap();

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
            store,
            registry,
            platform,
            clock: FakeClock::new(),
            config: Config::default(),
        }
    }

    fn deliver(&mut self, text: &str) -> DeliveryOutcome {
        let ctx = Ctx {
            home: self.home.path(),
            store: &mut self.store,
            registry: &self.registry,
            platform: &self.platform,
            clock: &self.clock,
            config: &self.config,
        };
        deliver_line(&ctx, &Party::Lead, &one_line(text, 200))
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
    // Give a wrongly sent paste and Enter time to reach the pane's input.
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(rig.typed(), "", "the pane's input got nothing");
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
