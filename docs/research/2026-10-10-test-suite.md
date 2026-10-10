# The test suite: what is known, and a plan to make it faster

Written 2026-10-10 by a lead session that ran a six-member read-only teamwire
team (`twtest`) and took its own measurements. The team's full reports are in
`~/.teamwire/archive/twtest-2026-10-10/reports/`: `history.md`,
`practice.md`, and the four test reviews `rv_cli.md`, `rv_flow.md`,
`rv_core.md` and `rv_adapt.md`. Nothing in the repository was changed by this
work except this file.

## Status, 2026-10-11

| Item | Result |
| --- | --- |
| Phase 0: commit the flake fixes | Done: `d82a69f` (tests) and `3b6c656` (gate and docs). |
| `cargo clean` | Done. It freed 14.2 GB, and a full rebuild took 95 s. Process starts did not get faster (see section 2). |
| Quit Zoom | Done by the user. Starts still cost 0.25 s. A SentinelOne `log show` process then ran at 100% CPU. |
| Phase 6: prune tests | Done for the entries that hold up. The suite has 16 fewer test functions. Six were dropped, and ten were folded into tests that remain. See the list below. |
| Phase 1, step 3 (Developer Tools) | Done by the user at 00:55 on 2026-10-11. Starts still cost 0.25 s afterwards. |
| Restart the Mac | Done by the user at about 01:47 on 2026-10-11. Starts became fast again. See "After the restart" in section 2. |
| Phase 1, step 4 (SentinelOne) | Open. Ask about it only if slow starts come back. |
| Phase 1, step 5 (gate twice with fast starts) | Done. Step 4 of the gate took 173 s and 177 s, down from 305 to 355 s. |
| Phase 2, step 1 (fake agents exit when their owner dies) | Done in `c6a4f34`. |
| Phase 5 (nextest, three runs) | Measured, gate not switched. All three runs passed: 152 s, 238 s and 157 s. Runs 2 and 3 overlapped another session's test run. The 23 end-to-end and crash tests run one at a time and add up to 129 s, which sets the floor for the whole run. nextest saves about 20 s, and about 12 s of that comes from skipping doctests. |

Tests removed. For each one, the behaviour it checked is still checked:

| Removed | Still checked by |
| --- | --- |
| `cli/tests/tmux_wiring.rs` `neither_herdr_nor_tmux_leaves_the_lead_with_no_host` | `cli/src/wiring.rs` unit tests (host choice) and `cli/tests/launch.rs` `a_member_on_host_none_is_refused_and_nothing_is_created`, which now also checks the refusal text |
| `cli/tests/preset.rs`: six `*_is_a_valid_spec` tests | One `every_preset_is_a_valid_spec` loop that shares one git repository |
| `mcp/tests/tools.rs` `a_team_that_does_not_exist_is_refused_as_identity` | `a_team_with_no_database_names_the_archive_but_stays_an_identity_refusal` |
| `mcp/tests/tools.rs` `a_lead_server_with_the_wrong_session_does_not_blame_the_session_id` | Folded into `a_lead_call_without_the_recorded_session_is_refused` |
| `mcp/tests/tools.rs` `tool_list_carries_the_cache_fields_a_2026_client_requires` | Folded into `tool_list_is_the_eight_tools_of_the_doc` |
| `store/tests/migrate.rs` `read_only_open_by_path_refuses_a_missing_file` | `read_only_open_never_migrates`. `open_read_only` calls `open_read_only_at` directly. |
| `msg/tests/send.rs` `a_token_hash_matches_what_launch_stores` | Nothing needed: it compared the test rig with itself. `identity.rs` proves the real hash check. |
| `msg/tests/resend.rs` `delivery_updates_keep_forward_only_moves_in_the_store_too` | `store/src/delivery/tests.rs` `states_never_move_backward` |
| `fakes/tests/claude.rs` `saying_no_to_the_dialog_ends_the_session` | `enter_alone_takes_no_exit_and_never_trusts`, which has the same steps plus one more check |
| `harness-claude/tests/launch_spec.rs`: four `settings_env_has_*` tests | One `settings_env_has_team_role_home_and_token` |

Proposed merges that were not made, and why:

