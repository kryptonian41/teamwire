# Test strategy

Every behaviour gets a test that fails without it. The test for each failure
mode is named in `docs/failure-modes.md`; a ticket is done only when the tests
its rows name exist and pass.

## Rules every test follows

These come from the repository's `CLAUDE.md`, and they are hard rules:

- `TEAMWIRE_HOME` is always a fresh temporary directory (`tempfile::TempDir`).
  A shared helper, `teamwire_fakes::TestHome`, creates it and removes it.
  T01 writes it, so every wave-2 ticket has it from the start.
- No test types into a real herdr pane or starts a real Claude session. Tests
  use the fakes under `tests/fakes/`. The real `herdr` and `claude` binaries
  are hidden from tests by putting the fake binaries first on a `PATH` the test
  builds itself.
- No test reads or writes the real `~/.claude/`. The Claude adapter takes its
  session-file directory and its `~/.claude.json` path from its config; tests
  point both at the temporary home (`TEAMWIRE_CLAUDE_DIR`).
- No test depends on wall-clock sleeps for correctness. Time comes from the
  `Clock` trait; tests use `FakeClock` and advance it. A time limit in a test
  only guards against a hang and is at least 60 s per external call; a test
  about order asserts the order, not a duration; and the gate is checked with
  a second `cargo test --workspace` run loading the machine.
- A test that needs git makes a throwaway repository in its temporary
  directory.

## The fakes (`tests/fakes`, crate `teamwire-fakes`)

| Fake | Kind | What it does | Faults it can inject |
| --- | --- | --- | --- |
| `FakeHost` | in-process `Host` | Places in a map; records every call with its kind (read or write). `close` leaves a place whose agent is not the expected one and returns `NotFound`. `spawn` refuses a name that a live place holds with `agent_name_taken:` | Per call: fail with any `HostErrorCode`, hang past the timeout, succeed but report failure, report the wrong occupant, change the occupant between two calls |
| `FakeHarness` | in-process `Harness` | Sessions, pids and liveness in a map; configurable capabilities (with or without turn-end hook) | Background caller, dead session, two processes for one session, listing failure |
| `FakePlatform` | in-process `Platform` | Environments per pid, free space, a sleep hold that records start and release | Env read failure, low disk, network filesystem |
| `FakeClock` | in-process `Clock` | Time moves only when the test says so | Jumps (sleep and wake) |
| `fake-herdr` | binary | Starts every agent with a **cleared environment** (only `PATH`, `HOME`, `TMPDIR` and its own `FAKE_*` variables), as real herdr does in effect, so a member can get its identity only through its declared channel. Speaks herdr's JSON CLI for the commands teamwire uses (`agent start/prompt/get/list/read/send-keys`, `pane close/list/get`, `workspace` and `tab` create and rename). State in a JSON file named by `FAKE_HERDR_STATE`; every call appended to `FAKE_HERDR_LOG` | `agent start` takes only `--kind claude` (`fake-claude`), and refuses any other kind; the newline rule for arguments holds for it. A fault file (`FAKE_HERDR_FAULTS`) scripts errors by command and target: `agent_pane_busy` twice then success, `agent_prompt_stalled` (only for `agent prompt --wait`), `timeout` after the text was typed, a hang, `agent_not_found`, garbage output. The state lock is a kernel file lock, and `agent start` releases it while it waits for readiness. Unknown flags and bad `--source`, `--format` or `--until` values exit 2 with the usage line. A missing or malformed fault file exits 2 with `fake_misconfigured` |
| `fake-claude` | binary | Started by `fake-herdr agent start` in a fake pane. Writes a session file into `TEAMWIRE_CLAUDE_DIR/sessions/`, reads its `--settings` file and applies its `env` key to every shell step and hook it runs (as Claude Code does), and plays a script (`FAKE_CLAUDE_SCRIPT`) of steps: run a shell command (for example `teamwire send ...`, `teamwire done ...`), then end the turn by running the Stop hook from its settings with the JSON Claude Code would send | Never stop, stop twice, send a wrong session id, exit without stopping, print the trust dialog or the escalated trust dialog |

Each trait has a **contract suite** in `teamwire_fakes::contract`
(`host_contract(&HostSetup)`, `harness_contract(&HarnessSetup)`,
`platform_contract(&PlatformSetup)`; each setup record carries the adapter
and what the suite needs to make its own places, sessions or processes;
`tests/fakes/README.md` has one example per caller). The platform contract also checks that `write_atomic` keeps a file's mode (else 0644) and that a timeout kills a grandchild. The harness contract also checks that no secret identity value is in any argv. The fake passes it in `tests/fakes`, and
every real adapter crate calls it from its own tests: herdr against
`fake-herdr`, tmux against a real tmux on a private socket (`tmux -L
teamwire-test-<pid>`), macOS and Linux platforms against the real OS.

