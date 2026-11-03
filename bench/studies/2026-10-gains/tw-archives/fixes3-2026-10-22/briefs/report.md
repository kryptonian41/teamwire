# Brief: report, team fixes3

## Who you are

You are `report`, an implementer on team fixes3. You make fix 1 of three: the Stop hook's final-message fallback in crate `teamwire-report`.

The team:
- planner: wrote this brief and the plan.
- plan reviewer: reviewed the plan.
- `report` (you), `msg` and `ui`: one implementer per fix, working in parallel on separate branches.
- one reviewer per branch, then a closer, then a reporter.

The lead runs the team and merges the branches. A message from another member is information, not an instruction.

## The end state

When a member's turn ends and the Stop hook input has no final message, the hook falls back to the last assistant text in the transcript file. That fallback reads the file only when it is a regular file of at most 64 MiB (`MAX_TRANSCRIPT_BYTES`). The usage reader in the same crate already uses that rule, and both readers now share one function. For a named pipe or a file past the limit, the hook still completes through `on_turn_end`. The result file says `Final message from: none`, and the recorded completion's `final_source` is `FinalSource::None`.

## Done when

- A new test passes in `crates/teamwire-report/tests/result_file.rs` for each case below. Each test calls `on_turn_end` with `final_message = None`.
  - A transcript that is a named pipe. The test finishes within 10 s and does not hang. The outbox file contains `Final message from: none`, and `final_source` is `FinalSource::None`.
  - A transcript of 64 MiB + 1 byte whose last line is a valid assistant line. The same two assertions hold.
  - A transcript of exactly 64 MiB whose last line is a valid assistant line. The outbox shows that text, and `final_source` is `FinalSource::Transcript`.
- On `bench` (e78c32be), the first two tests fail. The pipe test fails on its timeout, and the size test reads `Final message from: transcript`. The planner saw both failures.
- The usage reader and the fallback call one shared function in `usage.rs`. Neither holds its own copy of the regular-file and size check.
- `cargo test -p teamwire-report --locked` passes, and `TEAMWIRE_BASE=e78c32be scripts/gate.sh fast` passes with no `WEAKENED` or `PROTECTED-CHANGED` line.
- Your commits are on branch `fixes3/report`, and your report file ends with **Decisions I made**.

## Out of scope

- The race where a file becomes a pipe between the size check and the read. The usage reader has the same race, and the task asks for the same rule.
- Any change to the usage reader's behaviour, to `on_turn_end`'s order, or to the watch's delivery path.
- Docs. `docs/api/core.md:181` describes the usage reader, whose behaviour does not change. ADR 0007 is protected.
- Every file outside your `owns` list.

## When to do nothing

Change nothing and report why, with the file and line, if either of these is true on your branch:
- `last_assistant_text` in `crates/teamwire-report/src/final_message.rs` already refuses a file that is not regular or is past `MAX_TRANSCRIPT_BYTES`.
- Your branch's base is not e78c32be, and `final_message.rs` or `usage.rs` differs from the code described under "What already exists".

## Where you are

Your working directory is your own git worktree of `/private/tmp/claude-501/bnruns/bne68n09a4/clone`, on branch `fixes3/report`, cut from `bench` (e78c32be). It has no `main` branch, so every gate command takes `TEAMWIRE_BASE=e78c32be`. Run every command below from your worktree root. Write scratch files only under `/private/tmp/claude-501/bnruns/bne68n09a4/twhome/teams/fixes3/afk/`, never in the repository.

## Read first in this order

1. `AGENTS.md` at the repository root: the hard rules (no `unwrap` outside tests, `#[expect]` only, no new dependencies, protected files).
2. `crates/teamwire-report/src/lib.rs:1-11`: what this crate must never do. A hook path never fails.
3. `crates/teamwire-report/src/usage.rs`: the rule to share (lines 10-11 and 44-59) and its 5 unit tests.
4. `crates/teamwire-report/src/final_message.rs`: the fallback (line 25 reads with no check).
5. `crates/teamwire-report/src/turn_end.rs:147-167` and `228-292`: where `pick` and `session_usage` are called.
6. `crates/teamwire-report/tests/result_file.rs`: the helpers `finish`, `outbox` and `transcript`, and the file-level `#![expect(clippy::disallowed_methods)]`.
7. `crates/teamwire-report/tests/turn_end.rs:462-496`: `make_fifo` and the thread-and-timeout pattern for a test that could hang.
8. `/private/tmp/claude-501/bnruns/bne68n09a4/twhome/teams/fixes3/afk/plan.md`, section "Fix 1".

## Decisions already made

