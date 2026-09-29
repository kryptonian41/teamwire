# teamwire-fakes

Test doubles for the three adapter traits and the clock, two fake programs
(`fake-herdr`, `fake-claude`), and the contract suites every adapter must pass.
Every later test runs on these. Nothing here touches the real `~/.claude`,
`~/.teamwire`, `herdr` or `claude`.

| Item | Kind | Use |
| --- | --- | --- |
| `TestHome` | helper | A fresh `TEAMWIRE_HOME` in a temporary directory. |
| `FakeClock` | in-process `Clock` | Time moves when the test says. `advance`, `sleep_and_wake` (wall clock jumps, monotonic does not), `set_wall`. |
| `FakePlatform` | in-process `Platform` | Environments per pid, free space, network-filesystem flag, a sleep hold that records start and release (`hold_events`). It writes real files, so give it a temporary directory. |
| `FakeHarness` | in-process `Harness` | Sessions, pids, liveness; switchable capabilities (`set_caps`); a background caller; a listing failure. |
| `FakeHost` | in-process `Host` | Places in a map; scripted faults per method. |
| `fake-herdr`, `fake-claude` | binaries | See below. |
| `contract::*` | suites | See below. |

Every in-process fake keeps a `CallLog` (`fake.log()`): each call with its
`CallKind::Read` or `Write`, so a test can assert "the watch made only read
calls" (`log.count(CallKind::Write) == 0`).

## Faults on the in-process fakes

- `FakeHost::inject(op, Fault, times)` and `inject_always(op, Fault)`, where
  `op` is a method name (`"spawn"`, `"submit"`, `"state"`, `"occupant"`,
  `"read_tail"`, `"send_keys"`, `"close"`, `"list"`, `"live_agent_names"`).
  `Fault::Fail(HostError)` (any `HostErrorCode`), `Fault::Hang` (returns
  `Timeout`, changes nothing), `Fault::AcceptedButFail(HostError)` (does the
  work, then reports failure), `Fault::WrongOccupant(Occupant)`,
  `Fault::ChangeOccupantAfter(Occupant)` (the occupant changes between two
  calls). Busy twice then success is `inject("spawn", Fail(Busy), 2)`.
- `FakePlatform`: `fail_env_reads`, `set_free_bytes` (low disk),
  `set_network_fs`, `fail_writes` (full disk), `queue_run` (the next `run`
  result), `add_external_hold`, `reuse_hold_pid`, `end_hold_process`,
  `fail_hold_start`; `FakePlatform::without_sleep_hold()` is the Linux shape.
- `FakeHarness`: `add_session` with `Liveness::Dead`, two pids for one
  session, `set_background_caller`, `fail_listing`, `set_caps` (for example
  `turn_end_hook: false`).

## Contract suites

`teamwire_fakes::contract::{host_contract, harness_contract, platform_contract}`
each take one setup record, run every rule that all adapters share, and panic
with `contract violated: <rule>` on the first broken rule. A suite never
assumes a product: it checks only what the trait promises. Each caller builds
the setup; the suite makes and closes its own places.

### `host_contract(&HostSetup)`

`HostSetup::new(host, &request)` where `request: Fn(&Address) -> SpawnRequest`
returns the request for one new place (the program must keep running until
the place is closed). Fields to adjust: `timeout` (default 10 s),
`echoes_input` (true when the program echoes typed text, so the suite can look
for it in `read_tail`), `lists_agent_names` (false for a host without named
agents), `screen_attempts`.

T05, the herdr adapter against `fake-herdr`:

```rust
let mut env = FakeHerdrEnv::new()?;                  // private state, PATH with the fakes first
env.set_script(&ClaudeScript::default())?;           // what the started fake-claude does
let settings = env.write_file("settings.json", r#"{"env":{},"hooks":{}}"#)?;
let herdr = Herdr::new(platform_that_runs_with(env.env()), /* .. */);
let request = |name: &Address| SpawnRequest {
    name: name.clone(), title: "contract".into(), cwd: env.root().into(),
    argv: vec!["-n".into(), name.as_str().into(), "--settings".into(), settings.display().to_string()],
    env: vec![],
};
let mut setup = HostSetup::new(&herdr, &request);
setup.echoes_input = true;
host_contract(&setup);
```

