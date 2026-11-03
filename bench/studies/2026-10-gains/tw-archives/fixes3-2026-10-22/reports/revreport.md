# revreport (fixes3): review of branch fixes3/report

## Summary
1. Verdict: the branch is sound. I found no correctness, security or data-loss defect, and nothing that breaks the brief.
2. The fallback now reads the transcript through `usage::read_transcript(path, 0)` (`crates/teamwire-report/src/final_message.rs:25`). The usage reader calls the same function (`usage.rs:45`), and no other copy of the check is left.
3. All three new tests in `tests/result_file.rs` pass on the branch. With the old `final_message.rs` restored, the pipe test fails on its 10 s timeout and the 64 MiB + 1 test reads `Final message from: transcript`.
4. Two deliberate breaks of the shared check are each caught: `<=` changed to `<` fails the exactly-64 MiB test, and a dropped `is_file()` fails both pipe tests.
5. `cargo test -p teamwire-report --locked` passes (every binary), and `TEAMWIRE_BASE=e78c32be scripts/gate.sh fast` passes with no `WEAKENED` and no `PROTECTED-CHANGED` line.

## Progress
- Read the brief, the task, decisions.md, the member brief, the member report and the diff `e78c32be...fixes3/report` (3 files, +93 -7).
- Read the code around the diff: `usage.rs`, `final_message.rs`, `turn_end.rs` (both callers of `pick`, `late_final` at 150-169 and `deliver` at 226-295), `tests/result_file.rs`, `tests/turn_end.rs:462-496` and `tests/support/kit.rs:96-105`.
- Ran tests and the fast gate in a temporary clone at `/private/tmp/claude-501/bnruns/bne68n09a4/twhome/teams/fixes3/afk/tmp/revreport-clone`, checked out at 078aa59. I removed that clone after the run.

## Findings
None.

## What I checked, with the evidence

### The task's end-state sentences (fix 1)
| End-state sentence | True? | Evidence |
| --- | --- | --- |
| The fallback reads the transcript only when it is a regular file of at most 64 MiB | yes | `final_message.rs:25` calls `read_transcript(path, 0)`. `usage.rs:104-107` returns `None` unless `is_file()` and `0 + len <= MAX_TRANSCRIPT_BYTES`. |
| The fallback uses the same rule as the usage reader | yes | Both readers call `read_transcript` (`usage.rs:45`, `final_message.rs:25`). The expression in `read_transcript` is the old inline one with `size` renamed `used`, so the usage reader behaves the same. |
| A named pipe: the hook still completes through `on_turn_end` | yes, tested | `a_transcript_that_is_a_named_pipe_gives_no_final_message`. The helper `finish` asserts `TurnEndReply::Continue`, inside a thread with a 10 s `recv_timeout`. |
| A named pipe: the result file says `Final message from: none` and `final_source` is `FinalSource::None` | yes, tested | Same test, `result_file.rs:223-224`. |
| A file past the limit: the same three facts | yes, tested | `a_transcript_past_64_mib_gives_no_final_message`, file length 64 MiB + 1. |

The brief's extra boundary test, `a_transcript_of_exactly_64_mib_is_read`, also passes. That test shows the limit is inclusive and the text is still found.

### Each new test fails without its fix
I ran the tests in the clone with only `final_message.rs` put back to its bench version. The new tests stayed in place.

```
test a_transcript_past_64_mib_gives_no_final_message ... FAILED
  panicked at crates/teamwire-report/tests/result_file.rs:239:5:  ... - Final message from: transcript
test a_transcript_that_is_a_named_pipe_gives_no_final_message ... FAILED
  panicked at crates/teamwire-report/tests/result_file.rs:222:10:  (the hook blocked on the pipe)
test a_transcript_of_exactly_64_mib_is_read ... ok
test result: FAILED. 7 passed; 2 failed; finished in 10.01s
```

The exactly-64 MiB test passes on bench too. That is expected, because bench read every file. The break below shows that this test catches an off-by-one in the limit.

### Breaking the shared check on purpose
| Break in `usage.rs:106` | Result |
| --- | --- |
| `<= MAX_TRANSCRIPT_BYTES` changed to `<` | `a_transcript_of_exactly_64_mib_is_read` FAILED at `result_file.rs:255`. The 8 unit tests still pass. |
| `m.is_file() &&` removed | `a_transcript_that_is_a_named_pipe_gives_no_final_message` FAILED (timeout, `result_file.rs:222`). The existing `a_transcript_that_is_a_named_pipe_is_never_opened` FAILED too (`turn_end.rs:493`). |

So no test passes because of hard-coded or stubbed code.

### Paths through the code
- `pick` runs in two places: `late_final` (`turn_end.rs:156`) and `deliver` (`turn_end.rs:237`). Both now go through the check, because the check sits inside `last_assistant_text`.
- In the pipe test, `deliver` also passes the same pipe path to `session_usage` (`turn_end.rs:288-291`). That reader already refused a pipe, so the test does not hang there.
- `std::fs::metadata` follows a symlink. A symlink to a pipe is refused, and a symlink to a regular file is read. The usage reader worked the same way before this branch.

### The brief and the repository rules
- The brief's D3 asks for `pub(crate) fn read_transcript(path: &Path, used: u64) -> Option<(u64, Vec<u8>)>` in `usage.rs`. The branch has that function at `usage.rs:102`. `MAX_TRANSCRIPT_BYTES` stays at `usage.rs:11`, and the FIFO comment moved to `usage.rs:103`.
- The brief says "Add no new `#[expect(` line there." The diff adds none. The file-level `#![expect(clippy::disallowed_methods)]` covers the new `mkfifo` call and `std::fs::OpenOptions`.
- Large files are built sparsely, with a `"\n"` before `set_len` (`result_file.rs:194-202`), as the brief asks.
- The branch touches only three files, all inside `crates/teamwire-report/`. No `Cargo.lock`, no protected file, no `unwrap` in `src/`.
- `tests/result_file.rs` is now 261 lines, under the 400-line split rule.

### Commands I ran (in the temporary clone, at 078aa59)
| Command | Result |
| --- | --- |
| `cargo test -p teamwire-report --locked` | every binary ok. lib 8, done 8, gate 19, lead_stop 21, nohost 3, reminder 5, result_file 9, turn_end 22 |
| same tests with `final_message.rs` from bench, `--test result_file` | 7 passed, 2 failed (above) |
| same with `<` in place of `<=` | exactly-64 MiB test failed |
| same with `is_file()` removed, `--no-fail-fast` | both pipe tests failed on timeout |
| `TEAMWIRE_BASE=e78c32be scripts/gate.sh fast` (clone restored to 078aa59) | fmt ok, tidy ok, clippy ok. No `WEAKENED`, no `PROTECTED-CHANGED`. |

The member report says the same results. Its claims match what I saw.

### Checked and not reported
- The new `make_fifo` in `tests/result_file.rs:183-189` copies the helper at `tests/turn_end.rs:466-472`. I found no failure from the copy. The brief put the tests in this file and expected the `mkfifo` call there, so I am not reporting the copy.
- `crates/teamwire-harness-claude/src/cwd.rs` also reads a transcript, to find a working directory. That file belongs to another crate and is not the final-message fallback, so the task does not cover it.

## Questions for the parent
None.

## Decisions I made
- I removed my temporary clone at `afk/tmp/revreport-clone` after the run, to free its build directory. Nothing else under `afk/tmp/` was touched.
- I did not report the copied `make_fifo` test helper, because it has no failure and the brief put the tests in that file.
