# closer report, team fixes3

Names used here: `report`, `msg` and `ui` are the three implementers. Fix 1, fix 2 and fix 3 are the numbered sections of task.md. The joined head is branch `afk/fixes3` at 32dc818. Bench is the base, e78c32be.

## Summary

1. All three branches are merged on the joined head, and the merges are clean. The joined diff equals the three branch diffs added together: 11 files, +400, -15, and no file differs from its member branch.
2. Every check a member claimed passes when I run it myself, on the member's branch and again on the joined head. Every new test that should fail without its fix does fail when I revert that fix.
3. The full gate on the joined head fails at step 4, on `herdr_env_wins_over_tmux` only. That test fails the same way on bench (decision D-L1). Steps 5 and 6 pass when I run them on their own.
4. I found one real problem where two fixes meet. A cross-team message that was handed off stays `handed_off` in the sender's store forever. The status view lists it as unread forever. The new dashboard reason turns that into a permanent "1 unread" flag on the sender's team, even after the recipient read it and even after the team closed. Owner `ui`, graded wrong conclusion. Probes below.
5. The other findings are small: a duplicated "lead dead" on a card, a card test that covers 3 of 5 labels, an outdated reason in decision D5, and a process note about a reviewer's copied `node_modules`.

## Findings, most important first

### F1. The dashboard flags a team forever for a cross-team message (owner: ui; grade: wrong conclusion)

The parts involved:
- The sender's copy of a cross-team message keeps the last outcome the sender saw, and never becomes `acked` (`crates/teamwire-msg/src/remote.rs:10-11`). ADR 0016 says the same: "Neither log shows whether the other side read it" (`docs/adr/0016-messages-between-teams.md:86-87`).
- The status view, which feeds the dashboard, lists every `handed_off` row older than 600 s as unread, for any recipient kind (`crates/teamwire-watch/src/status.rs:164-176`). The only exits are a body short enough to fit on the typed line, or a later completion by a local member (`crates/teamwire-deliver/src/unread.rs:19-31`).
- The watch's own UNREAD alert skips every recipient that is not a local member (`crates/teamwire-watch/src/snapshot.rs:144-145`). So the watch never alerts on these rows, but the status view lists them.
- Fix 3 turns any non-empty `unread_deliveries` into the reason `<n> unread`, at every team state (`ui/src/web/model/teams.ts:80`), and `hasProblem` then puts the team under "Needs a look" (`teams.ts:63`).
- Fix 2 adds a new way into this state. A resend to a closing team whose copy was handed off or read now writes `handed_off` on the sender's row (`remote.rs:197`).

What I ran, in the temporary clone on the joined head. The probe files were removed afterwards.

| Probe | Result |
| --- | --- |
| teamwire-msg: a real cross-team send of a 400-character body, then `ack_delivery` for `db` in beta | beta's row `acked`. alpha's row for `db@beta` still `handed_off`. `unread_since` on alpha returns `Some(..)`. Exit 0. |
| teamwire-watch: a `handed_off` row to `db@beta` in alpha, aged 601 s, then one more day | `status_with` lists it in `unread_deliveries` at age 601 s, and again at age 87001 s. Exit 0. |
| ui: a `closed` team whose status lists one unread delivery to `db@beta`, aged one day | `problemReasons` returns `["1 unread"]`, and `hasProblem` returns `true`. Exit 0. |

The result is that a team that sent one long message to another team sorts under "Needs a look" with "1 unread" from 10 minutes later until the end of time. This happens even when the recipient read the message, and even after the sender's team closed.

The literal task sentence is met. The table says "Unread deliveries exist → `<n> unread`", and the status JSON does call these unread. I grade it wrong conclusion because it changes what the dashboard ships: every reviewer concluded fix 3 has no defect, and a permanent false flag is one.

Before fix 2 the resend-to-a-closing-team case was flagged forever too, as "1 stuck deliveries". After fix 2 the label changes to "1 unread". The plain cross-team send is new: on bench the dashboard did not flag it at all.

Possible fixes, for the parent to choose (see Questions):
- (a) In `problemReasons`, count only unread rows whose `to` has no `@`. This is a one-line filter in a file `ui` owns. It drops cross-team rows from a literal reading of "unread deliveries exist".
- (b) In the status view, skip rows whose recipient is in another team, as the watch snapshot already does. This is outside every member's files and changes the CLI's JSON content (not its schema).
- (c) Accept the flag and document it.