- The merges in `launch/dispatch.rs`, `watch/poll.rs`, `deliver/retry.rs` and
  `core/sanitize.rs` touch tests that run in memory. Merging them saves no
  time and only rewrites working tests.
- `msg/identity.rs`: the two tests take different paths. One is a lead with
  no session id, and the other is a member with the wrong token.
- `msg/send.rs` exact-body test: it sends a body with two lines and a dash.
  Folding it into another test would lose that case.
- `close/archive.rs:157`, `cli/tmux_wiring.rs:317`,
  `store/migrate.rs:84` and `msg/reprompt.rs:117`: the reviewers themselves
  marked these as uncertain or low value.

A flaky test found in this pass: `fakes/tests/claude.rs`
`session_file_status_values_are_the_ones_claude_code_writes` failed in every
full run today. Alone it passes in 5.4 to 5.6 s, against a 5 s limit for
reaching "busy" that is counted from the prompt. The prompt takes about
3.5 s to arrive because it passes through several slow process starts. The
fix is faster process starts (phase 1). Raising the limit would loosen the
test.

## 1. What the earlier sessions did

Three Claude Code sessions worked on test speed on 2026-10-10. The session
`eccca834` did the work. The session `7d97f2b5` summarised it and
investigated leftover fake agents. The session `b184487e` ran full gates at
the same time while it built `teamwire install`.

1. The sessions measured step 4 of the gate (`cargo test --workspace`) at
   305 to 355 s warm. Step 4 is almost the whole gate.
2. They fixed five flaky tests. The `close` tests now wait for the fake
   agent's status to say `done`. The two platform timeout tests now wait
   3 s. The tmux test servers now ignore `~/.tmux.conf`. The
   special-characters tmux test now polls for its echo. And
   `TEAMWIRE_FAKES_PREBUILT=1` stops each test process from rebuilding the
   fake binaries.
3. They tried nextest in the gate. Four gate runs failed, each in a
   different test. The user chose to go back to `cargo test`. nextest is
   still installed, and `.config/nextest.toml` exists but is unused.
4. A cold gate passed in 683 s, and a warm test step passed in 310 s. The
   fixes made the suite reliable. They did not make it faster.
5. **None of this is committed.** Both sessions ended by asking whether to
   commit, and neither question has an answer.

## 2. Causes: confirmed, and not confirmed

### Confirmed

| Cause | Evidence |
| --- | --- |
| At times, every process start on this Mac costs about 0.25 s, and a newly built binary's first start costs 0.5 to 4.5 s. | Measured at 23:52 after `cargo clean`. `/bin/echo` took 0.25 s on each of four starts, and a tiny C program in an empty temp folder took 0.56 s, then 0.25 s. At 21:00 the same `/bin/echo` took 0.00 s. So the cost comes and goes, and it applies to every program, not only to test binaries. |
| A trivial test costs about 1.8 s under nextest. The median test took 1.78 s in a warm run, and only 34 of 1,173 tests took under 0.5 s. | `nextest-warm.log`, today. A pure in-memory test should take milliseconds. Each test under nextest is a separate start of its binary, so each one pays the start cost above, plus waiting for a CPU. |
| The `close` test race with the fake agent's Stop hook. | Reproduced by session `eccca834` with an injected delay: 7 or 8 failures of 12 before the fix, 0 of 12 after. |
| Per-process rebuilding of the fakes broke parallel runs. | 67 of 120 runs failed before `TEAMWIRE_FAKES_PREBUILT=1`, and 0 of 120 after. |
| `~/.tmux.conf` makes each test tmux server start about 20 times slower. | 520 ms with the user's config, 25 ms with `-f /dev/null`. |
| A test process killed by a signal leaves its fake agents running for good. | Reproduced by `7d97f2b5`: 9 agents before the kill and 9 after, each with parent PID 1. It happened again today. A crash test hit nextest's 240 s limit and left one `fake-claude` behind. |
| Doctests cost about 12 s and run zero tests. | Measured by `eccca834`: `0 doctests run`. Only one fenced block exists, and it is a `text` block. |
| The slow tests are the ones that build a git repository or start the fake terminal host. | All four reviews agree. The worst groups are `teamwire-verify` (its fixture runs `git init`, three `git config` calls and a commit per test), the `close` and `launch` rigs, and `teamwire-cli`'s `close.rs`, `verify.rs` and `preset.rs`. |
| Each end-to-end and crash test process runs `cargo build` for the CLI again. | `tests/e2e/src/bin.rs:12` and `tests/crash/tests/support/rig.rs:72` cache the build in a `OnceLock`, which lasts only one process. `TEAMWIRE_FAKES_PREBUILT` does not cover this build. |

