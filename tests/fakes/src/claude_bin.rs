//! The body of the `fake-claude` binary: a stand-in for one Claude Code
//! session. It writes a session file, reads its `--settings` file, waits for
//! prompts, runs the steps of its script, and ends each turn by running the
//! Stop hook with the JSON Claude Code would send. See `tests/fakes/README.md`.
//!
//! It refuses to start unless `TEAMWIRE_CLAUDE_DIR` or `FAKE_CLAUDE_DIR`
//! names the directory for its session file, so it can never write into the
//! real `~/.claude`.

mod agents;
mod script;
mod settings;

pub use script::{Fault, Script, Startup, Step};

use std::io::{BufRead as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc;
use std::time::Duration;

use serde_json::json;

use crate::fsutil;
use crate::harness::{ESCALATED_TRUST_TEXT, PLAIN_TRUST_TEXT, PROMPT_TEXT, TRUST_OPTION_TEXT};
use settings::{Settings, hold_reason, run_shell};

/// How many times a Stop hook may hold one turn before the turn ends anyway
/// (Claude Code allows eight).
const MAX_HOLDS: usize = 8;

/// Where prompts and keys come from: files in a pane directory when
/// `fake-herdr` started the process, standard input otherwise.
enum Input {
    Pane {
        dir: PathBuf,
        prompts_seen: usize,
        keys_seen: usize,
    },
    Stdin(mpsc::Receiver<String>),
}

enum Event {
    Prompt(String),
    Key(String),
}

impl Input {
    fn from_env() -> Self {
        if let Some(dir) = std::env::var_os("FAKE_PANE_DIR") {
            return Self::Pane {
                dir: PathBuf::from(dir),
                prompts_seen: 0,
                keys_seen: 0,
            };
        }
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in std::io::stdin().lock().lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        Self::Stdin(rx)
    }

    /// The next prompt or key, waiting for it. `None` when input has ended.
    fn next(&mut self, dialog_open: bool) -> Option<Event> {
        loop {
            match self {
                Self::Pane {
                    dir,
                    prompts_seen,
                    keys_seen,
                } => {
                    let (name, seen) = if dialog_open {
                        ("keys", &mut *keys_seen)
                    } else {
                        ("inbox", &mut *prompts_seen)
                    };
                    let text = fsutil::read_text(&dir.join(name)).unwrap_or_default();
                    if let Some(line) = text.lines().nth(*seen) {
                        *seen += 1;
                        return Some(if dialog_open {
                            Event::Key(line.to_owned())
                        } else {
                            let prompt = serde_json::from_str::<String>(line)
                                .unwrap_or_else(|_| line.to_owned());
                            Event::Prompt(prompt)
                        });
                    }
                    std::thread::sleep(Duration::from_millis(15));
                }
                Self::Stdin(rx) => {
                    let line = rx.recv().ok()?;
                    return Some(if dialog_open {
                        Event::Key(line)
                    } else {
                        Event::Prompt(line)
                    });
                }
            }
        }
    }
}

struct Session {
    id: String,
    name: String,
    cwd: String,
    file: PathBuf,
    pane_dir: Option<PathBuf>,
    seq: u64,
    settings: Settings,
    script: Script,
    transcript: PathBuf,
    /// Set by a script step; the Stop hook input lists them, and a non-empty
    /// list means the session is paused, not finished.
    background_tasks: Vec<String>,
}

fn say(text: &str) {
    let mut out = std::io::stdout().lock();
    // The screen is a file or a terminal; a write error has nowhere to go.
    writeln!(out, "{text}").ok();
    out.flush().ok();
}

/// The `status` Claude Code writes in its session file: `idle`, `busy`, or
/// `waiting` (with `waitingFor`). `fake-herdr` gets the finer status through
/// the pane directory, so `done` is idle here.
fn session_status(status: &str) -> &'static str {
    match status {
        "working" => "busy",
        "blocked" => "waiting",
        _ => "idle",
    }
}

fn refuse(message: &str) -> ExitCode {
    writeln!(std::io::stderr().lock(), "fake-claude: {message}").ok();
    ExitCode::from(2)
}

#[expect(
    clippy::disallowed_methods,
    reason = "a session file records the real start time; fake-claude is a process, not a service under test"
)]
fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(0))
}