### F2. A dead lead shows "lead dead" twice on its card (owner: ui; grade: wording)

The card's meta row already prints the lead's liveness label (`ui/src/web/Overview.tsx:63-66`). The new reasons row prints the same label again when the category is failed or attention (`Overview.tsx:88-96`). A probe render of an open team with a dead lead found `>lead dead<` twice on the one card (exit 0, count 2). The task asks only that the card show the reasons, so nothing is wrong, but a reader sees the same words twice.

### F3. The card test covers 3 of the 5 labels, and cannot cover the lead label (owner: ui; grade: wrong detail)

`ui/src/web/Overview.test.ts:17-31` renders the fixtures, which give `1 unread`, `1 question waiting` and `watch stale`. No card test renders `AFK spent` or a lead label. A card test for the lead label would also pass without the reasons row, because the meta row already holds `>lead dead<` (see F2). The conclusion "the card shows the reasons" still holds, because one `map` renders every reason (`Overview.tsx:90-94`).

### F4. Decision D5 gives a wrong reason (owner: planner; grade: wrong detail)

`afk/decisions.md:26` says a `send` retried with the same client key "now gets the same honest outcome as `resend`". The plan reviewer showed this is false: `send` refuses a closing or closed recipient team before it stores anything (`crates/teamwire-msg/src/rules/foreign.rs:41-42`), so a retried send reaches the changed branch only in a short race (reports/planrev.md, "Note with no failure"). The decision itself still holds, and no doc or code copied the claim. The sentence is still in decisions.md.

### F5. The ui report leaves out the 3 skipped tests (owner: ui; grade: wording)

`reports/ui.md:113` says `bun test` gave "170 pass, 0 fail". The run gives 170 pass, 3 skip, 0 fail. The 3 skips are on bench too: they need `TW_UI_LIVE=1` (`ui/src/shared/teamwire.test.ts:61`).

### F6. A reviewer copied `node_modules`, and two reviewer clones are still on disk (owner: revui and revmsg; grade: wording, no conclusion changes)

revui copied `ui/node_modules` from the ui worktree into its clone (reports/revui.md:317, 374). The team rules forbid copying `node_modules` or `target/`, because each copied file costs a scan by the security agent. Its test results are still valid, since the lockfile is the same. Two clones are left under `afk/tmp/`: `revmsg-clone` (2.2 GB, with `target/`) and `revui-clone` (131 MB, with `target/` and the copied `node_modules`). I did not delete them.

## What was merged

| Branch | Commits | Files | Merge on the joined head |
| --- | --- | --- | --- |
| fixes3/report (078aa59) | dcfb436, 078aa59 | `crates/teamwire-report/src/usage.rs`, `src/final_message.rs`, `tests/result_file.rs` (+93 -7) | d49e10f |
| fixes3/msg (38a272a) | b4e7570, 77c9b40, 38a272a | `crates/teamwire-msg/src/remote.rs`, `tests/cross_team.rs`, `docs/api/cli.md`, `docs/api/mcp.md` (+177 -3) | 362b6f1 |
| fixes3/ui (5a002b0) | 17fceae, df8fd9b, 5a002b0 | `ui/src/web/model/teams.ts`, `teams.test.ts`, `ui/src/web/Overview.tsx`, `Overview.test.ts` (+130 -5) | 32dc818 |

All three branches start from e78c32be. For every file a member changed, `git diff <branch> afk/fixes3 -- <file>` is empty, so the merges changed nothing.

## What I verified, with real exit codes

`teamwire verify fixes3`: exit 0. report, msg and ui each PASS with 0 flags, and the verdict is "clean". No edit outside an owned path and no weakened test.

All runs below are mine, in the temporary clone `$TEAMWIRE_HOME/cache/closer/fixes3/clone`. The ui setup (`bun install --frozen-lockfile` in `ui/`) ran once, exit 0.

| Command | fixes3/report | fixes3/msg | fixes3/ui | joined head |
| --- | --- | --- | --- | --- |
| report's check: `cargo test -p teamwire-report --locked && TEAMWIRE_BASE=e78c32be scripts/gate.sh fast` | 0 (95 tests) | | | 0 |
| msg's check: `cargo test -p teamwire-msg --locked && TEAMWIRE_BASE=e78c32be scripts/gate.sh fast` | | 0 (104 tests) | | 0 |
| `cargo test -p teamwire-cli --locked --test preset` (msg claimed it) | | 0 (10 tests) | | 0 |
| ui's check: `cd ui && bun test && bun run typecheck` | | | 0 (170 pass, 3 skip, 0 fail) | 0 |
| `TEAMWIRE_BASE=e78c32be scripts/gate.sh fast` (ui claimed it) | | | 0 | 0 |