`platform_that_runs_with(env.env())` means: the `Platform` the adapter calls
`run` on starts `herdr` with exactly `env.env()` (`PATH`, `FAKE_HERDR_*`,
`TEAMWIRE_CLAUDE_DIR`), and `PATH` finds the fake because `FakeHerdrEnv`
installed a `herdr` link first.

T16, the tmux adapter against a real tmux on a private socket:

```rust
let socket = format!("teamwire-test-{}", std::process::id());
let tmux = Tmux::new(platform, /* -L */ &socket);
let request = |name: &Address| SpawnRequest {
    name: name.clone(), title: "contract".into(), cwd: dir.path().into(),
    argv: vec!["cat".into()], env: vec![],
};
let mut setup = HostSetup::new(&tmux, &request);
setup.echoes_input = true;          // a terminal running cat echoes what is typed
setup.lists_agent_names = false;    // if the adapter has no agent names
host_contract(&setup);
```

### `harness_contract(&HarnessSetup)`

The caller builds the harness with its files and directories in a temporary
directory, then fills: `member` (a `MemberLaunch` whose `identity` holds a
secret-named value such as `TEAMWIRE_TOKEN`; the suite checks the token
reaches the member only through the channel the harness declares, that a
file holding it is mode 0600 or tighter, and that `Debug` hides it),
`turn_end_stdin` and `turn_end_session` (one valid hook input and the session
it names), `prompt_screen` and `dialog_screen` (text the real harness shows),
`live_session` (a session the harness knows is running; `None` skips the
liveness rules), `unknown_session`, `timeout`.

T06, the Claude adapter with its paths in a temporary directory:

```rust
let dir = tempfile::tempdir()?;
let claude = ClaudeCode::new(ClaudePaths::under(dir.path()), platform);
let setup = HarnessSetup {
    harness: &claude,
    member: member_launch(dir.path()),               // identity has TEAMWIRE_TOKEN
    turn_end_stdin: br#"{"session_id":"member-1","stop_hook_active":false}"#.to_vec(),
    turn_end_session: SessionId::parse("member-1")?,
    prompt_screen: "ready\n> ".into(),               // what real Claude shows at its prompt
    dialog_screen: "Do you trust the files in this folder?".into(),
    live_session: Some(session_started_by_fake_claude),
    unknown_session: SessionId::parse("never-heard-of")?,
    timeout: Duration::from_secs(5),
};
harness_contract(&setup);
```

`fake-claude` (started with `TEAMWIRE_CLAUDE_DIR` set to the same directory the
adapter reads) writes the session file `live_session` refers to.

### `platform_contract(&PlatformSetup)`

