# Plan: three fixes in teamwire (team fixes3)

## Summary

Base `bench` at e78c32be. Three implementers work in parallel, one per fix. No two of them share a file.

| Member | Files to change | Helper to reuse | New tests (where) |
| --- | --- | --- | --- |
| report | `crates/teamwire-report/src/usage.rs`, `src/final_message.rs` | the usage reader's regular-file and `MAX_TRANSCRIPT_BYTES` check, moved into `read_transcript(path, used)` in usage.rs (D3) | named pipe, 64 MiB + 1, exactly 64 MiB, all through `on_turn_end` (`tests/result_file.rs`) |
| msg | `crates/teamwire-msg/src/remote.rs`, `docs/api/cli.md`, `docs/api/mcp.md` | `outcome_of`, `gone`, `mirror` and `remote_of` in remote.rs, and the store's `ReadTx::delivery` | handed off, read, pending (closing and closed), all through `resend` (`tests/cross_team.rs`) |
| ui | `ui/src/web/model/teams.ts`, `ui/src/web/Overview.tsx` | `livenessInfo` (teams.ts:189) for the lead label and category | `problemReasons` and `hasProblem` (`teams.test.ts`), the card (new `Overview.test.ts`) |

Other files that read or pin what changes. Each one is inside its member's `owns`.
- report: `src/turn_end.rs` calls `final_message::pick` (lines 156, 237) and `session_usage` (291). The 5 unit tests in `src/usage.rs` pin the usage rule. `tests/turn_end.rs:474-496` pins "a pipe is never opened" for usage only. `tests/result_file.rs:71-99` pins the fallback and `Final message from: none`. `tests/support/kit.rs:96-105` builds the hook input with a final message, so every existing test skipped the fallback.
- msg: `src/resend.rs:106-123` and `src/send.rs:131-151` call `deliver_to_team`. `tests/cross_team.rs:432-444` pins "a missing copy stays undelivered", and it must stay green unchanged. `tests/resend.rs:338` pins the refusal when the sender's own team closes, which is unchanged. `crates/teamwire-cli/tests/preset.rs:419-434` reads `docs/api/mcp.md` headings only.
- ui: `Overview.tsx:15-17,54` reads `hasProblem` and `sortTeams`. `teams.test.ts:39-46` pins the sort order, and line 42 must change (D9). The CLI JSON does not change, so `ui/src/shared/teamwire.ts` does not change.

Checks (the `check` field). All three passed on a fresh clone of `bench`. The table is below.
- report: `cargo test -p teamwire-report --locked && TEAMWIRE_BASE=e78c32be scripts/gate.sh fast`, setup `""`
- msg: `cargo test -p teamwire-msg --locked && TEAMWIRE_BASE=e78c32be scripts/gate.sh fast`, setup `""`
- ui: `cd ui && bun test && bun run typecheck`, setup `cd "$1/ui" && bun install --frozen-lockfile`

Every fix was prototyped on a temporary clone. Each new test failed on `bench` and passed with the fix, and the fast gate passed. The prototype diffs are in `afk/tmp/*-prototype.diff` for reference.

Known environment failure: `herdr_env_wins_over_tmux` (teamwire-cli) fails on unchanged `bench` from these sessions. The lead was told (message m001).

## Fix 1: the final-message fallback (member `report`)

### What is wrong
- The Stop hook picks the final message with `final_message::pick` (`crates/teamwire-report/src/final_message.rs:10-20`). It uses the hook's own field first. With no field, it reads the transcript in `last_assistant_text`, which calls `std::fs::read(path)` with no check (line 25).
- The usage reader (`usage.rs:44-59`) checks `metadata.is_file()` and the 64 MiB sum before it reads.
- `on_turn_end` calls `pick` in `deliver` (turn_end.rs:237) and in `late_final` (turn_end.rs:156). The pick runs before the usage read (turn_end.rs:288-292). So on `bench` a named pipe blocks the hook before the usage reader can refuse it.

### Change
1. In usage.rs, move the check into `pub(crate) fn read_transcript(path: &Path, used: u64) -> Option<(u64, Vec<u8>)>`. The function returns the length and bytes only for a regular file whose length keeps `used + len <= MAX_TRANSCRIPT_BYTES`. The loop in `session_usage` calls `read_transcript(file, size)`. The comment about a FIFO moves to the function's doc comment.
2. In final_message.rs, `last_assistant_text` gets its bytes from `crate::usage::read_transcript(path, 0)?`.
3. `pick` already returns `(None, FinalSource::None)` when the text is `None`. `deliver` then keeps a crash left-over's stored message, or `None` for a new completion (done.rs:200-201). So nothing else changes.

