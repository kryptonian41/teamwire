# Report: three fixes in teamwire (team fixes3)

All three fixes are done. Each one is tested, and the finished work is on branch `afk/fixes3`. Every check the task names passes on the final commit. The full gate fails only on one test. That test fails the same way on the unchanged base.

## What you asked

You asked for three independent changes in teamwire, each finished when its behaviour is true and tested (afk/task.md):

> 1. The Stop hook never opens a named pipe or a huge file for the final message (crate `teamwire-report`).
> "the final-message fallback reads the transcript only when it is a regular file of at most 64 MiB, with the same rule as the usage reader. For a named pipe or a file past the limit, the hook still completes through `on_turn_end`, the result file says `Final message from: none`, and the recorded completion's `final_source` is `FinalSource::None`."
>
> 2. A resend to a team that is closing keeps what that team already holds (crate `teamwire-msg`).
> "when the receiving team is closing or closed, `resend` reads that team's stored delivery row for the recipient. A row there that is handed off or read keeps that outcome [...] A row still pending reports `Unconfirmed` with reason `DeliveryErrorCode::TeamClosed` [...] Only a missing copy is recorded undelivered, as today."
>
> 3. The dashboard says why a team needs a look (`ui/`).
> "a new exported function `problemReasons(team: TeamModel): string[]` returns one label per signal, in this order" (lead liveness, `watch stale`, `AFK spent`, `<n> unread`, `<n> question waiting`). "`hasProblem` is also true whenever `problemReasons` returns anything. The overview card shows the reasons."

You also asked for a test per change, these checks, a commit, no push, and the branch name at the end:

```bash
cargo test -p <crate> --locked
cd ui && bun test
TEAMWIRE_BASE=e78c32be scripts/gate.sh fast
```

## What I understood

The team read the task as three end states on base `bench` at e78c32be, with no shared file (afk/understanding.md).

| Fix | End state the team built toward |
| --- | --- |
| 1, final message | The usage reader and the final-message fallback use one shared check. A named pipe and a 64 MiB + 1 byte file both give `Final message from: none` and `FinalSource::None`. A file of exactly 64 MiB is still read. |
| 2, resend | The code that delivers to another team, `deliver_to_team`, reads the receiving team's row when that team is not open. `handed_off` or `acked` there gives `HandedOff`. `pending` gives `Unconfirmed` with `TeamClosed`. No copy, or no database, gives `Undelivered` as before. Nothing is typed in any case. |
| 3, dashboard | `problemReasons` returns the five labels in order. The first three appear only while the team state is `open`. `hasProblem` is true when the list is not empty. Each overview card shows its own team's reasons. |

The understanding put out of scope the team detail page, the dashboard schemas, the watch's delivery path, and the race where a file becomes a pipe between the check and the read.

## The result

The finished work is on branch **`afk/fixes3`**, at commit `1dd9b22`. The branch starts from `bench` at e78c32be. It changes 11 files, with 423 lines added and 15 removed.

```bash
git -C /private/tmp/claude-501/bnruns/bne68n09a4/clone log --oneline e78c32be..afk/fixes3
```

```text
1dd9b22 Merge fixes3/ui closer fixes
b04844d ui: show the lead's liveness once per card; test the AFK reason on a card
c66ed52 ui: count an unread delivery only to a member of this team
32dc818 Merge fixes3/ui
5a002b0 ui: each overview card shows its team's problem reasons
df8fd9b ui: a team with a problem reason counts as a problem
17fceae ui: add problemReasons, the labels for what needs the lead
362b6f1 Merge fixes3/msg
38a272a Document resend to a team that is closing or closed
77c9b40 Resend records a copy that may have arrived in a closing team as unconfirmed
b4e7570 Resend records a copy that arrived in a closing team as handed off
d49e10f Merge fixes3/report
078aa59 teamwire-report: the final-message fallback reads only a regular transcript within 64 MiB
dcfb436 teamwire-report: move the transcript file check into read_transcript
```

Each fix also sits alone on its own branch, cut from e78c32be:

| Branch | Head | Files it changes |
| --- | --- | --- |
| `fixes3/report` | 078aa59 | `crates/teamwire-report/src/usage.rs`, `src/final_message.rs`, `tests/result_file.rs` |
| `fixes3/msg` | 38a272a | `crates/teamwire-msg/src/remote.rs`, `tests/cross_team.rs`, `docs/api/cli.md`, `docs/api/mcp.md` |
| `fixes3/ui` | b04844d | `ui/src/web/model/teams.ts`, `teams.test.ts`, `ui/src/web/Overview.tsx`, `ui/src/web/Overview.test.ts` |