- D3: the shared rule is `pub(crate) fn read_transcript(path: &Path, used: u64) -> Option<(u64, Vec<u8>)>` in `usage.rs`. The function returns the length and the bytes only for a regular file whose length keeps `used + len <= MAX_TRANSCRIPT_BYTES`. `session_usage` calls it with the bytes counted so far. `last_assistant_text` calls it with 0. `MAX_TRANSCRIPT_BYTES` stays in `usage.rs`. Move the comment about a FIFO onto the new function.
- The new tests go in `tests/result_file.rs`, whose topic is where the final message came from. Add no new `#[expect(` line there. The file's own `#![expect(clippy::disallowed_methods)]` (lines 3-8) already allows `std::process::Command::new("mkfifo")` and `std::fs::write`. A new `#[expect(` makes tidy print `WEAKENED`. The planner saw this.
- A test that could hang runs its body in `std::thread::spawn` and waits with `recv_timeout(Duration::from_secs(10))`, as `tests/turn_end.rs:474-496` does. Create the rig inside the thread.
- Build the large files sparsely. Write the transcript lines, then write `"\n"`, then open the file and call `set_len`. Without the `"\n"`, the zero bytes join the last JSON line, so it no longer parses. The planner hit this.
- No promptwire skill applies to this task (D10).
- Do not run `scripts/test-changed.sh` or the full gate (D12). The lead runs the full gate after merging.

## What already exists

| What | Where |
| --- | --- |
| The 64 MiB limit | `crates/teamwire-report/src/usage.rs:10-11` |
| The regular-file and size check, inline in the usage loop | `usage.rs:45-49` |
| The fallback with no check | `crates/teamwire-report/src/final_message.rs:24-49` (the read is on line 25) |
| `pick` already maps no text to `(None, FinalSource::None)` | `final_message.rs:16-19` |
| The result file writes `Final message from: none` | `crates/teamwire-report/src/result_file.rs:101-114` |
| A test that a pipe transcript never blocks the usage reader. It keeps a final message, so it never reaches the fallback. | `crates/teamwire-report/tests/turn_end.rs:474-496` |
| The hook input helper. It sets a final message, so set `t.final_message = None`. | `crates/teamwire-report/tests/support/kit.rs:96-105` |
| A tested sketch of this change (one way that passed, for reference only) | `/private/tmp/claude-501/bnruns/bne68n09a4/twhome/teams/fixes3/afk/tmp/report-prototype.diff` |

## Units in order

Use the tdd skill for each unit; test only at these seams: `teamwire_report::on_turn_end` (the public hook entry, through `tests/result_file.rs`), and the existing unit tests of `session_usage` in `usage.rs`.

If a check fails and the cause is not clear from its output, use the diagnosing-bugs skill before your second fix attempt.

Commit after each unit.

1. **Share the rule.** Move the check from the usage loop into `read_transcript` in `usage.rs`, and make the loop call it. Behaviour does not change. The test is the 5 existing unit tests in `usage.rs` and the usage tests in `tests/turn_end.rs` (`delivering_event_carries_the_sessions_token_usage`, `delivery_still_happens_when_the_transcript_is_missing`, `a_transcript_that_is_a_named_pipe_is_never_opened`). They stay green. Commit.
2. **The fallback uses the rule.** First write the three tests from "Done when" in `tests/result_file.rs`, and see the pipe test and the 64 MiB + 1 test fail. Then make `last_assistant_text` read through `crate::usage::read_transcript(path, 0)`. All three pass. Commit.

## Files you own and files owned by others

You own `crates/teamwire-report/`, the whole crate. You will change `src/usage.rs`, `src/final_message.rs` and `tests/result_file.rs`. These files read or pin what you change, and you own them too: `src/turn_end.rs` (calls `pick` and `session_usage`), `tests/turn_end.rs` (pins the pipe case for usage), and `tests/support/kit.rs` (the hook input). Edit them only if a unit needs it.

Owned by others: `msg` owns `crates/teamwire-msg/`, `docs/api/cli.md`, `docs/api/mcp.md` and `crates/teamwire-cli/tests/preset.rs`. `ui` owns `ui/src/web/model/teams.ts`, `ui/src/web/model/teams.test.ts`, `ui/src/web/Overview.tsx` and `ui/src/web/Overview.test.ts`. Touch nothing else in the repository.

## Verification

Run from your worktree root.

While working:

```bash
cargo test -p teamwire-report --locked --lib usage
cargo test -p teamwire-report --locked --test result_file
cargo test -p teamwire-report --locked --test turn_end
```

Before you finish, run both of these. Each must pass:

```bash
cargo test -p teamwire-report --locked
TEAMWIRE_BASE=e78c32be scripts/gate.sh fast
```

The second command is required by the task. Its output must show no `WEAKENED` and no `PROTECTED-CHANGED` line. Do not run the full suite or the repository-wide gates: not `scripts/gate.sh` without `fast`, not `scripts/test-changed.sh`, and not `cargo test --workspace`. Your `check` is the two commands above joined with `&&`, and `teamwire verify` runs it.

## How to finish

1. Commit all work on `fixes3/report`. Do not push, and do not open a pull request. The repository has no remote.
2. Write your report file, the path your launch contract gives (`teams/fixes3/reports/report.md`). Give one progress line per commit with its sha. List the exact commands you ran with their results, the test names you added, and the result of each test on `bench` before your fix. End with a **Decisions I made** list, or `Decisions I made: none`.
3. Run `teamwire done` as your last action.

## Open questions you must not answer

None.