### Tests, in `crates/teamwire-report/tests/result_file.rs`
- A named-pipe transcript with `final_message = None`. The test runs the body in a thread and waits with `recv_timeout(10 s)`, as `tests/turn_end.rs:474-496` does, so a regression fails instead of hanging. It asserts `Final message from: none` in the outbox file and `FinalSource::None`.
- A transcript holding the `transcript()` helper's lines, then `"\n"`, then extended with `set_len(64 * 1024 * 1024 + 1)` (a sparse file, no disk use). It asserts `none` and `FinalSource::None`. On `bench` this test fails with `Final message from: transcript`.
- The same with `set_len(64 * 1024 * 1024)`. It asserts the last assistant text and `FinalSource::Transcript`, which pins "at most".

## Fix 2: resend to a closing or closed team (member `msg`)

### What is wrong
- `deliver_to_team` (`crates/teamwire-msg/src/remote.rs:37-82`) opens the other team's store and calls `receive`. `receive` calls `Tx::insert_received` (`crates/teamwire-store/src/received.rs:37-44`), which returns `None` when that team is not open.
- On `None`, every try is mirrored as `gone(team)`, which is undelivered with `TeamClosed` (remote.rs:66-71, 174-183). The code never reads the copy that team already holds.

### Change
In the `Ok(None)` branch, for each try, read the receiving team's row with `there.store.read(|tx| tx.delivery(&message.ulid, &local))`. The recipient name `local` comes from `remote_of(...).map(Remote::local)`, as in `deliver_there`. Then map the row like this.

| Row there | Outcome passed to `mirror` |
| --- | --- |
| `HandedOff` or `Acked` | `outcome_of(&row)`, which turns `Acked` into `HandedOff` |
| `Pending` or `Unconfirmed` (D4) | `Unconfirmed`, the row's route, error `TeamClosed` with a detail that says an earlier attempt may have reached it |
| `Undelivered` (D4), or no row | `gone(team)` |

`mirror` (remote.rs:189-232) then writes the sender's row and keeps its forward-only rules. A read is allowed while the team is closing. Nothing is typed, because `deliver_one` is never called on this branch. Also update the module comment (remote.rs:10-13) and add one sentence to `docs/api/cli.md:344-351` and `docs/api/mcp.md:110-114` (D13).

### Tests, in `crates/teamwire-msg/tests/cross_team.rs`
Use `stored_in_alpha_only`. Write beta's copy as `a_resend_after_the_line_arrived_only_records_it` does (lines 391-429), with `insert_received` and then `set_delivery_state`, or `ack_delivery` for a read row (`set_delivery_state` refuses `Acked`, delivery.rs:129). Close beta with `begin_close`, and add `finish_close(at, &teamwire_store::CloseEnd::Completed)` for "closed". Advance the clock 61 s, then call `resend`.
- Closing team, copy handed off: report `HandedOff`, reason `None`, alpha's row `handed_off`, `t.typed("db")` empty.
- Closed team, copy read (`acked`): the same.
- Closing team, copy pending: report `Unconfirmed`, reason `Some(TeamClosed)`, alpha's row `unconfirmed`, nothing typed.
- The existing `a_team_that_closed_after_the_send_was_stored_leaves_it_undelivered` covers the missing copy. It stays unchanged.

## Fix 3: problem reasons on the dashboard (member `ui`)

### What is wrong
`hasProblem` (`ui/src/web/model/teams.ts:54-64`) looks only at errors, stuck deliveries, undelivered results and member categories. The lead's liveness, the watch, the AFK budget, unread deliveries and waiting questions are ignored.

### Change
1. Export `problemReasons(t: TeamModel): string[]` from teams.ts. While `t.status.state === "open"`, it adds three labels in this order. The first is `livenessInfo(s.lead).label` when its category is `failed` or `attention`. The second is `watch stale` when `s.watch.state === "stale"`. The third is `AFK spent` when `s.afk?.spent === true`. At any state, it then adds `${n} unread` for `s.unread_deliveries.length > 0`. Last, it adds `${n} question waiting` for the questions in `s.unanswered_questions` whose `to` includes `"lead"` and whose `age_s >= 600` (D6, D7).
2. `hasProblem` gets `|| problemReasons(t).length > 0`.
3. In `TeamRow` (Overview.tsx:50-109), render each reason as `<span className="flag">` inside `<p className="chips">` in the card body, before the stuck-delivery flags (D8).
4. Change `teams.test.ts:42` so the "quiet" team also clears `unread_deliveries` and `unanswered_questions` (D9).