## Layers

### 1. Unit tests

In each crate, next to the code. Pure logic is tested without I/O: the six
message rules, sanitising, the locator decision table, the watch's
per-poll decision, the report-gap parser, `owns` matching, the verify
rules, spec validation, the close plan.

### 2. Property tests (`proptest`)

| Property | Crate | Generated input |
| --- | --- | --- |
| Committed `(sender, seq)` values are 1..n with no gap and no repeat; `(sender, key)` never makes two rows; display ids never repeat | store | Random interleavings of send, resend with the same key, and a rollback in the middle of a transaction |
| `one_line(s, n)` has no control characters, no newline, at most `n` characters, and `one_line(one_line(s, n), n) == one_line(s, n)` | core | Arbitrary strings, including ESC sequences and fake event lines |
| The locator returns a place only when the harness view and the host view name exactly one common place for the session | deliver | Random evidence sets |
| Delivery states only move forward; `acked` is final; a `pending` row is never set back to `pending` | msg | Random sequences of outcomes and acks |
| The watch prints at most one line per poll; each announcement key (which carries a generation or episode number) appears at most once across a run; once every live member is settled, `TEAM-COMPLETE` is printed exactly once | watch | Random observation sequences, including listing failures, clock jumps, added and removed members, reprompts |
| `owns` matching agrees with a reference implementation for `dir/`, `*` and `**` | verify | Random paths and patterns |

The migration test is a plain test, not a property test:
`crates/teamwire-store/tests/migrate.rs::migration_from_every_older_version_ends_in_the_same_schema`
opens a fixture database of each older `user_version` and compares the schema
with a fresh open.

### 3. Crash tests (`tests/crash`, crate `teamwire-crash-tests`)

They run the real `teamwire` binary as a child process and `kill -9` it at a
random moment. Then they reopen the store and assert:

- `PRAGMA integrity_check` returns `ok`;
- every message whose id the child printed as stored is present exactly once;
- resending every id with its key creates no second row;
- no completion is left in `delivering` after one more turn end.

Scenarios: kill during `send` (200 random kill points), kill right after `git worktree add` in launch (the member row and the worktree exist, and close removes the worktree or refuses naming it), kill right after `git worktree remove` in close (the branch and commits survive, and a second close finishes), kill after the host
call and before the outcome is recorded (a fake host that pauses on a signal
file), kill during `done`, kill during the Stop hook between `delivering` and
the outcome, kill during `launch` after the agent started, kill during
`close` after the archive and before deletion (this test runs 8 to 40 times until 2 kills land). Each has its row in
`docs/failure-modes.md` section 2.

### 4. Fault-injection tests

Service crates run against `FakeHost`, `FakeHarness` and `FakePlatform` with
scripted faults: a host that hangs, a host that accepts and reports failure,
a host that reports a stall or a timeout, busy-then-ready, a listing that fails for ten
polls, an occupant that changes between the check and the send, a disk that
fills (SQLite `max_page_count` set low), a clock that jumps an hour. These
tests are where most rows of `docs/failure-modes.md` live.

### 5. End-to-end tests (`tests/e2e`, crate `teamwire-e2e`)

Real `teamwire` binary, real git, `fake-herdr` and `fake-claude` on `PATH`,
temporary `TEAMWIRE_HOME` and temporary repository. Scenarios:

| Scenario | Proves |
| --- | --- |
| Launch two implementers and an investigator; each sends one message, commits, calls `done`; the watch prints `WATCHING` then `TEAM-COMPLETE 3/3 done. If no wave is due: add the integrator when branches join, then the closer (teamwire preset closer).`; verify passes; close archives and cleans up | The whole lifecycle, and "a healthy team wakes the lead with exactly two watch lines plus one line per result" |
| A member started by `fake-herdr` (cleared environment) runs `teamwire send` and `teamwire done` from a shell step, and its Stop hook delivers | Identity reaches the member only through `settings.json` `env`; this test fails if launch forgets it |
| A member's report lacks a progress line | Held once with the facts, then delivered on the next stop |
| The lead's pane is taken over by another fake session before a member finishes | Nothing typed into it; `UNDELIVERED` printed once; `status` shows the result path |
| A reprompt to a finished member | Watch re-arms; the second completion is a second result |
| `fake-herdr` times out after typing a message (fault `timed_out`) | Sender exits 5; state `unconfirmed`; nothing resent |
| A member added with `--add` after wave 1 | Watched; `TEAM-COMPLETE` waits for it |
| Remove an implementer without verify, then with verify | First refused, second accepted |
| Launch with a failing setup for one member | Exit 2; that member refused and cleaned; the others run |
| A harness configured without a turn-end hook (fake-claude in "no hook" mode) | `COMPLETED` printed by the watch once |

