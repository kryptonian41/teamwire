# report (fixes3): fix 1, the Stop hook's final-message fallback

## Progress
- dcfb436 move the regular-file and size check into usage::read_transcript; session_usage calls it | left: fallback tests and fix
- 078aa59 last_assistant_text reads through usage::read_transcript(path, 0); three new tests in tests/result_file.rs | left: none

## What changed
- `crates/teamwire-report/src/usage.rs`: new `pub(crate) fn read_transcript(path, used) -> Option<(u64, Vec<u8>)>`. It holds the regular-file and size check that was inline in the `session_usage` loop, with the FIFO comment. `session_usage` calls it with the bytes counted so far. Its behaviour is unchanged.
- `crates/teamwire-report/src/final_message.rs:25`: the fallback reads through `crate::usage::read_transcript(path, 0)`. No other copy of the check exists.
- `crates/teamwire-report/tests/result_file.rs`: two helpers (`make_fifo`, `transcript_of_len`) and three tests. No new `#[expect(` line.

## Tests added, and their result on bench (e78c32be) before the fix
| Test | Before the fix | After the fix |
| --- | --- | --- |
| `a_transcript_that_is_a_named_pipe_gives_no_final_message` | FAILED: panicked at result_file.rs:222, "the hook blocked on the pipe" after the 10 s timeout | ok |
| `a_transcript_past_64_mib_gives_no_final_message` | FAILED: outbox read `Final message from: transcript` | ok |
| `a_transcript_of_exactly_64_mib_is_read` | ok | ok |

## Commands run
- `cargo test -p teamwire-report --locked --lib usage`: 5 passed (after unit 1).
- `cargo test -p teamwire-report --locked --test turn_end`: 22 passed, including the three usage tests the brief names (after unit 1).
- `cargo test -p teamwire-report --locked --test result_file`: 7 passed, 2 failed before the fix (table above); 9 passed after.
- `cargo test -p teamwire-report --locked`: every test binary passed on 078aa59.
- `TEAMWIRE_BASE=e78c32be scripts/gate.sh fast`: exit 0. fmt ok, tidy ok, clippy ok. No `WEAKENED` and no `PROTECTED-CHANGED` line. Output saved at /private/tmp/claude-501/bnruns/bne68n09a4/twhome/teams/fixes3/afk/report-gate.txt.

## Decisions I made
- The two large-file tests share one small helper, `transcript_of_len`, instead of repeating the sparse-file code from the prototype. It builds on the file's existing `transcript` helper.
- The 64 MiB test also asserts `Final message from: transcript` in the outbox, beside the text and `final_source`, to match the neighbouring fallback test.
- The doc comment of `read_transcript` names the three refusal cases. The FIFO comment moved inside the function body, next to the check it explains.