### Not confirmed, or refuted

| Claim | Status |
| --- | --- |
| "The macOS program check (`syspolicyd`) is the bottleneck." | Not proven. It rested on one `ps` snapshot taken while two sessions ran tests. Which process adds the 0.25 s to each start is unknown. The candidates are `syspolicyd` and `XprotectService`, the SentinelOne agent, and a `log stream` process that Zoom keeps at 100% CPU from boot (pid 1566, parent `zoom.us`). A restart removed the cost. The SentinelOne `log show` process at 100% CPU was gone after the restart, which makes it the leading suspect. That is a correlation only. |
| "The size of `target/debug/deps` causes the slow starts." | **Refuted.** At 21:00 a binary in that folder (284,534 files) cost 0.25 s per start, while copies in small folders cost 0.00 s. A temp folder filled with 100,000 empty files reproduced the cost. After `cargo clean`, the folder held 12,963 files and the cost was unchanged, and `/bin/echo` paid it too. The earlier tests happened to line up with the machine-wide cost coming and going. `cargo clean` still freed 14.2 GB. |
| "A fresh build folder makes the suite fast." | Not shown. One run built into a fresh folder at `/private/tmp`. Nine tests, most of them git-heavy `verify` tests, hit the 240 s limit, and the run was stopped. The timeouts are not explained. |
| "Leftover fake agents load the machine." | Refuted. Each one uses about 0.1% CPU. |
| "Slow process start makes the two platform timeout tests fail." | Unproven. The 3 s fix hides the cause and costs about 2.5 s. |
| "nextest saves 70 to 115 s." | Rough. Measured in a private clone under load, and nextest never passed a full gate on the shared checkout. |

### After the restart, 2026-10-11

The user restarted the Mac at about 01:47. The measurements below were taken
between 01:59 and 02:04, 12 to 17 minutes after boot. The machine was still
doing its boot work during that time. `dasd`, the process that schedules
background jobs, ran at 100% CPU, and the load average stayed between 3.9
and 6.5.

| Measure | Before the restart | After the restart |
| --- | --- | --- |
| 50 starts of `/bin/echo` | about 12 s (0.25 s each) | 0.14 s and 0.12 s (about 3 ms each) |
| First start of `teamwire_store`'s test binary after boot | 0.5 to 4.5 s for a new binary | 0.19 s |
| Later starts of the same binary (`--list`) | 0.25 s or more | 5 to 7 ms |
| Step 4 of the gate, `cargo test --workspace`, warm | 305 to 355 s | 173 s, then 177 s |
| Whole gate, all six steps | 683 s cold | about 187 s, twice |

Both gate runs passed every step. No other session was building or testing
during the second run.

What the machine looked like after the restart:

- The SentinelOne agent processes were running, each at 0% CPU. The
  SentinelOne `log show` process from the night before was gone.
- Zoom was not running.
- No `fake-claude` or `kill_send` processes were left over.
- Low power mode was off, and macOS had recorded no thermal or CPU warnings.

The restart fixed the symptom. The cause is still unknown, and the cost came
and went before the restart too, so it may come back. This command checks for
it in about a second:

```bash
s=$(gdate +%s.%N); for i in $(seq 50); do /bin/echo >/dev/null; done; e=$(gdate +%s.%N); echo "$e - $s" | bc
```

A healthy machine prints about 0.15. A machine with the slow starts prints
about 12. If the slow starts come back, check first for a process at 100%
CPU, and especially for a `log show` or `log stream` process.

The gate times above set the new starting point for phases 2 to 5. Those
phases should be measured against 173 to 177 s, not against 305 to 355 s.

## 3. Review of the tests