The four checks the task names, on the joined head:

| Command | Exit |
| --- | --- |
| `cargo test -p teamwire-report --locked` | 0 |
| `cargo test -p teamwire-msg --locked` | 0 |
| `(cd ui && bun test)` | 0 |
| `TEAMWIRE_BASE=e78c32be scripts/gate.sh fast` | 0 (fmt ok, tidy ok, clippy ok, no WEAKENED or PROTECTED-CHANGED line) |

### Each new test fails without its fix

| Change in the clone | Result |
| --- | --- |
| report: `crates/teamwire-report/src/` set back to e78c32be, tests kept | exit 101. The pipe test fails at `result_file.rs:222` ("the hook blocked on the pipe: Timeout"). The 64 MiB + 1 test fails at `result_file.rs:239` with `Final message from: transcript`. The exactly-64 MiB test passes, as expected, because bench read every file. |
| report: `<= MAX_TRANSCRIPT_BYTES` changed to `<` in `usage.rs:106` | exit 101. The exactly-64 MiB test fails. So that test pins "at most". |
| msg: `crates/teamwire-msg/src/` set back to e78c32be | exit 101. The handed-off, read, pending and unconfirmed tests fail with `(Undelivered, Some(TeamClosed), Undelivered, 0)`. The `undelivered` test passes, as expected under D-L2. |
| msg: the `undelivered` arm mapped like `pending` | exit 101. `a_copy_that_surely_never_arrived_stays_undelivered_when_its_team_closes` fails. So that test pins D-L2. |
| ui: `teams.ts` and `Overview.tsx` set back to e78c32be | exit 1. `teams.test.ts` cannot load (`Export named 'problemReasons' not found`), and the card test fails. |
| ui: only `Overview.tsx` set back | exit 1. Only the card test fails. |
| ui: only `|| problemReasons(t).length > 0` removed from `hasProblem` | exit 1. Only "a team whose only signal is a reason is a problem and sorts first" fails. |
| ui: the D9 edit to the old sort test undone | exit 1. "sorts problem teams first, then by title" fails. So the D9 edit was needed and did not weaken the test. |

### The full gate on the joined head, step by step, against bench

| Step | Joined head | Bench e78c32be |
| --- | --- | --- |
| Full gate `TEAMWIRE_BASE=e78c32be scripts/gate.sh` | exit 1 at step 4 | exit 1 at step 4 |
| 1 fmt | ok | ok |
| 2 tidy | ok | ok |
| 3 clippy | ok | ok |
| 4 test, as the gate runs it | FAILED: `herdr_env_wins_over_tmux` (`crates/teamwire-cli/tests/tmux_wiring.rs:377`, "could not run tmux: not_found", left `Some(1)`, right `Some(0)`) | FAILED: the same test, same line, same values |
| 4 test with `--no-fail-fast` added | exit 101. 168 test binaries, 1863 passed, 1 failed: `herdr_env_wins_over_tmux` | exit 101. 1854 passed, 2 failed: `herdr_env_wins_over_tmux` and `kill9_after_the_agent_started_and_was_recorded_is_closed_by_close` |
| `herdr_env_wins_over_tmux` alone | exit 101 | exit 101 |
| `kill_launch` alone, 6 runs each | 6 of 6 exit 0 | 6 of 6 exit 0 |
| 5 doc, run on its own | exit 0 | not run |
| 6 docs, run on its own | exit 0 ("docs check: ok") | not run |

The herdr test fails on both commits. Its own comment says it holds only with no `tmux` or `herdr` process among the test's parents, and this session runs under both (`TMUX` set, `HERDR_ENV=1`). So the failure comes from the environment, as D-L1 says.

The bench-only crash-test failure (`tests/crash/tests/kill_launch.rs:78`, "REFUSED: scout is busy") happened once under the load of the full run. The same test passed 12 times out of 12 alone, on both commits. The team did not cause it, and it is not a regression.

## Each end-state sentence of task.md, against the joined head