What each fix does, in one line:
- **Fix 1.** A new function `read_transcript` in `usage.rs` holds the regular-file and 64 MiB check. The usage reader and the final-message fallback both call it.
- **Fix 2.** A new function `held_there` in `remote.rs` reads the closing team's row and decides the sender's row from it. The function writes nothing in the closing team.
- **Fix 3.** `problemReasons` returns the labels, `hasProblem` counts them, and each overview card shows them as flags.

To try it, check out the branch and run the task's checks from the repository root:

```bash
git checkout afk/fixes3
cargo test -p teamwire-report --locked
cargo test -p teamwire-msg --locked
(cd ui && bun install --frozen-lockfile && bun test && bun run typecheck)
TEAMWIRE_BASE=e78c32be scripts/gate.sh fast
```

The new tests by name:

| Fix | Test file | New tests |
| --- | --- | --- |
| 1 | `crates/teamwire-report/tests/result_file.rs` | `a_transcript_that_is_a_named_pipe_gives_no_final_message`, `a_transcript_past_64_mib_gives_no_final_message`, `a_transcript_of_exactly_64_mib_is_read` |
| 2 | `crates/teamwire-msg/tests/cross_team.rs` | `a_copy_handed_off_in_a_closing_team_is_recorded_handed_off`, `a_copy_read_in_a_closed_team_is_recorded_handed_off`, `a_pending_copy_in_a_closing_team_is_recorded_unconfirmed`, `an_unconfirmed_copy_in_a_closing_team_stays_unconfirmed`, `a_copy_that_surely_never_arrived_stays_undelivered_when_its_team_closes` |
| 3 | `ui/src/web/model/teams.test.ts`, `ui/src/web/Overview.test.ts` | 7 tests in teams.test.ts, including one for a cross-team unread row. 2 card tests in Overview.test.ts. |

## Verified

Every check the task names passed on the final commit, 1dd9b22. The lead ran them there after the ui fix round (afk/final-checks/exit-codes.txt).

| Command | Where it ran | Exit |
| --- | --- | --- |
| `cd ui && bun test` | lead, final commit 1dd9b22 | 0 (172 pass, 3 skip, 0 fail) |
| `cd ui && bun run typecheck` | lead, 1dd9b22 | 0 |
| `cargo test -p teamwire-report --locked` | lead, 1dd9b22 | 0 |
| `cargo test -p teamwire-msg --locked` | lead, 1dd9b22 | 0 |
| `TEAMWIRE_BASE=e78c32be scripts/gate.sh fast` | lead, 1dd9b22 | 0 (fmt ok, tidy ok, clippy ok) |
| `cd ui && bun test && bun run typecheck` | `teamwire verify`, ui worktree at b04844d (verify/ui.log) | pass (172 pass, 3 skip, 0 fail) |

The 3 skipped dashboard tests need `TW_UI_LIVE=1`, and they skip on the base too (`ui/src/shared/teamwire.test.ts:61`).

The closer checked an earlier joined commit, **32dc818**. That commit came before the ui fix round. The closer did not check the two ui fix commits, c66ed52 and b04844d. Only `teamwire verify` and the lead's final run above checked them. Here are the closer's runs (reports/closer.md):

| Command | Where it ran | Exit |
| --- | --- | --- |
| `teamwire verify fixes3` | team state | 0, all three members PASS, 0 flags |
| report's check: crate tests + fast gate | `fixes3/report` and 32dc818 | 0 and 0 |
| msg's check: crate tests + fast gate | `fixes3/msg` and 32dc818 | 0 and 0 |
| `cargo test -p teamwire-cli --locked --test preset` | `fixes3/msg` and 32dc818 | 0 and 0 |
| ui's check: `bun test && bun run typecheck` | `fixes3/ui` at 5a002b0, and 32dc818 | 0 and 0 |
| Full gate `TEAMWIRE_BASE=e78c32be scripts/gate.sh` | 32dc818 | **1**, at step 4 |
| Full gate | bench e78c32be | **1**, at the same step 4 |
| Gate step 4 with `--no-fail-fast` | 32dc818 | 101: 1863 passed, 1 failed |
| Gate step 4 with `--no-fail-fast` | bench e78c32be | 101: 1854 passed, 2 failed |
| Gate steps 5 (doc) and 6 (docs), run alone | 32dc818 | 0 and 0 |

**The full gate failed only on `herdr_env_wins_over_tmux`.** That test fails the same way on unchanged bench, at `crates/teamwire-cli/tests/tmux_wiring.rs:377` with "could not run tmux: not_found". The test's own comment says it holds only when no `tmux` or `herdr` process is among its parents. Every agent session on this machine runs under both. The lead recorded it as a failure that exists before this run (decision D-L1). The second bench failure, `kill9_after_the_agent_started_and_was_recorded_is_closed_by_close`, happened once under load. That test then passed 12 runs out of 12 alone, on both commits.

