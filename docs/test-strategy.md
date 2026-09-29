# Test strategy

Every behaviour gets a test that fails without it. The test for each failure
mode is named in `docs/failure-modes.md`; a ticket is done only when the tests
its rows name exist and pass.

## Rules every test follows

These come from the repository's `CLAUDE.md`, and they are hard rules:

- `TEAMWIRE_HOME` is always a fresh temporary directory (`tempfile::TempDir`).
  A shared helper, `teamwire_fakes::TestHome`, creates it and removes it.
- No test types into a real herdr pane or starts a real Claude session. Tests
  use the fakes under `tests/fakes/`. The real `herdr` and `claude` binaries
  are hidden from tests by putting the fake binaries first on a `PATH` the test
  builds itself.
- No test reads or writes the real `~/.claude/`. The Claude adapter takes its
  session-file directory and its `~/.claude.json` path from its config; tests
  point both at the temporary home (`TEAMWIRE_CLAUDE_DIR`).
- No test depends on wall-clock sleeps for correctness. Time comes from the
  `Clock` trait; tests use `FakeClock` and advance it.
- A test that needs git makes a throwaway repository in its temporary
  directory.

## The fakes (`tests/fakes`, crate `teamwire-fakes`)

| Fake | Kind | What it does | Faults it can inject |
| --- | --- | --- | --- |
| `FakeHost` | in-process `Host` | Places in a map; records every call with its kind (read or write) | Per call: fail with any `HostErrorCode`, hang past the timeout, succeed but report failure, report the wrong occupant, change the occupant between two calls |
| `FakeHarness` | in-process `Harness` | Sessions, pids and liveness in a map; configurable capabilities (with or without turn-end hook) | Background caller, dead session, two processes for one session, listing failure |
| `FakePlatform` | in-process `Platform` | Environments per pid, free space, a sleep hold that records start and release | Env read failure, low disk, network filesystem |
| `FakeClock` | in-process `Clock` | Time moves only when the test says so | Jumps (sleep and wake) |
| `fake-herdr` | binary | Starts every agent with a **cleared environment** (only `PATH`, `HOME`, `TMPDIR` and its own `FAKE_*` variables), as real herdr does in effect, so a member can get its identity only through its declared channel. Speaks herdr's JSON CLI for the commands teamwire uses (`agent start/prompt/get/list/read/send-keys`, `pane close/list/get`, `workspace` and `tab` create and rename). State in a JSON file named by `FAKE_HERDR_STATE`; every call appended to `FAKE_HERDR_LOG` | A fault file (`FAKE_HERDR_FAULTS`) scripts errors by command and target: `agent_pane_busy` twice then success, `agent_prompt_stalled`, a hang, `agent_not_found`, garbage output |
| `fake-claude` | binary | Started by `fake-herdr agent start` in a fake pane. Writes a session file into `TEAMWIRE_CLAUDE_DIR/sessions/`, reads its `--settings` file and applies its `env` key to every shell step and hook it runs (as Claude Code does), and plays a script (`FAKE_CLAUDE_SCRIPT`) of steps: run a shell command (for example `teamwire send ...`, `teamwire done ...`), then end the turn by running the Stop hook from its settings with the JSON Claude Code would send | Never stop, stop twice, send a wrong session id, exit without stopping, print the trust dialog or the escalated trust dialog |

Each trait has a **contract suite** in `teamwire_fakes::contract`
(`host_contract(&dyn Host)`, `harness_contract(&dyn Harness)`,
`platform_contract(&dyn Platform)`). The fake passes it in `tests/fakes`, and
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
| Every migration path from each older `user_version` ends in the same schema | store | Each older version's fixture database |
| `one_line(s, n)` has no control characters, no newline, at most `n` characters, and `one_line(one_line(s, n), n) == one_line(s, n)` | core | Arbitrary strings, including ESC sequences and fake event lines |
| The locator returns a place only when the harness view and the host view name exactly one common place for the session | deliver | Random evidence sets |
| Delivery states only move forward; `acked` is final; a `pending` row is never set back to `pending` | msg | Random sequences of outcomes and acks |
| The watch prints at most one line per poll; each announcement key (which carries a generation or episode number) appears at most once across a run; once every live member is settled, `TEAM-COMPLETE` is printed exactly once | watch | Random observation sequences, including listing failures, clock jumps, added and removed members, reprompts |
| `owns` matching agrees with a reference implementation for `dir/`, `*` and `**` | verify | Random paths and patterns |