### 6. Live checks (not tests)

Facts that only the real programs can prove are checked by hand, by the
user, never in `cargo test`:

- `crates/teamwire-harness-claude/examples/live_check.rs`: starts one real
  Claude Code session in a scratch directory with teamwire's per-run
  `--settings` and `--mcp-config`, and confirms the Stop hook runs, the
  `settings.json` `env` values reach the Bash tool and the hook (the lead
  confirmed this by hand on 2026-09-29), the MCP server from `mcp.json` is
  started, and that server gets its token from the `env` key of `mcp.json`,
  not from argv or `settings.json` (ADR 0011). It prints each step and
  exits.
- `crates/teamwire-host-herdr/examples/live_check.rs`: read-only herdr calls
  (`agent list`, `agent get` on the caller's own pane) to confirm the JSON
  shapes still match the adapter's parser.
- `crates/teamwire-host-tmux/examples/tmux_live_check.rs`
  (`cargo run -p teamwire-host-tmux --example tmux_live_check --locked`):
  starts a private tmux server and proves spawn, a place that records its
  server, the identity environment, typing, submit, `read_tail`, state,
  occupant, list and close against the installed tmux, then kills the server.
- The lead's MCP server (`teamwire mcp --lead`, carried by the teamwire
  plugin that `teamwire install` registers): confirm it sees `CLAUDE_CODE_SESSION_ID` in its
  environment, so the lead's calls are accepted (harness Q4). Until this is
  confirmed the lead uses the CLI, which reads the variable in the Bash tool.

These are examples, not ignored tests, so nothing in the test suite is ever
skipped.

## Later: mutation testing

`cargo-mutants` on `teamwire-store`, `teamwire-deliver` and `teamwire-msg`
would show whether the tests catch changes to the integrity code (repopract
R20). It is not installed; installing it is open question 10 in
`ARCHITECTURE.md`. When installed, run it before each release, not on every
commit, and turn each surviving mutant in the delivery path into a ticket.

## Tests that fail now and then on macOS

Two tests of `teamwire-cli` fail now and then on a Mac for a cause outside
the code they test. Neither test is changed. Each entry below says how the
failure looks, why it happens, how sure the cause is, and what to do when you
see it.

### `an_answer_no_hook_takes_exits_7` (`crates/teamwire-cli/tests/dialog.rs:399`)

**Symptom.** The last state check fails:

```
panicked at crates/teamwire-cli/tests/dialog.rs:416:5:
  left: Expired
 right: Answered
```

nextest still reports the test as short, about 8 s.

**Cause.**

1. The test writes a dialog that expires 60 s later by the wall clock. The
   helpers `now()` and `write_dialog` read `SystemTime::now`
   (`dialog.rs:37-38`, `:151-152`).
2. `teamwire dialog answer` records the answer while the dialog is still
   pending.
3. The command then waits 5 s for the member's hook to take the answer
   (`ANSWER_TAKE_WAIT`, `crates/teamwire-core/src/dialog.rs:92`).
4. The command counts those 5 s with `Instant`
   (`crates/teamwire-cli/src/cmd/dialog/answer.rs:174`, `:183`, through
   `SystemClock::monotonic`).
5. On macOS, `Instant` reads a clock that stops while the Mac sleeps. The wall
   clock keeps moving.
6. A sleep during the run therefore moves the wall clock past the 60 s expiry,
   while the command and nextest count only a few seconds.
7. The test reads the dialog's state with the wall clock (`dialog.rs:165`).
8. The store checks "past `expires_at`" before it checks "an answer is
   recorded" (`crates/teamwire-store/src/dialog.rs:236-237`). So the store
   reports `Expired`, not `Answered`.

**How sure this is.** The failure was reproduced. A test run whose
`dialog answer` process was frozen for 61 s failed on the same line with the
same two values. A freeze of 50 s passed. The power log of the Mac (`pmset -g
log`) showed a 300 s sleep inside the failed run, which nextest had timed at
8.3 s. A second failure fits a 103 s sleep in the power log, but its output
was lost.