Nobody ran the full gate on the final commit 1dd9b22. The ui fix round changed only files under `ui/`, which the gate's Rust steps do not read.

Each new test fails without its fix. The closer reverted each fix on its own and saw the new tests fail:

| Revert | Result |
| --- | --- |
| report: `src/` set back to e78c32be | the pipe test times out, and the 64 MiB + 1 test reads `transcript` |
| report: `<=` changed to `<` in the size check | the exactly-64 MiB test fails |
| msg: `src/` set back to e78c32be | 4 new tests fail with `(Undelivered, Some(TeamClosed), Undelivered, 0)` |
| msg: the `undelivered` arm treated like `pending` | the D-L2 test fails |
| ui: `teams.ts` and `Overview.tsx` set back | `teams.test.ts` cannot load, and the card test fails |

The ui member also showed failures for its fix-round tests before each fix (reports/ui.md). The cross-team unread test failed before c66ed52. The "lead dead shows once" card test failed before b04844d with `Expected: 1, Received: 2`.

## Decisions made for you

The four decisions below change the result most. The full list, with reasons and how to undo each, is in afk/decisions.md.

| Decision | What was chosen | Why it matters to you |
| --- | --- | --- |
| D4 and D-L2: a closing team's row that is `undelivered` | The sender's row stays `undelivered` with `TeamClosed`, the same as a missing copy. A row that is `unconfirmed` there is treated like `pending`. | The task says "Only a missing copy is recorded undelivered". Read word for word, an `undelivered` copy should not stay undelivered. The lead kept D4, because that row means the line surely did not arrive. Calling it unconfirmed would tell the sender it may have arrived. A test pins this choice. To undo it, change one match arm in `held_there` in `remote.rs` and its test. |
| D-L3: ADR 0016 | No new ADR was written. | One sentence in ADR 0016 is now out of date. See "Not done, and why". |
| D-L4: which unread rows count for `<n> unread` | Only rows whose recipient is in this team, meaning no `@` in `to`. | The task says "Unread deliveries exist". The sender's copy of a message to another team stays `handed_off` forever by design, so the status view lists it as unread forever. Without the filter, one long message to another team would flag the sender's team forever, even after it closes. To undo it, drop the filter in `problemReasons`. |
| D1: the team roster | planner, plan reviewer, three implementers in parallel (report, msg, ui), one reviewer per branch, a closer and a reporter. | No analyst and no scouts. Each fix touched a separate area, so the three implementers shared no file. |

Other decisions that shape the result:
- **D3.** The shared check lives in `usage.rs` as `read_transcript(path, used)`, next to the 64 MiB constant.
- **D5.** The fix 2 change sits in `deliver_to_team`, which `send` and `resend` share. `send` refuses a closed team earlier, so `send` reaches the new code only in a short race.
- **D6.** The label is `<n> question waiting` for every n, word for word from the task. So two questions show as `2 question waiting`.
- **D7.** A question counts only when its `to` list holds exactly `lead`. `lead@beta` does not count. A team whose state cannot be read counts as not open.
- **D8 and D15.** Each reason shows as a `.flag` span on the card. The card test checks each reason on its own card and on no other card.
- **D9.** An old sort test now clears two lists on its "quiet" team. The fixture it copies has an unread row and a waiting question, which now count as a problem. The expected order did not change.
- **D13.** `docs/api/cli.md` and `docs/api/mcp.md` each gained one sentence on a resend to a closing team.