Four reviewers read all of the roughly 1,170 tests, crate by crate. A test
could be marked for removal only when the reviewer named the test that
already covers it, with a file and line.

| Review | Tests | Drop | Merge | Move to a unit test |
| --- | --- | --- | --- | --- |
| CLI, MCP, end-to-end, crash | 218 | 2 or 3 | 6 | 0 |
| launch, close, watch, report, deliver | 262 | 0 | 4 | 0 (1 needs a code change) |
| store, msg, verify, core | 288 | 3 | 5 | 4 |
| adapters, platform, otel, fakes | 288 | 1 | about 8 | 0 |

**The suite has very little dead weight.** About 30 tests, under 3% of the
suite, are clear drop or merge candidates, and they are cheap ones. Deleting
tests will not make the suite noticeably faster. The time is in how the
expensive tests are set up. The full lists, each with the covering test, are
in the four review reports.

## 4. What current Rust practice says (from `practice.md`)

| Practice | Source | Here today |
| --- | --- | --- |
| One integration test binary per crate (`tests/it/main.rs` with modules), not one per file | matklad, "Delete Cargo Integration Tests" | 108 binaries. Merging is easy in every crate. One special case: `platform/tests/process.rs:39` re-runs itself by test name. |
| Exempt the terminal from the macOS program check in Privacy & Security > Developer Tools. The exemption covers only that app's child processes, so a multiplexer (herdr, tmux) must be added as well. | nexte.st/docs/installation/macos | Not done. Untested. |
| `doctest = false` for internal library crates | matklad, "Fast Rust Builds" | Not set. |
| nextest, with test groups for heavy tests and `cargo test --doc` run separately | nexte.st | Configured but unused. |
| Wait on an event, not a fixed sleep | general | About 13 s of fixed sleeps remain, in `e2e/add.rs:41` (7 s), `e2e/stall.rs:30,60` and `platform/tests/process.rs:138`. |
| Run only the tests of affected crates in the inner loop (nextest `rdeps()`) | nexte.st filtersets | Not done. |

## 5. The plan

Each phase ends with a measurement. Do not start the next phase until the
measurement is recorded. Run every measurement on a quiet machine: one test
run at a time, no other session building, and `uptime` printed before and
after.

### Phase 0: Save the work already done (about 15 minutes)

1. Run `cargo xtask tidy --base main` and `scripts/gate.sh` on the current
   tree.
2. Commit the flake fixes as two commits, one for the tests and fakes, and
   one for the gate and docs. Leave out `HANDOFF-lead-tools-smoke-test.md`.
3. In the same docs commit, change `docs/conventions.md` section 2 so that
   it calls the `syspolicyd` explanation a hypothesis, not a fact.

Decision for the user: whether to commit. The tree passed a full gate once
(683 s).

### Phase 1: Find what makes each process start slow (about 30 minutes)

Every test start, git call and fake-host call pays this cost, so it is worth
more than any code change. `cargo clean` was done on 2026-10-10 and did not
help.

1. Time `/bin/echo x` ten times with `/usr/bin/time -p`. A healthy start
   takes under 0.01 s.
2. Quit Zoom, which stops its `log stream` process. Time `/bin/echo` again.
3. If starts are still slow, add Ghostty, herdr and tmux to System Settings,
   Privacy & Security, Developer Tools. Open a new terminal, and time again.
4. If starts are still slow, ask whoever manages SentinelOne whether it
   checks every program start, and whether a developer folder can be
   excluded.
5. Once a start costs milliseconds, run step 4 of the gate twice and record
   both times.

Steps 2 to 4 change the machine, so the user does them.

### Phase 2: Stop leaks and repeated builds (about 1 hour of work)

1. Make each fake agent exit when its test process is gone. Pass
   `FAKE_OWNER_PID` from `FakeHerdrEnv::env()` (`tests/fakes/src/fixture.rs:204`),
   and check it about once a second in the wait loop
   (`tests/fakes/src/claude_bin.rs:71`). Handle the `NeverStop` fault the
   same way. `tests/fakes/` is protected.
2. Make the end-to-end and crash tests honour `TEAMWIRE_FAKES_PREBUILT=1`
   for the CLI binary too (`tests/e2e/src/bin.rs`,
   `tests/crash/tests/support/rig.rs:72`).