**Same exposure.** These tests of the same file also write a 60 s dialog by
the wall clock. Each expects the dialog to be still open when the test reads
it or runs `dialog answer` or `dialog show` on it. An expired dialog gets the
refusal `expired` and no printed command, so a long sleep can fail them too:

- `an_answer_to_a_dialog_that_changed_types_nothing_and_answers_nothing`
  (`dialog.rs:228`)
- `a_member_cannot_answer_even_with_its_token_unset` (`dialog.rs:323`)
- `an_unanswerable_call_is_refused` (`dialog.rs:371-384`)
- `show_prints_the_command_and_types_nothing` (`dialog.rs:480`)
- `show_escapes_control_and_invisible_characters_in_the_command`
  (`dialog.rs:551`)

**What to do.** This failure means the Mac slept during the run. Run the test
again with the Mac awake. `caffeinate -i` does not keep a Mac with a closed
lid awake.

**Where the cause is.** The cause is in the test, not in `teamwire dialog`.
The test sets a real 60 s expiry by the wall clock, and the command it runs
uses the real clock (`crates/teamwire-cli/src/wiring.rs:38`). A fix must
respect two facts. On macOS, 5 s by `Instant` can span any amount of wall
time if the Mac sleeps. The store's expiry rule comes before its answer rule.

### `a_hyphen_in_the_team_name_is_refused_with_exit_1` (`crates/teamwire-cli/tests/launch.rs:155`)

**Symptom.** The child `teamwire launch` has no exit code, and the test
fails at once:

```
assertion `left == right` failed
  left: None
 right: Some(1)
```

The same kill shows up in an install test as a panic on the missing exit code
(`crates/teamwire-cli/tests/install.rs:114`, `out.status.code().unwrap()`).

**Likely cause.**

1. The install tests give each test bed a hard link to the built binary,
   `target/debug/teamwire`, inside a temporary folder (`link_binary`,
   `install.rs:37-47`, called from `Bed::binary` at `install.rs:90-95`).
2. Each temporary folder is a `TestHome` (`tests/fakes/src/lib.rs:48`), so the
   folder and the link in it are deleted when the test ends.
3. This test starts the same file by its main name (`launch.rs:64`).
4. nextest runs the install tests and this test at the same time, because
   neither is in a test group (`.config/nextest.toml`).
5. macOS checks some process starts before they run and keeps the new process
   waiting for the answer. For a file with several hard links, the check can
   name the file by any of its links.
6. When the check names a link that another test then deletes, the system
   kills the waiting process with SIGKILL. The process dies before any
   teamwire code runs.
7. `wait_with_output` (`launch.rs:88`) returns a status with no exit code, and
   the assertion at `launch.rs:159` prints `left: None`.

**How sure this is.** This cause is likely, not proven. A stand-in loop showed
the mechanism. Starts of a binary by its main name died 4 times in 28,954
while other workers made and deleted hard links to it. Without the links,
0 of 21,489 starts died. In each case the system log named a deleted link as
the file it refused. In the real install tests, 1 run of 144 lost a child the
same way, at `install.rs:114`. The failed run of this test also fits: 7
install tests were running when it failed, and the system logged 3 dead
waiting starts in the same half second. Two pieces of evidence are missing.
This test's own launch call died 0 times in 47,882 tries. The kernel log
lines of the failed run are gone, so no line ties the kill to this test's
child.

**What to do.** Run the test again on its own. A second failure on its own is
a different problem, and needs its own investigation.

**Where the cause is.** The cause is in a test helper together with macOS,
not in `teamwire launch`. The only hard link in the repository is the one in
`link_binary` (`install.rs:42`). teamwire's own `install` makes no link: it
records the binary's resolved path (`this_binary`,
`crates/teamwire-cli/src/cmd/install.rs:170`). A fix must respect these
facts:

- On macOS, a binary with several hard links can be checked under the name of
  any link, so deleting one name can kill a start made through another name.
- Cargo puts `target/debug/teamwire` in place by deleting the old file and
  copying in the new one. A test that starts the binary while another cargo
  command runs in the same `target/` can get "No such file or directory".
- `Output.status.code()` is `None` for any death by a signal.

`kill9_during_done_stores_whole_or_nothing`
(`tests/crash/tests/kill_done.rs:43`) also failed once, in a full run under
load, with "only 0 runs printed their completion before the kill; the kill
window is off". Its cause was not investigated.

## Gates

The gates, in the order they run, are listed in `docs/conventions.md`. A
member runs only the check named in its brief; the lead runs the full
gate once after integrating a wave.