| Sentence | True? | Test that proves it |
| --- | --- | --- |
| Fix 1: "the final-message fallback reads the transcript only when it is a regular file of at most 64 MiB" | yes. `final_message.rs:25` reads through `read_transcript(path, 0)` (`usage.rs:102-108`). | `a_transcript_that_is_a_named_pipe_gives_no_final_message` (`result_file.rs:205`), `a_transcript_past_64_mib_gives_no_final_message` (`:228`), `a_transcript_of_exactly_64_mib_is_read` (`:244`) |
| Fix 1: "with the same rule as the usage reader" | yes. Both readers call the one function (`usage.rs:45`, `final_message.rs:25`). | No test compares the two readers. The structure proves it. The existing usage tests (`usage.rs` unit tests, `a_transcript_that_is_a_named_pipe_is_never_opened` in `tests/turn_end.rs`) still pass. |
| Fix 1: "For a named pipe or a file past the limit, the hook still completes through `on_turn_end`" | yes | The two tests above. The helper `finish` asserts `TurnEndReply::Continue` (`result_file.rs:28-32`). The pipe test runs in a thread with a 10 s limit. |
| Fix 1: "the result file says `Final message from: none`" | yes | the same two tests (`result_file.rs:223`, `:239`) |
| Fix 1: "the recorded completion's `final_source` is `FinalSource::None`" | yes | the same two tests (`result_file.rs:224`, `:240`) |
| Fix 2: "when the receiving team is closing or closed, `resend` reads that team's stored delivery row for the recipient" | yes. `held_there` reads it (`remote.rs:183-211`, read at `:192`), called from the not-open branch (`remote.rs:68-75`). | all five new tests in `cross_team.rs:602-656` |
| Fix 2: "A row there that is handed off or read keeps that outcome: the resend reports `HandedOff`, the sender's own row for `db@beta` becomes handed off, and nothing is typed" | yes (`remote.rs:197`) | `a_copy_handed_off_in_a_closing_team_is_recorded_handed_off` (`:602`, closing) and `a_copy_read_in_a_closed_team_is_recorded_handed_off` (`:610`, closed). Each asserts `HandedOff`, no reason, alpha's row `HandedOff`, and 0 lines typed. |
| Fix 2: "A row still pending reports `Unconfirmed` with reason `DeliveryErrorCode::TeamClosed`, and the sender's row becomes unconfirmed" | yes (`remote.rs:198-205`) | `a_pending_copy_in_a_closing_team_is_recorded_unconfirmed` (`:618`) |
| Fix 2: "Only a missing copy is recorded undelivered, as today" | true for a missing copy. A copy whose row there is `undelivered` is also recorded undelivered (`remote.rs:209`). The lead decided that in D-L2, so it is a recorded choice, not a defect. | missing copy: `a_team_that_closed_after_the_send_was_stored_leaves_it_undelivered` (`:432`, unchanged). D-L2 case: `a_copy_that_surely_never_arrived_stays_undelivered_when_its_team_closes` (`:646`) |
| Fix 3: "a new exported function `problemReasons(team: TeamModel): string[]` returns one label per signal, in this order" | yes (`teams.ts:71-84`) | "lists all problem reasons in a fixed order" (`teams.test.ts:100`) |
| Fix 3: lead liveness failed or attention gives the `livenessInfo` label, only while open | yes (`teams.ts:74-76`) | "gives each problem reason its own label" (`:88`: dead, gone, blocked), "a lead that is not failing gives no liveness reason" (`:121`), the closing, closed and null loop in `:100` |
| Fix 3: watch `stale` gives `watch stale`, only while open | yes (`teams.ts:77`) | `:88`, `:100` |
| Fix 3: `afk.spent` gives `AFK spent`, only while open | yes (`teams.ts:78`) | `:88` (spent and not spent), `:100` |
| Fix 3: unread deliveries give `<n> unread`, at any state | yes (`teams.ts:80`). See F1 for which rows count. | `:88`, `:100`, "problem reasons on the fixtures" (`:127`) |
| Fix 3: questions to the lead waiting 600 s or more give `<n> question waiting`, at any state | yes (`teams.ts:81-82`, `QUESTION_WAIT_S = 600` at `:68`) | `:88` (n = 2), `:100`, `:127` |
| Fix 3: "A question to another member, or one under 600 s old, does not count" | yes | "counts a question only to the lead and only after 600 s" (`:114`: 599 and 600, `api`, `lead@beta`, `[lead, api]`) |
| Fix 3: "`hasProblem` is also true whenever `problemReasons` returns anything" | yes (`teams.ts:63`) | "a team whose only signal is a reason is a problem and sorts first" (`:135`), for `watch stale` |
| Fix 3: "The overview card shows the reasons" | yes (`Overview.tsx:55, 88-96`) | "each card shows its own team's problem reasons and no other team's" (`Overview.test.ts:17`), for 3 of the 5 labels (F3) |

