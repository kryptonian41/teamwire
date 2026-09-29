//! The shared kill harness: a team in a temporary home, live fake members in
//! fake herdr panes, the real `teamwire` binary run as a child process, and a
//! `SIGKILL` at a random moment. Randomness comes from one seed that is
//! printed, so a failing run replays with `TEAMWIRE_CRASH_SEED=<seed>`.
#![expect(
    clippy::disallowed_methods,
    reason = "the crash tests start the real teamwire binary and kill it"
)]
#![expect(
    clippy::print_stderr,
    reason = "the seed and the kill window are printed so a failing run can be replayed"
)]
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "helpers in an integration-test support file unwrap; a failure there fails the test"
)]

use std::io::{Read as _, Seek as _, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use teamwire_core::{
    Actor, HarnessId, HostId, LeadRoute, MemberKind, MemberState, Mode, Place, Recipient, Role,
    SessionId, SessionKind, TeamName, Timestamp,
};
use teamwire_fakes::{ClaudeScript, FakeHerdrEnv, TestHome};
use teamwire_store::{NewMember, NewTeam, Store};

pub const TEAM: &str = "alpha";
pub const LEAD_SESSION: &str = "lead-1";

pub fn at() -> Timestamp {
    Timestamp::from_unix_millis(1_790_000_000_000).unwrap()
}

pub fn token_of(role: &str) -> String {
    format!("token-of-{role}")
}

/// SHA-256 of `text`, by the system `shasum`: this crate lists no hash
/// library, and the product hashes tokens with plain SHA-256.
pub fn sha256(text: &str) -> [u8; 32] {
    use std::io::Write as _;
    let mut child = Command::new("/usr/bin/shasum")
        .args(["-a", "256"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(text.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    let hex = String::from_utf8(out.stdout).unwrap();
    let hex = hex.split_whitespace().next().unwrap();
    let mut bytes = [0_u8; 32];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap();
    }
    bytes
}

/// The path of the `teamwire` binary, built once per test process.
pub fn teamwire_bin() -> &'static Path {
    static BIN: OnceLock<PathBuf> = OnceLock::new();
    BIN.get_or_init(|| {
        let exe = std::env::current_exe().unwrap();
        let profile_dir = exe.parent().and_then(Path::parent).unwrap().to_path_buf();
        let target_dir = profile_dir.parent().unwrap().to_path_buf();
        let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml");
        let mut args = vec![
            "build".to_owned(),
            "--locked".to_owned(),
            "-p".to_owned(),
            "teamwire-cli".to_owned(),
            "--bin".to_owned(),
            "teamwire".to_owned(),
            "--manifest-path".to_owned(),
            manifest.to_owned(),
            "--target-dir".to_owned(),
            target_dir.display().to_string(),
        ];
        if profile_dir.file_name().is_some_and(|n| n == "release") {
            args.push("--release".to_owned());
        }
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let out = Command::new(cargo).args(&args).output().unwrap();
        assert!(
            out.status.success(),
            "building teamwire failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let bin = profile_dir.join("teamwire");
        assert!(bin.is_file(), "{} is missing", bin.display());
        bin
    })
}

/// A small, replayable random source (`SplitMix64`) seeded from `getrandom`,
/// or from `TEAMWIRE_CRASH_SEED` when a failure is replayed.
#[derive(Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn from_env(name: &str) -> Self {
        let seed = if let Ok(s) = std::env::var("TEAMWIRE_CRASH_SEED") {
            s.parse().expect("TEAMWIRE_CRASH_SEED is a number")
        } else {
            let mut b = [0_u8; 8];
            getrandom::fill(&mut b).unwrap();
            u64::from_le_bytes(b)
        };
        eprintln!("{name}: seed {seed} (replay with TEAMWIRE_CRASH_SEED={seed})");
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A duration in `[0, max]`, with microsecond steps.
    pub fn up_to(&mut self, max: Duration) -> Duration {
        let micros = u64::try_from(max.as_micros()).unwrap().max(1);
        Duration::from_micros(self.next_u64() % (micros + 1))
    }
}

#[derive(Debug)]
pub enum Identity {
    Lead,
    Member(String),
}

/// A child process with its stderr and stdout going to files, so output
/// written before a kill can be read after it, even when a grandchild (a
/// fake herdr call) still holds a pipe open.
#[derive(Debug)]
pub struct Running {
    pub child: Child,
    out: std::fs::File,
    err: std::fs::File,
}

#[derive(Debug)]
pub struct Ended {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Running {
    pub fn kill9(&mut self) {
        // The child may already have exited; that is a legal kill point.
        kill(
            Pid::from_raw(self.child.id().try_into().unwrap()),
            Signal::SIGKILL,
        )
        .ok();
    }

    pub fn finish(mut self) -> Ended {
        let status = self.child.wait().unwrap();
        Ended {
            code: status.code(),
            stdout: slurp(&mut self.out),
            stderr: slurp(&mut self.err),
        }
    }

    /// What the child has written to stderr so far.
    pub fn stderr_so_far(&mut self) -> String {
        slurp(&mut self.err)
    }
}

fn slurp(file: &mut std::fs::File) -> String {
    let mut text = String::new();
    file.seek(SeekFrom::Start(0)).unwrap();
    file.read_to_string(&mut text).unwrap();
    text
}

#[derive(Debug)]
pub struct Rig {
    pub herdr: FakeHerdrEnv,
    pub home: TestHome,
    /// A directory put before the fakes on the `PATH` of every command, for a
    /// wrapper that slows one host call down.
    pub path_prefix: Option<PathBuf>,
}

impl Default for Rig {
    fn default() -> Self {
        Self::new()
    }
}

impl Rig {
    /// Team `alpha`, its lead session `lead-1`, no members yet.
    pub fn new() -> Self {
        let mut herdr = FakeHerdrEnv::new().unwrap();
        herdr.set_script(&ClaudeScript::default()).unwrap();
        let home = TestHome::new().unwrap();
        let mut store = Store::open(home.path(), &team_name()).unwrap();
        store
            .write(|tx| {
                tx.create_team(
                    at(),
                    &NewTeam {
                        name: team_name(),
                        title: "Alpha".to_owned(),
                        lead_harness: HarnessId::from_static("claude-code"),
                        lead_host: HostId::from_static("herdr"),
                        lead_route: LeadRoute::Sink,
                        lead_session: Some((
                            SessionId::parse(LEAD_SESSION).unwrap(),
                            SessionKind::Interactive,
                        )),
                    },
                )
                .map(|_| ())
            })
            .unwrap();
        Self {
            herdr,
            home,
            path_prefix: None,
        }
    }

    pub fn store(&self) -> Store {
        Store::open(self.home.path(), &team_name()).unwrap()
    }

    /// A member. `live` starts a `fake-claude` session in its own herdr pane;
    /// otherwise the member is recorded and no process stands behind it.
    pub fn add_member(&self, role: &str, kind: MemberKind, live: bool) {
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
            let name = format!("tw_{TEAM}_{role}");
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
        let hash = sha256(&token_of(role));
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

    /// `teamwire <args>` with a cleared environment: the fakes' `PATH`, the
    /// temporary home, and the caller's identity.
    pub fn command(&self, args: &[&str], identity: &Identity) -> Command {
        let mut cmd = Command::new(teamwire_bin());
        cmd.args(args)
            .env_clear()
            .envs(self.herdr.env())
            .env("TEAMWIRE_HOME", self.home.path())
            .env("HERDR_ENV", "1")
            .stdin(Stdio::null());
        if let Some(prefix) = &self.path_prefix {
            let path = self
                .herdr
                .env()
                .into_iter()
                .find_map(|(k, v)| (k == "PATH").then_some(v))
                .unwrap();
            cmd.env("PATH", format!("{}:{path}", prefix.display()));
        }
        match identity {
            Identity::Member(role) => {
                cmd.env("TEAMWIRE_TEAM", TEAM)
                    .env("TEAMWIRE_ROLE", role)
                    .env("TEAMWIRE_TOKEN", token_of(role));
            }
            Identity::Lead => {
                cmd.env("CLAUDE_CODE_SESSION_ID", LEAD_SESSION);
            }
        }
        cmd
    }

    /// Starts the command with output going to files.
    pub fn start(&self, args: &[&str], identity: &Identity) -> Running {
        let out = tempfile::tempfile().unwrap();
        let err = tempfile::tempfile().unwrap();
        let mut cmd = self.command(args, identity);
        cmd.stdout(Stdio::from(out.try_clone().unwrap()))
            .stderr(Stdio::from(err.try_clone().unwrap()));
        Running {
            child: cmd.spawn().unwrap(),
            out,
            err,
        }
    }

    pub fn run(&self, args: &[&str], identity: &Identity) -> Ended {
        self.start(args, identity).finish()
    }

    /// Starts the command, waits `delay`, sends `SIGKILL`, and returns what
    /// the child had written by then.
    pub fn run_and_kill(&self, args: &[&str], identity: &Identity, delay: Duration) -> Ended {
        let mut running = self.start(args, identity);
        let started = Instant::now();
        while started.elapsed() < delay {
            std::hint::spin_loop();
        }
        running.kill9();
        running.finish()
    }
}

pub fn team_name() -> TeamName {
    TeamName::parse(TEAM).unwrap()
}

/// The store's own consistency check.
pub fn assert_integrity(store: &Store) {
    assert_eq!(store.integrity_check().unwrap(), "ok");
}

/// Every stored message, oldest first. A damaged row fails the test.
pub fn all_messages(store: &Store) -> Vec<teamwire_core::Message> {
    let all = store
        .read(|tx| tx.messages(&teamwire_store::MessageFilter::default()))
        .unwrap();
    assert!(all.bad.is_empty(), "{:?}", all.bad);
    all.items
}

/// How many stored messages carry this body. The tests send each message with
/// its own key as the body, so a count above 1 is a duplicate row.
pub fn count_with_body(store: &Store, body: &str) -> usize {
    all_messages(store)
        .iter()
        .filter(|m| m.body == body)
        .count()
}

pub fn recipient(role: &str) -> Recipient {
    Recipient::parse(role).unwrap()
}

pub fn actor(name: &str) -> Actor {
    Actor::parse(name).unwrap()
}