3. Measure: run step 4 twice. Then kill one crash test by hand with SIGTERM
   and check that no `fake-claude` is left behind.

### Phase 3: Make the expensive test setups cheaper (about half a day)

Each item keeps every test and only changes how its setup is built.

| Change | Where | Expected effect |
| --- | --- | --- |
| Pass `-c user.name=… -c user.email=… -c commit.gpgsign=false` on the commit instead of three `git config` calls | `crates/teamwire-verify/tests/support/fixture.rs:146-149` | 3 fewer git processes in each of about 45 tests |
| Skip the git repository in tests that never use it | `teamwire-verify/tests/files.rs` (7 tests), 5 tests in `verify.rs`, and about 18 pane-only tests in `close/tests/support/rig.rs:118` | about 30 fewer repositories per run |
| Build one template repository per test process, and copy it per test | the verify, close and launch fixtures | the largest saving in this phase, but untried |
| Fold the six `*_is_a_valid_spec` tests into one loop over the presets, with one repository | `crates/teamwire-cli/tests/preset.rs:170-195` | 5 fewer repositories |
| Replace fixed sleeps with waits on an event | `e2e/add.rs:41`, `e2e/stall.rs:30,60`, `tmux_wiring.rs:234`, `platform/tests/process.rs:138` | about 13 s off the one-at-a-time end-to-end group |
| Set `doctest = false` in each library crate | each `crates/*/Cargo.toml` | about 12 s, and zero tests lost |

Measure step 4 twice after the phase.

### Phase 4: One integration test binary per crate (about half a day)

1. Merge the integration tests of each crate into `tests/it/main.rs`, with
   one module per old file. Do one crate per commit, and start with
   `teamwire-verify` and `teamwire-cli`.
2. Handle `platform/tests/process.rs:39`, which re-runs its own binary with
   `--exact sleeper_entry`. Under a merged binary that name becomes
   `process::sleeper_entry`.
3. Update the `cargo test -p <crate> --test <file>` commands in
   `docs/build-plan.md` and in briefs.
4. Measure: the build time after `touch crates/teamwire-core/src/lib.rs`,
   then step 4 twice. The binary count falls from 108 to about 19.

### Phase 5: Revisit the test runner (about 1 hour)

Do this phase only after phases 1 to 4. Each test under nextest is one start
of a binary, so nextest gains the most once each start is cheap.

1. Run nextest with `.config/nextest.toml` three times. Count failures, and
   time each run.
2. Switch the gate to nextest only if all three runs pass and the time beats
   `cargo test`. Keep `cargo test --doc` only if doctests are still on.
3. As a separate experiment, add Ghostty, herdr and tmux to Developer Tools,
   and repeat step 1. This step is the user's to do, in System Settings.

### Phase 6: Prune and move tests (about 2 hours, needs the user's approval)

1. Show the user the drop and merge list from section 3, with the covering
   test for each entry. Remove only the entries the user approves.
2. Move the four `teamwire-verify` tests that check a pure patch-text rule
   (`tests/verify.rs:260,298,345,377`) to unit tests in
   `src/inspect/tests.rs`. Keep one end-to-end weakening test
   (`verify.rs:242`).
3. Ask the user about the crash tests' kill-point counts (200 in
   `kill_send`, 60 in `kill_done`, 40 in `kill_hook`). Lowering them saves
   time but weakens a documented guarantee. `docs/test-strategy.md` section 3
   names 200.

### Phase 7: A faster inner loop (about 1 hour)

Add a script that runs only the changed crate and the crates that depend on
it, for example `cargo nextest run -E 'rdeps(=teamwire-store)'`. Keep the full
gate for the end of each wave. `scripts/` is protected.

## 6. Open questions for the user

1. Should phase 0 commit the uncommitted flake fixes?
2. May phase 1 run `cargo clean` on the shared checkout?
3. May `tests/e2e` depend on `teamwire-cli`, so that it can use
   `CARGO_BIN_EXE_teamwire` instead of building the binary itself?
   `tests/e2e/src/bin.rs:1-4` says it chose not to.
4. Will you approve test removals one by one in phase 6?
5. Are fewer crash-test kill points acceptable?