### 3. Crash tests (`tests/crash`, crate `teamwire-crash-tests`)

They run the real `teamwire` binary as a child process and `kill -9` it at a
random moment. Then they reopen the store and assert:

- `PRAGMA integrity_check` returns `ok`;
- every message whose id the child printed as stored is present exactly once;
- resending every id with its key creates no second row;
- no completion is left in `delivering` after one more turn end.

Scenarios: kill during `send` (200 random kill points), kill after the host
call and before the outcome is recorded (a fake host that pauses on a signal
file), kill during `done`, kill during the Stop hook between `delivering` and
the outcome, kill during `launch` after the agent started, kill during
`close` after the archive and before deletion. Each has its row in
`docs/failure-modes.md` section 2.

### 4. Fault-injection tests

Service crates run against `FakeHost`, `FakeHarness` and `FakePlatform` with
scripted faults: a host that hangs, a host that accepts and reports failure,
a host that reports a stall, busy-then-ready, a listing that fails for ten
polls, an occupant that changes between the check and the send, a disk that
fills (SQLite `max_page_count` set low), a clock that jumps an hour. These
tests are where most rows of `docs/failure-modes.md` live.

### 5. End-to-end tests (`tests/e2e`, crate `teamwire-e2e`)

Real `teamwire` binary, real git, `fake-herdr` and `fake-claude` on `PATH`,
temporary `TEAMWIRE_HOME` and temporary repository. Scenarios:

| Scenario | Proves |
| --- | --- |
| Launch two implementers and an investigator; each sends one message, commits, calls `done`; the watch prints `WATCHING` then `TEAM-COMPLETE 3/3 done`; verify passes; close archives and cleans up | The whole lifecycle, and "a healthy team wakes the lead with exactly two watch lines plus one line per result" |
| A member started by `fake-herdr` (cleared environment) runs `teamwire send` and `teamwire done` from a shell step, and its Stop hook delivers | Identity reaches the member only through `settings.json` `env`; this test fails if launch forgets it |
| A member's report lacks a progress line | Held once with the facts, then delivered on the next stop |
| The lead's pane is taken over by another fake session before a member finishes | Nothing typed into it; `UNDELIVERED` printed once; `status` shows the result path |
| A reprompt to a finished member | Watch re-arms; the second completion is a second result |
| `fake-herdr` stalls on a message | Sender exits 5; state `unconfirmed`; nothing resent |
| A member added with `--add` after wave 1 | Watched; `TEAM-COMPLETE` waits for it |
| Remove an implementer without verify, then with verify | First refused, second accepted |
| Launch with a failing setup for one member | Exit 2; that member refused and cleaned; the others run |
| A harness configured without a turn-end hook (fake-claude in "no hook" mode) | `COMPLETED` printed by the watch once |
| Run the Python-coexistence check: a fake global Stop hook that behaves like `team-report.sh`'s first checks is also present | It does nothing for teamwire members |

### 6. Live checks (not tests)

Facts that only the real programs can prove are checked by hand, by the
user, never in `cargo test`:

- `crates/teamwire-harness-claude/examples/live_check.rs`: starts one real
  Claude Code session in a scratch directory with teamwire's per-run
  `--settings` and `--mcp-config`, and confirms the Stop hook runs, the
  `settings.json` `env` values reach the Bash tool and the hook (the lead
  confirmed this by hand on 2026-09-29), and the MCP tools are listed. It
  prints each step and exits.
- `crates/teamwire-host-herdr/examples/live_check.rs`: read-only herdr calls
  (`agent list`, `agent get` on the caller's own pane) to confirm the JSON
  shapes still match the adapter's parser.

These are examples, not ignored tests, so nothing in the test suite is ever
skipped.

## Later: mutation testing

`cargo-mutants` on `teamwire-store`, `teamwire-deliver` and `teamwire-msg`
would show whether the tests catch changes to the integrity code (repopract
R20). It is not installed; installing it is open question 11 in
`ARCHITECTURE.md`. When installed, run it before each release, not on every
commit, and turn each surviving mutant in the delivery path into a ticket.

## Gates

The gates, in the order they run, are listed in `docs/conventions.md`. A
ticket runs only the check named in its build-plan row; the lead runs the full
gate once after integrating a wave.