### Tests
- `ui/src/web/model/teams.test.ts`: one test per label, the order with all five present, the three open-only labels dropped for `closing`, `closed` and `null` states, a question under 600 s or to another party not counted, and `hasProblem` true from a reason alone. Build each input by spreading a fixture status with overrides. Never edit `ui/fixtures/`.
- `ui/src/web/Overview.test.ts` (new): render `Overview` with `renderToStaticMarkup(createElement(Overview, {...}))` for at least two teams with different reasons. Take each named team's card as its `<a class="team…" href="#/team/<name>">…</a>` element (D15). Check that each reason appears on its own team's card and on no other team's card. On the fixtures, `problemReasons` gives `[["1 unread", "1 question waiting"], ["watch stale"], []]` for payments, docs and lost. So payments' card holds the first two, docs' card holds `watch stale`, and lost's card holds none.
- After the plan review, I ran this per-card test on a temporary clone with the ui prototype. It passed on the correct card. It failed when payments' reasons were also rendered on the docs card. It failed when the reasons were rendered only in the page heading. With it, `bun test src/web` gave 46 pass and 0 fail, and `bun run typecheck` exited 0. Splitting the page on `<li>` does not work, because the `lost` card lists its errors as `<li>` items. The tested file is `afk/tmp/Overview.per-card.test.ts`. The whole-page test in `afk/tmp/ui-prototype.diff` is the weaker one the reviewer flagged, so do not copy it.

## Planning check results (fresh clone of `bench`)

Method: `git clone` of the checkout into `afk/tmp/<role>`, then `git checkout bench` (HEAD e78c32be). Setup ran as launch runs it: `sh -c <setup> teamwire-setup <path>` in `<path>` (crates/teamwire-launch/src/member_steps.rs:202-208). The check ran as verify runs it: `sh -c <check>` in `<path>` (crates/teamwire-verify/src/check.rs:38). Script: `afk/tmp/fresh_check.sh` (`bash afk/tmp/fresh_check.sh <role> <setup> <check>`). Logs: `afk/tmp/<role>.log`, results: `afk/tmp/<role>.result`. The three clones were deleted after the checks and the prototype runs. The prototype diffs and logs are kept in `afk/tmp/`.

| Member | setup exit | check exit | Check time | What ran |
| --- | --- | --- | --- | --- |
| report | 0 (empty) | 0 | 36 s | 8 test binaries of teamwire-report, all ok. Gate: fmt ok, tidy ok, clippy ok. |
| msg | 0 (empty) | 0 | 34 s | 11 test binaries of teamwire-msg, all ok. Gate: fmt ok, tidy ok, clippy ok. |
| ui | 0 | 0 | 5 s | `bun install`: 14 packages. `bun test`: 163 pass, 3 skip, 0 fail. `tsc --noEmit`: ok. |

The times include a full compile from an empty target directory on this 18-core machine.

## Prototype results (same temporary clones, after the checks)

| Fix | On `bench` | With the prototype fix |
| --- | --- | --- |
| 1 | pipe test: no answer after 10 s, `expect("the hook blocked on the pipe")` failed. 64 MiB + 1: `Final message from: transcript`. | all 9 tests in result_file.rs pass. All teamwire-report tests pass. Fast gate ok. |
| 2 | handed off, read, pending: each reported `(Undelivered, Some(TeamClosed), Undelivered, 0)` | the 3 probes pass, all 15 cross_team tests pass, all teamwire-msg tests pass. Fast gate ok. |
| 3 | `sorts problem teams first`: expected `["Zzz","Alpha","Zed"]`, received `["Alpha","Zed","Zzz"]` once `hasProblem` used the reasons | 47 tests in `src/web` pass after D9. `tsc` exit 0. A render test sees all three reasons. |

Dependents (teamwire-cli, -mcp, -e2e, -crash-tests) on both Rust prototypes: see "Dependents run" below.

## Mistakes my probes made (so the implementers skip them)

| Attempt | Result | Do this instead |
| --- | --- | --- |
| Pad the transcript with `set_len` right after the last line | The NUL bytes join the last JSON line, which no longer parses. The "exactly 64 MiB" test then reads `Early line`. | Write `"\n"` before `set_len`. |
| Add `make_fifo` to result_file.rs with its own `#[expect(clippy::disallowed_methods)]` | tidy prints `WEAKENED crates/teamwire-report/tests/result_file.rs:183` | Add no `#[expect`. The file's own `#![expect(clippy::disallowed_methods)]` (lines 3-8) already covers `Command::new("mkfifo")`. Clippy passes. |
| `other => tx.set_delivery_state(...)` in a test helper | clippy `wildcard_enum_match_arm` fails the gate, in tests too | Name the variants: `other @ (DeliveryState::HandedOff \| DeliveryState::Unconfirmed \| DeliveryState::Undelivered)`. |

## Dependents run