Fields: `platform`, `dir` (an empty scratch directory), `live_pid` (a running
process; the test's own pid is fine), `dead_pid` (a reaped child, or `None`),
`env_probe` (`EnvProbe { pid, var, value }` for a child you started with that
variable set, or `None`), `run_cases` (`RunCase::unix_standard()` for a real
Unix platform, or empty), `hold_lead` (the pid a sleep hold protects; used
only when the platform has a hold).

T03, the macOS platform against the real OS:

```rust
let dir = tempfile::tempdir()?;
let mut child = std::process::Command::new("sleep").arg("30")
    .env("TEAMWIRE_CONTRACT_PROBE", "probe-value").spawn()?;
let mut dead = std::process::Command::new("true").spawn()?;
dead.wait()?;
let platform = MacOs::new();
platform_contract(&PlatformSetup {
    platform: &platform,
    dir: dir.path(),
    live_pid: Pid::new(std::process::id()),
    dead_pid: Some(Pid::new(dead.id())),
    env_probe: Some(EnvProbe { pid: Pid::new(child.id()), var: "TEAMWIRE_CONTRACT_PROBE".into(),
                               value: "probe-value".into() }),
    run_cases: RunCase::unix_standard(),
    hold_lead: Pid::new(child.id()),
});
child.kill()?;
```

The in-process fakes pass their own suites in `tests/fakes/tests/contract.rs`;
that file also shows a fake that breaks a rule making the suite fail.

## `fake-herdr`

A stand-in for the `herdr` command line. Build with
`cargo build -p teamwire-fakes --bins`. In a test, do not run it by hand:
`FakeHerdrEnv` (private state directory, environment, fault and script files,
log reader) and `FakeBins` / `FakeBinDir` (a bin directory where `herdr` and
`claude` are links to the fakes, and a `PATH` of that directory plus
`/usr/bin:/bin:/usr/sbin:/sbin`) do the setup. `FakeBins::ensure()` builds the
binaries once per process, so any crate's tests can use them. `FakeBinDir`
refuses to build a `PATH` that could reach a real `herdr` or `claude`.

Commands (herdr's own names and flags): `agent start|prompt|get|list|read|send-keys`,
`pane close|list|get`, `workspace create|get|rename`, `tab create|rename`.
Anything else fails with `unknown_command`. Output is herdr's envelope:
success is `{"id":"cli:agent:get","result":{..,"type":"agent_info"}}` on
stdout, exit 0; failure is `{"error":{"code":..,"message":..}}` on stderr,
exit 1. Exit 2 means the fake itself is misconfigured (no `FAKE_HERDR_STATE`).

`agent start <name> --kind claude --pane <pane> [--timeout ms] -- <claude args>`
starts `fake-claude` (or `$FAKE_CLAUDE_BIN`) in the pane with a **cleared
environment**: only `PATH`, `HOME`, `TMPDIR`, every `FAKE_*` variable, plus
`FAKE_PANE_DIR`, `FAKE_PANE_ID` and `FAKE_CLAUDE_DIR` (its own
`TEAMWIRE_CLAUDE_DIR`, renamed). No `TEAMWIRE_*` and no `HERDR_*` variable
reaches the agent, so a member gets its identity only through its
`settings.json` `env`. Like herdr it waits until the agent is ready:
`agent_not_ready` (the agent stays running) when the agent shows a startup
dialog, `timeout` when it never becomes ready. It refuses a newline in an
argument (`invalid_agent_argument`), a second agent in a pane
(`agent_pane_busy`), a used name (`agent_name_in_use`, a name only the fake
uses) and any kind but `claude` (`agent_kind_unsupported`, also the fake's).

`agent prompt` refuses a blocked agent with `agent_blocked` before typing. With
`--wait` it needs a state change (`working` or `blocked`) within
`FAKE_HERDR_WAIT_MS` (default 5000) or fails with `agent_prompt_stalled`, and
respects `--timeout` (`timeout`). `agent read --lines N` returns the last
lines of the agent's screen as `result.text`. `agent send-keys` accepts
`enter esc tab backspace up down left right ctrl+c` and single characters and
refuses others with `invalid_key`. `agent get` reports `agent_session.value`
from the session file `fake-claude` wrote (else the id on its command line).
`pane close` ends the agent and, as herdr does, closes an emptied tab and an
emptied workspace.

Environment of `fake-herdr` itself:

| Variable | Meaning |
| --- | --- |
| `FAKE_HERDR_STATE` | Required. The JSON state file. Pane directories (`screen.txt`, `status.json`, `inbox`, `keys`) sit beside it. |
| `FAKE_HERDR_LOG` | Optional. One JSON line per call: `{"argv":[..],"kind":"read"\|"write","fault":null\|"error:<code>"\|"hang"\|"garbage"\|"stalled"}`. |
| `FAKE_HERDR_FAULTS` | Optional. The fault file. |
| `FAKE_HERDR_HANG_MS` | How long a `hang` fault sleeps (default 30000), then fails with `timeout`. |
| `FAKE_HERDR_WAIT_MS` | The no-activity limit for `agent prompt --wait` (default 5000). |
| `FAKE_HERDR_START_WAIT_MS` | The cap on waiting for a started agent to be ready (default 10000). |
| `FAKE_CLAUDE_BIN` | The program `--kind claude` runs (default: `fake-claude` beside `fake-herdr`, else on `PATH`). |

The fault file, built with `HerdrFaults` or written by hand:

```json
{"rules": [
  {"cmd": "agent start", "times": 2, "action": {"error": {"code": "agent_pane_busy", "message": "busy"}}},
  {"cmd": "agent prompt", "target": "tw_demo_api", "times": 1, "action": "stalled"},
  {"cmd": "pane list", "action": "hang"},
  {"cmd": "agent list", "action": "garbage"},
  {"cmd": "agent get", "target": "tw_demo_api", "action": {"error": {"code": "agent_not_found"}}}
]}
```

`cmd` is the two command words and may be left out to match any call;
`target` is the first operand (an agent name or pane id) and may be left out;
`times` counts firings across processes (the counts live in the state file)
and, when left out, means always. The first rule that matches and has
firings left wins. `stalled` is for `agent prompt`: the text is typed into the
agent, then the call fails with `agent_prompt_stalled`, because herdr's docs
say a stall does not prove the prompt was not delivered.

## `fake-claude`

One fake Claude Code session. Flags it reads: `--settings <file>`,
`--session-id <id>` or `--resume <id>`, `-n <name>`; it ignores the rest. It
refuses to start (exit 2) unless `TEAMWIRE_CLAUDE_DIR` or `FAKE_CLAUDE_DIR`
names a directory, so it can never write into the real `~/.claude`.

- It writes `<dir>/sessions/<pid>.json` with the keys the real file has
  (`pid`, `sessionId`, `cwd`, `startedAt`, `version`, `kind`, `entrypoint`,
  `name`, `status`, `updatedAt`, ..) and removes it when it exits cleanly.
- It reads the `--settings` file: the `env` object is applied to every shell
  step and every hook it runs, as Claude Code does; `hooks.Stop[].hooks[].command`
  entries are the Stop hooks.
- Prompts arrive in `$FAKE_PANE_DIR/inbox` (when `fake-herdr` started it) or as
  lines on standard input. Each prompt is one turn: it echoes `> <prompt>`,
  runs the turn's steps, then runs each Stop hook with this JSON on stdin:
  `{"session_id","transcript_path","cwd","permission_mode":"bypassPermissions","hook_event_name":"Stop","stop_hook_active","last_assistant_message"}`.
  A hook that exits 2 (reason on stderr), or prints
  `{"decision":"block","reason":..}`, holds the turn: the reason is shown as
  `Stop hook feedback: ..`, the script's `on_hold` steps run, and the hooks run
  again with `stop_hook_active: true` (at most three holds).
- The prompt `/exit` ends the session.

The script (`FAKE_CLAUDE_SCRIPT` names a JSON file; `ClaudeScript` builds it):

```json
{
  "turns": [[{"run": "teamwire send lead 'hello'"}, {"print": "some text"}, {"sleep_ms": 50}]],
  "on_hold": [{"run": "teamwire done --summary 'fixed'"}],
  "startup": "none | trust | escalated_trust",
  "fault": "none | never_stop | stop_twice | wrong_session | exit_without_stop",
  "no_hook": false,
  "final_message": "text for last_assistant_message"
}
```

`turns[n]` is what the (n+1)th prompt does; the last list repeats. A `run`
step is `sh -c <command>` with the settings `env` applied; its output and
`[exit N]` go to the screen. Faults: `never_stop` (steps run, the hook never
does, the agent stays `working`), `stop_twice` (the hook runs twice for one
turn), `wrong_session` (the hook input names another session), 
`exit_without_stop` (the process ends after the steps without the hook),
`no_hook` (a harness without a turn-end hook). `startup: trust` prints the
plain folder-trust dialog (`PLAIN_TRUST_TEXT`); `escalated_trust` prints the
one that also asks to run hooks and MCP servers (`ESCALATED_TRUST_TEXT` first).
While a dialog is open the agent is `blocked`. The cursor starts on "No, exit"
(as in Claude Code); `down`, `up`, `1` and `2` move it, `enter` confirms.
Answering "No, exit", `esc` or `ctrl+c` ends the session.