## Where the members' work meets

| Pair | What I checked | Result |
| --- | --- | --- |
| msg and ui | Fix 2 writes `handed_off` on more sender rows. Fix 3 flags unread rows. | F1. |
| report and ui | Does the dashboard read anything fix 1 changes? `git grep` for `final_source`, `final message` and `transcript` under `ui/src` finds nothing. | No contact. |
| report and msg | The two crates share no changed file and no changed function. | No contact. |
| msg's docs and ADR 0016 | `docs/adr/0016-messages-between-teams.md:89-90` still says a team that closes leaves the sender's row `undelivered`. After fix 2 that is true only for a missing or `undelivered` copy. | Known, decided by the lead as D-L3. `docs/adr/` is protected. |
| Member reports against reviewer reports | report and revreport agree (9 tests in result_file, gate ok). msg and revmsg agree (17 in cross_team, 104 in the crate, 10 in preset). ui and revui agree, except F5. | No contradiction in facts. |

## Did each member do its task?

- **report: did all of it.** It added the one shared function (`usage.rs:99-108`), and the fallback calls it (`final_message.rs:25`). The usage loop calls it too (`usage.rs:45`). It added the three tests the brief asked for (`result_file.rs:205`, `:228`, `:244`) with no new `#[expect(`. It changed only owned files. It has 2 commits and 2 progress lines, and it ends with "Decisions I made".
- **msg: did all of it.** It added `held_there` (`remote.rs:183-211`) with every `DeliveryState` named and the D-L2 comment (`remote.rs:206-209`). It added the five tests, with the name the brief required (`cross_team.rs:646`). The diff of `cross_team.rs` is +127 -0, so the existing missing-copy test is unchanged. It updated `docs/api/cli.md:350-355`, `docs/api/mcp.md:115-118` and the module comment (`remote.rs:14-15`). It has 3 commits and 3 progress lines, and it ends with "Decisions I made".
- **ui: did all of it.** It added `problemReasons` (`teams.ts:71-84`), extended `hasProblem` (`teams.ts:63`), and rendered the reasons on the card (`Overview.tsx:88-96`). It wrote the per-card test the brief required (`Overview.test.ts:11-15`) and applied D9 (`teams.test.ts:42`). It has 3 commits and 3 progress lines, and it ends with "Decisions I made". F1 is about the task's own rule meeting existing status behaviour, not a step ui skipped.

## Known limits

- `shortcut:` comments: the team's branches add none. `grep -i shortcut` over the joined diff finds 0 matches. The 6 such comments in the tree are all on bench already.
- D-L2: an `undelivered` row in the closing team is recorded undelivered, which differs from a literal reading of "Only a missing copy is recorded undelivered". The lead decided it.
- D-L3: ADR 0016:89-90 still states the old rule. No superseding ADR was written.
- The team page's "Needs a look" section lists only stuck deliveries and undelivered results (`ui/src/web/TeamDetail.tsx:81-126`). A team flagged on the overview for `watch stale` alone shows no such section on its own page. The ui brief put `TeamDetail.tsx` out of scope.
- Fix 2 does not assert the kept route or the error detail text on an `Unconfirmed` outcome (revmsg noted this too). The code does both (`remote.rs:200-204`).
- Fix 1 handles a file that becomes a pipe between the size check and the read the same way as the usage reader, which is out of scope by the brief.
- `herdr_env_wins_over_tmux` fails in any session under tmux or herdr, on bench too (D-L1). The full gate cannot pass from this machine's agent sessions.
- `kill9_after_the_agent_started_and_was_recorded_is_closed_by_close` failed once on bench under load and then passed 12 of 12 runs alone.

## Claimed but not verified by me

- The planner's dependents run on the prototypes (plan.md, "Dependents run"). I ran the whole workspace test on the joined head instead, which covers those crates.
- revreport's mutation that drops `is_file()`. I ran a different break of the same function (`<=` to `<`) and the full revert.
- Four of revui's seven mutations (the open check, the `> 600` threshold, `startsWith("lead")`, the dropped `attention` category). I ran the other three kinds and two full reverts.
- The planner's prototype results in plan.md. Those clones were deleted, and the real branches now stand in for them.