What the members decided on their own (reports/*.md, "Decisions I made"):

| Member | Decision that matters |
| --- | --- |
| ui | The card leaves out the reason equal to the lead's liveness label. The card's top row already shows that label. `problemReasons` still returns it, so `hasProblem` is unchanged. |
| msg | `held_there` names every `DeliveryState` with no catch-all arm, so a new state forces a decision. The docs name the reason code `team_closed`. |
| report | The 64 MiB test also checks `Final message from: transcript` in the result file. |
| planner | msg owns `crates/teamwire-cli/tests/preset.rs` read-only, because that test reads `docs/api/mcp.md`. |
| closer | It ran gate step 4 again with `--no-fail-fast`, on both commits, so one failing test could not hide others. |
| revui | It copied `ui/node_modules` into its clone instead of installing. The team rules forbid that copy (finding F6). |

## Not done, and why

**ADR 0016 is out of date (D-L3).** `docs/adr/0016-messages-between-teams.md:89-90` still says:

> A team that closes after the sender's copy is stored leaves that row `undelivered` with `team_closed`.

After fix 2, that sentence holds only when the closing team holds no copy, or holds a copy marked `undelivered`. A copy handed off or read there now gives `handed_off`. A pending copy now gives `unconfirmed`. `docs/adr/` is protected. The project changes an ADR only by adding a new one that replaces it, and the task did not ask for one. To fix this, add a new ADR that replaces that sentence.

**Closer finding F4 is not fixed.** Decision D5 in afk/decisions.md:26 gives a wrong reason. The decision says a `send` retried with the same client key "now gets the same honest outcome as `resend`". The claim is false. `send` refuses a closing or closed team before it stores anything (`crates/teamwire-msg/src/rules/foreign.rs:41-42`). A retried `send` reaches the new code only in a short race. The decision itself still holds, and no code or doc copies the claim. The same wrong sentence is also in afk/understanding.md.

**Closer finding F6 is not fixed.** Two reviewer clones are still on disk under `afk/tmp/`:

| Clone | Size | Holds |
| --- | --- | --- |
| `afk/tmp/revmsg-clone` | 2.2 GB | a full `target/` build directory |
| `afk/tmp/revui-clone` | 127 MB | `target/` and a copied `ui/node_modules` |

The revui reviewer copied `node_modules`, which the team rules forbid. Its test results are still valid, because the lockfile is the same. Nobody deleted the clones. Deleting them is safe once you no longer need them.

The closer's other findings, F1, F2, F3 and F5, were all fixed in the ui fix round (reports/ui.md).

## Risks

**The `<n> unread` label is narrower than the task's words (D-L4).** The dashboard skips unread rows addressed to another team. A real unread message to another team therefore never flags the sender's team. The status view and `teamwire status` still list those rows, unchanged.

**Nobody ran the full gate on the final commit.** The closer ran the full gate on 32dc818. The ui fix round after that changed only `ui/` files. The lead's final run and `teamwire verify` covered those two commits.

**Nobody could make the full gate pass on this machine.** `herdr_env_wins_over_tmux` fails in any session under tmux or herdr. A run from a plain shell with no tmux or herdr parent would show whether the whole gate passes.

**The team detail page does not match the overview.** The team page's "Needs a look" section lists only stuck deliveries and undelivered results (`ui/src/web/TeamDetail.tsx:81-126`). A team flagged on the overview for `watch stale` alone shows nothing there. The task scoped the detail page out.

**Two details of fix 2 have no test.** The tests do not check the kept route or the error text on an `Unconfirmed` outcome. The code does both (`remote.rs:200-204`). The reviewer revmsg and the closer both noted this.

**A file can change between the check and the read.** A file that becomes a named pipe in that window would still block the hook. The usage reader has the same gap. The task asked for the same rule, not a stronger one.

**Some reviewer claims were not repeated by the closer.** The closer did not repeat revreport's break that drops `is_file()`, or four of revui's seven breaks. The closer ran other breaks of the same code instead.

## Cost

The run took about **1 hour 7 minutes** of wall time, from the team's start at about 15:05 UTC to 16:12 UTC when this report was written.

The team used about **61.2 million tokens**, nearly all of them cache reads. Output tokens totalled about 315 thousand. The numbers come from `teamwire metrics fixes3`.

| Member | Role | Effort | Input | Cache write | Cache read | Output | Total |
| --- | --- | --- | --- | --- | --- | --- | --- |
| planner | investigator | xhigh | 232 | 297,365 | 24,531,030 | 115,789 | 24,944,416 |
| closer | closer | xhigh | 180 | 236,986 | 15,986,688 | 71,759 | 16,295,613 |
| planrev | reviewer | xhigh | 104 | 186,522 | 7,478,263 | 42,496 | 7,707,385 |
| ui | implementer | high | 64 | 95,507 | 3,197,541 | 15,264 | 3,308,376 |
| revmsg | reviewer | xhigh | 58 | 105,961 | 2,912,381 | 23,468 | 3,041,868 |
| revui | reviewer | xhigh | 40 | 95,549 | 1,945,989 | 18,398 | 2,059,976 |
| revreport | reviewer | xhigh | 36 | 86,699 | 1,644,635 | 14,245 | 1,745,615 |
| msg | implementer | high | 30 | 68,345 | 1,237,175 | 9,143 | 1,314,693 |
| report | implementer | high | 20 | 59,016 | 749,206 | 4,886 | 813,128 |
| **all members** | | | 764 | 1,231,950 | 59,682,908 | 315,448 | **61,231,070** |

Every member ran on claude-opus-5-5. The metrics do not yet include this reporter or the lead itself.