impl Session {
    fn write_session_file(&self, status: &str) {
        let doc = json!({
            "pid": std::process::id(),
            "sessionId": self.id,
            "cwd": self.cwd,
            "startedAt": now_millis(),
            "version": "fake",
            "peerProtocol": 1,
            "kind": "interactive",
            "entrypoint": "cli",
            "name": self.name,
            "status": session_status(status),
            "updatedAt": now_millis(),
        });
        let mut doc = doc;
        if status == "blocked"
            && let Some(map) = doc.as_object_mut()
        {
            map.insert("waitingFor".to_owned(), json!("input needed"));
        }
        if let Some(parent) = self.file.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        fsutil::write_atomic(&self.file, doc.to_string().as_bytes()).ok();
    }

    /// Records a state change where `fake-herdr` and the session file can see it.
    fn set_status(&mut self, status: &str) {
        self.seq += 1;
        if let Some(dir) = &self.pane_dir {
            let doc = json!({ "status": status, "seq": self.seq });
            fsutil::write_atomic(&dir.join("status.json"), doc.to_string().as_bytes()).ok();
        }
        self.write_session_file(status);
    }

    fn exit(&mut self) -> ExitCode {
        self.set_status("exited");
        std::fs::remove_file(&self.file).ok();
        ExitCode::SUCCESS
    }

    fn run_step(&mut self, step: &Step) {
        match step {
            Step::BackgroundTasks(tasks) => self.background_tasks.clone_from(tasks),
            Step::Print(text) => say(text),
            Step::SleepMs(ms) => std::thread::sleep(Duration::from_millis(*ms)),
            Step::Run(command) => {
                say(&format!("$ {command}"));
                let ran = run_shell(command, &self.settings.env, None);
                for line in ran.stdout.lines().chain(ran.stderr.lines()) {
                    say(line);
                }
                say(&format!("[exit {}]", ran.code));
            }
        }
    }

    /// Runs every Stop hook with the JSON Claude Code sends. Returns the
    /// reason if one of them holds the agent.
    fn run_stop_hooks(
        &self,
        turn: usize,
        hold_active: bool,
        wrong_session: bool,
    ) -> Option<String> {
        let session_id = if wrong_session {
            format!("not-{}", self.id)
        } else {
            self.id.clone()
        };
        let final_message = self
            .script
            .final_message
            .clone()
            .unwrap_or_else(|| format!("fake-claude finished turn {turn}"));
        let input = json!({
            "session_id": session_id,
            "transcript_path": self.transcript.display().to_string(),
            "cwd": self.cwd,
            "permission_mode": "bypassPermissions",
            "hook_event_name": "Stop",
            "stop_hook_active": hold_active,
            "prompt_id": format!("prompt-{turn}"),
            "background_tasks": self.background_tasks.iter()
                .map(|t| json!({ "id": t, "status": "running" }))
                .collect::<Vec<_>>(),
            "last_assistant_message": final_message,
        })
        .to_string();
        let mut hold = None;
        for command in &self.settings.stop_hooks {
            let ran = run_shell(command, &self.settings.env, Some(&input));
            hold = hold.or_else(|| hold_reason(&ran));
        }
        hold
    }

    /// One prompt: its steps, then the end of the turn.
    fn turn(&mut self, n: usize, prompt: &str) -> Option<ExitCode> {
        say(&format!("{PROMPT_TEXT}{prompt}"));
        fsutil::append(
            &self.transcript,
            format!("{}\n", json!({ "user": prompt })).as_bytes(),
        )
        .ok();
        if prompt.trim() == "/exit" {
            return Some(self.exit());
        }
        self.set_status("working");
        self.background_tasks.clear();
        for step in self.script.turn(n).to_vec() {
            self.run_step(&step);
        }
        let fault = self.script.fault;
        if fault == Fault::NeverStop {
            std::thread::sleep(Duration::from_secs(3600));
        }
        if fault == Fault::ExitWithoutStop {
            return Some(self.exit());
        }
        if !self.script.no_hook {
            let wrong = fault == Fault::WrongSession;
            let mut active = false;
            for _ in 0..MAX_HOLDS {
                let Some(reason) = self.run_stop_hooks(n, active, wrong) else {
                    break;
                };
                say(&format!("Stop hook feedback: {reason}"));
                for step in self.script.on_hold.clone() {
                    self.run_step(&step);
                }
                active = true;
            }
            if fault == Fault::StopTwice {
                self.run_stop_hooks(n, false, wrong);
            }
        }
        say(PROMPT_TEXT);
        // herdr shows `done` after a turn, until someone looks at the agent.
        self.set_status("done");
        None
    }