I ran the commands that `scripts/test-changed.sh <crate>` prints, with `--no-fail-fast` added, on each Rust prototype clone after its build:

```bash
TEAMWIRE_FAKES_PREBUILT=1 cargo test --locked --no-fail-fast -p teamwire-cli -p teamwire-crash-tests -p teamwire-e2e -p teamwire-mcp -p teamwire-msg     # msg clone
TEAMWIRE_FAKES_PREBUILT=1 cargo test --locked --no-fail-fast -p teamwire-cli -p teamwire-crash-tests -p teamwire-e2e -p teamwire-mcp -p teamwire-report  # report clone
```

| Prototype | Test binaries | Passed | Failed | The failing test |
| --- | --- | --- | --- | --- |
| msg | 63 | 502 | 1 | `herdr_env_wins_over_tmux` |
| report | 60 | 495 | 1 | `herdr_env_wins_over_tmux` |

`herdr_env_wins_over_tmux` (`crates/teamwire-cli/tests/tmux_wiring.rs:373-378`) also fails on unchanged Rust code. I ran `cargo test -p teamwire-cli --locked --test tmux_wiring` in the ui clone, whose crates match `bench`, and got the same failure (`afk/tmp/bench-tmux-wiring.log`): `could not run tmux: not_found ... left: Some(1) right: Some(0)`. The test's own comment (lines 370-372) says it holds only when no `tmux` or `herdr` process is among the test's parents. This session runs with `TMUX` and `HERDR_ENV` set, under both. So the failure comes from the environment, not from either fix. A first run without `--no-fail-fast` stopped at this test binary, which is why I ran again.

Grep for dependents that could see the change: `grep -rln resend crates/teamwire-cli/tests crates/teamwire-mcp/tests tests/e2e tests/crash` lists `crates/teamwire-cli/tests/send.rs`, `crates/teamwire-mcp/tests/tools.rs` and `tests/crash/tests/kill_send.rs`. None of them closes a receiving team. `grep -rln transcript` over the same folders finds nothing.

## Spec validation

`afk/spec-wave1.json` starts from `teamwire preset parallel-tickets`. The team fixes3 already exists, because the planner runs in it, so the plain dry run refuses:

```text
$ teamwire launch --dry-run --spec afk/spec-wave1.json
refused: team_exists: team fixes3 already exists; pass --add to extend it, pick another name, or close it first
exit 1
```

The lead will add the members with `--add`, and that form passes:

```text
$ teamwire launch --add --dry-run --spec afk/spec-wave1.json
dry run: would add to team fixes3
  report (implementer, worktree) worktree .../clone/.worktrees/fixes3-report on branch fixes3/report via tmux, identity by harness_config
  msg (implementer, worktree) worktree .../clone/.worktrees/fixes3-msg on branch fixes3/msg via tmux, identity by harness_config
  ui (implementer, worktree) worktree .../clone/.worktrees/fixes3-ui on branch fixes3/ui via tmux, identity by harness_config
nothing was created
exit 0
```

A copy that differs only in `"team": "fixes3dry"` (`afk/tmp/spec-wave1-newteam.json`) passes the plain `teamwire launch --dry-run --spec`, with exit 0, and `teamwire teams` lists no `fixes3dry` afterwards. So every field also validates for a new team.

The `owns` lists do not overlap. They are `crates/teamwire-report/` for report. For msg they are `crates/teamwire-msg/`, `docs/api/cli.md`, `docs/api/mcp.md` and `crates/teamwire-cli/tests/preset.rs`. For ui they are `ui/src/web/model/teams.ts`, `teams.test.ts`, `ui/src/web/Overview.tsx` and `Overview.test.ts`.

## Ruled out

- A schema change in `ui/src/shared/teamwire.ts`: every field the reasons need is already parsed (`watch` line 137, `afk` line 139, `unread_deliveries` line 144, `unanswered_questions` line 145).
- A lint step for ui: `ui/package.json` has only `start`, `dev`, `test` and `typecheck`. The gate never enters `ui/`.
- A new dependency for the named pipe: the report tests already make one with `mkfifo` (tests/turn_end.rs:462-472). `nix` is not a dependency of teamwire-report.
- A doc change for fix 1: `docs/api/core.md:181` describes the usage reader, whose behaviour stays the same. ADR 0007 is protected.
- Another reader of `hasProblem` or `sortTeams`: `grep -rn "hasProblem\|sortTeams" ui/src` finds only `teams.ts`, `Overview.tsx:3,15-17,54` and `teams.test.ts:6,40,45`.
- A file-size problem: tidy does not count files under `tests/` (xtask/src/sizes.rs:59-75). usage.rs grows by about 10 lines, to about 115 non-test lines.