## Questions for the parent

1. F1: which way should the permanent "unread" flag for cross-team messages go? (a) `problemReasons` counts only unread rows whose `to` has no `@`, a one-line change in a file `ui` owns. (b) The status view skips cross-team recipients as the watch snapshot does, outside every member's files. (c) Accept it and document it. The task's table says "Unread deliveries exist", so (a) and (b) both narrow that sentence.
2. F6: should the two leftover reviewer clones under `afk/tmp/` (2.2 GB and 131 MB) be deleted? I did not touch them.

## Progress

- read the brief, task.md, plan.md, decisions.md, the three implementer briefs, all member and reviewer reports, comms m001 and m002, and `teamwire status fixes3`
- `teamwire verify fixes3`: exit 0, report, msg and ui PASS with 0 flags, verdict "clean"
- made the clone at `$TEAMWIRE_HOME/cache/closer/fixes3/clone`. report's check on fixes3/report (078aa59): exit 0
- report revert of `src/` to e78c32be with tests kept: result_file exit 101, the pipe test and the 64 MiB + 1 test fail. The `<=` to `<` mutation: the exactly-64 MiB test fails
- msg's check on fixes3/msg (38a272a): exit 0, 104 tests, fast gate ok. preset test exit 0. Revert of `src/` to e78c32be: cross_team exit 101, 4 new tests fail with `(Undelivered, Some(TeamClosed), Undelivered, 0)`. The undelivered-to-unconfirmed mutation: the D-L2 test fails
- ui setup in the clone exit 0. ui's check on fixes3/ui (5a002b0): exit 0 (170 pass, 3 skip, 0 fail, tsc ok). Fast gate exit 0. Reverts: both source files give 2 failures (load error and card test). `Overview.tsx` only: the card test fails. The `hasProblem` line only: the sort-first test fails. The D9 line undone: the old sort test fails
- joined head (32dc818): report tests 0, msg tests 0, `bun test` 0, typecheck 0, fast gate 0, preset 0
- full gate on the joined head: exit 1. Steps 1-3 ok. Step 4 FAILED on `herdr_env_wins_over_tmux`. Steps 5-6 did not run
- joined head: step 4 with `--no-fail-fast` exit 101, 168 test binaries, 1863 passed, 1 failed (the herdr test only). Step 5 doc exit 0. Step 6 docs exit 0
- no `shortcut:` comment in the team's diff (0 matches). The 6 in the tree are all on bench already
- bench e78c32be: full gate exit 1 at the same step 4 failure. Step 4 with `--no-fail-fast` exit 101, 1854 passed, 2 failed (the herdr test and `kill9_after_the_agent_started_and_was_recorded_is_closed_by_close`)
- `kill_launch` alone: bench 6 of 6 exit 0, joined head 6 of 6 exit 0. Herdr test alone: bench exit 101, joined head exit 101
- probes on the joined head for F1 (three files) and F2 (one file), all exit 0 with the results above. The clone was reset and cleaned afterwards (`git status` empty)

## Decisions I made

- I ran the ui setup as `cd "$C/ui" && bun install --frozen-lockfile`, with the clone's path put in for `$1`, instead of through `sh -c`. My rules forbid wrapping a command in `sh -c`, and the command is otherwise the same.
- I ran gate step 4 a second time with `--no-fail-fast`, and steps 5 and 6 on their own, on the joined head. The gate stops at its first failing step, and `cargo test` stops at its first failing test binary, so the plain run hides every result after the herdr test. I did the same `--no-fail-fast` run on bench for a fair comparison.
- I ran `kill_launch` six times alone on each commit, to tell a one-off failure from a real difference between the commits.
- I wrote four probe tests in the clone only, to test the F1 interaction and the F2 render, and removed them with `git reset --hard` and `git clean -fd`.
- I graded F1 as wrong conclusion even though the literal task sentence is met, because it changes what the dashboard ships, and every reviewer concluded fix 3 had no defect. I named `ui` as owner because `problemReasons` is where the status signal becomes a flag. The status view itself belongs to no member of this team.
- I did not delete the leftover reviewer clones under `afk/tmp/`. They are not mine, and deleting them is the parent's call.