    /// The startup dialog. Returns false when the answer was "No, exit".
    /// As in Claude Code, "No, exit" is preselected and only Down then Enter
    /// trusts the folder; digits and a bare Enter do not.
    fn dialog(&mut self, input: &mut Input, escalated: bool) -> bool {
        say(PLAIN_TRUST_TEXT);
        if escalated {
            say(ESCALATED_TRUST_TEXT);
        }
        let render = |cursor: usize| {
            let mark = |i: usize| if i == cursor { "\u{276f}" } else { " " };
            say(&format!("{} No, exit", mark(0)));
            say(&format!("{} {TRUST_OPTION_TEXT}", mark(1)));
        };
        render(0);
        self.set_status("blocked");
        let mut cursor = 0_usize;
        loop {
            let Some(Event::Key(key)) = input.next(true) else {
                return false;
            };
            match key.as_str() {
                "down" => cursor = 1,
                "up" => cursor = 0,
                "esc" | "ctrl+c" => return false,
                "enter" => return cursor == 1,
                _ => continue,
            }
            render(cursor);
        }
    }
}

/// The value after `flag` on the command line.
fn flag_value<'a>(argv: &'a [String], flags: &[&str]) -> Option<&'a str> {
    argv.windows(2)
        .find(|w| w.first().is_some_and(|f| flags.contains(&f.as_str())))
        .and_then(|w| w.get(1).map(String::as_str))
}

fn claude_dir() -> Option<PathBuf> {
    ["TEAMWIRE_CLAUDE_DIR", "FAKE_CLAUDE_DIR"]
        .iter()
        .find_map(|k| std::env::var_os(k).filter(|v| !v.is_empty()))
        .map(PathBuf::from)
}

/// Runs one session until it exits.
pub fn run(argv: &[String]) -> ExitCode {
    let Some(claude_dir) = claude_dir() else {
        return refuse(
            "set TEAMWIRE_CLAUDE_DIR (or FAKE_CLAUDE_DIR); fake-claude never uses ~/.claude",
        );
    };
    if argv.first().map(String::as_str) == Some("agents") {
        return agents::run(&claude_dir, argv);
    }
    let settings = match flag_value(argv, &["--settings"]) {
        Some(path) => match Settings::load(Path::new(path)) {
            Ok(s) => s,
            Err(e) => return refuse(&e),
        },
        None => Settings::default(),
    };
    let script_path = std::env::var_os("FAKE_CLAUDE_SCRIPT").map(PathBuf::from);
    let script = match script::load(script_path.as_deref()) {
        Ok(s) => s,
        Err(e) => return refuse(&e),
    };
    let pid = std::process::id();
    let id = flag_value(argv, &["--session-id", "--resume"])
        .map_or_else(|| format!("fake-session-{pid}"), str::to_owned);
    let pane_dir = std::env::var_os("FAKE_PANE_DIR").map(PathBuf::from);
    let mut session = Session {
        name: flag_value(argv, &["-n", "--name"])
            .unwrap_or("fake-claude")
            .to_owned(),
        cwd: std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        file: claude_dir.join("sessions").join(format!("{pid}.json")),
        transcript: claude_dir.join("projects").join(format!("{id}.jsonl")),
        background_tasks: Vec::new(),
        pane_dir,
        seq: 0,
        settings,
        script,
        id,
    };
    if let Some(parent) = session.transcript.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    session.write_session_file("starting");
    let mut input = Input::from_env();

    let startup = session.script.startup;
    if startup != Startup::None && !session.dialog(&mut input, startup == Startup::EscalatedTrust) {
        return session.exit();
    }
    say("fake-claude ready");
    say(PROMPT_TEXT);
    session.set_status("idle");

    let mut turns = 0;
    while let Some(event) = input.next(false) {
        if let Event::Prompt(text) = event {
            if let Some(code) = session.turn(turns, &text) {
                return code;
            }
            turns += 1;
        }
    }
    session.exit()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_value_finds_the_argument_after_any_alias() {
        let argv: Vec<String> = ["-n", "x", "--session-id", "abc", "--settings", "/s.json"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        assert_eq!(
            flag_value(&argv, &["--session-id", "--resume"]),
            Some("abc")
        );
        assert_eq!(flag_value(&argv, &["--settings"]), Some("/s.json"));
        assert_eq!(flag_value(&argv, &["--model"]), None);
    }
}
