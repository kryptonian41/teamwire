# planrev: review of the fixes3 plan

**Summary**
1. The plan is sound. All three briefs match task.md for every end state, label, order and resend case I compared. I found no ownership overlap, no file without an owner, and no wrong base or branch.
2. Finding 1 (medium-low, fix 2): decision D4 records an `undelivered` copy in beta as undelivered. The task says "Only a missing copy is recorded undelivered", and a new test will pin the opposite. The parent should confirm the reading before launch.
3. Finding 2 (low, fix 3): the planned render test for the overview checks that each label appears somewhere on the page. The test does not check that the label sits on the team's own card, which ui.md:40 asks for.
4. The ui member's `check` field leaves out the fast gate. That omission costs nothing: the gate reads only Rust files and protected paths, so a change under `ui/` cannot change its result. ui.md:42 and ui.md:133 still make the member run it.
5. I checked every line reference that the three briefs give. Each one points at the code it describes on `bench` (e78c32be).

## Progress

- Read task.md, understanding.md, plan.md, decisions.md, the three briefs, spec-wave1.json and the three prototype diffs.
- Read the code each brief names: `crates/teamwire-report` (usage.rs, final_message.rs, turn_end.rs, done.rs, result_file.rs, tests), `crates/teamwire-msg` (remote.rs, resend.rs, send.rs, rules/foreign.rs, tests/cross_team.rs), the store (received.rs, delivery.rs, team.rs, store.rs), `ui/` (teams.ts, teams.test.ts, Overview.tsx, teamwire.ts, fixtures/status.json, AGENTS.md), `scripts/gate.sh`, `scripts/check-docs.sh` and `xtask/src/base.rs`.
- Ran no build or test. My brief names no check, and the planner's logs in `afk/tmp/` cover the runs.

## Findings, most serious first

### 1. The plan's handling of an `undelivered` copy contradicts a sentence of the task (fix 2, medium-low)

- Task line, fix 2 end state: "Only a missing copy is recorded undelivered, as today."
- Plan lines: decisions.md:19-22 (D4), plan.md:57, msg.md:24 (table row `undelivered (D4) → Undelivered, reason TeamClosed`) and msg.md:33 (a new test that pins it).
- Failure case: alpha stores m001 for `db@beta`. Beta already holds its copy, and the row for `db` is `undelivered`. Beta starts closing. A resend by `api` reaches the new branch in `deliver_to_team` (remote.rs:66-71). Under D4, the branch returns `gone(team)`, and `mirror` writes alpha's row as `undelivered` with `TeamClosed`. The copy exists, so a literal reading of the task forbids that outcome.
- The planner's reading has a reason behind it. An `undelivered` row in beta means the line surely did not arrive there, and D4 says the task "does not name" this state. The word "Only" in the task does cover it, though. Once the msg member commits the test from msg.md:33, a closer who checks the task word for word will find a test that asserts the opposite of that sentence.
- I raise this as a question below. I do not propose a change.

### 2. The planned overview test cannot tell whether a reason is on its own card (fix 3, low)

- Task line, fix 3: "The overview card shows the reasons."
- Brief line, ui.md:40: "A new `ui/src/web/Overview.test.ts` renders `Overview` and shows that each reason appears on its card."
- Plan line, plan.md:81: "render `Overview` ... and check that the reasons appear." ui.md:96 points the member at the prototype diff as "one way that passed". In `afk/tmp/ui-prototype.diff`, `Overview.test.ts` runs `expect(html).toContain(r)` against the HTML of the whole page.
- Failure case 1: suppose an implementation prints every team's reasons once, in the page heading next to "2 need a look", and puts nothing on the cards. The planned test passes, and the cards show no reasons.
- Failure case 2: suppose `TeamRow` renders payments' `1 unread` on the docs card. The planned test passes again.
- The brief's wording is stricter than the plan's test. The risk exists only if the member copies the prototype test as it stands.

## Checked and sound

| Area | What I checked | Result |
| --- | --- | --- |
| Fix 1 end state | report.md:17-27 against task.md fix 1: a regular file, at most 64 MiB, the same rule as the usage reader, a pipe and a file past the limit both give `Final message from: none` and `FinalSource::None` through `on_turn_end` | Matches |
| Fix 1 code | The fallback in `final_message.rs:25` is the only reader of the transcript besides `usage.rs:46-49`. Both callers of `pick` (turn_end.rs:156, 237) run before the usage read at turn_end.rs:288-292. A new completion starts with `final_source: FinalSource::None` (done.rs:200-201). So when the shared function refuses, the result is `none` | Plan correct |
| Fix 1 tests | The pipe test runs in a thread and waits 10 s, as tests/turn_end.rs:474-496 does. The sparse files get a `"\n"` before `set_len`. No new `#[expect(` line goes into result_file.rs. usage.rs has exactly 5 unit tests | Correct |
| Fix 2 end state | msg.md:19-25 and :29-35 against task.md fix 2. Handed off and read give `HandedOff`, alpha's row becomes `handed_off`, nothing is typed. Pending gives `Unconfirmed` with `TeamClosed`, and alpha's row becomes `unconfirmed`. A missing copy gives undelivered, as today | Matches, except finding 1 |
| Fix 2, closing versus closed | `insert_received` returns `None` for every state other than `open` (received.rs:43), and `TeamState` has only open, closing and closed. So one branch serves both states. The tests use `begin_close` alone for "closing" and add `finish_close(Completed)` for "closed". `open_existing` still opens a closed team's database (store.rs:153-159) | Correct |
| Fix 2 state moves | `may_move` (delivery.rs:70-78) allows pending, undelivered or unconfirmed to move to `handed_off` or `unconfirmed`. `mirror` never moves a row backward. `report_of` gives no reason for `handed_off` (attempt.rs:254-263), so "reason `None`" holds even for an acked row that kept an old error | Correct |
| Fix 2 typing | The new branch calls neither `write_copy` nor `deliver_one` | Nothing is typed |
| Fix 3 labels and order | ui.md:17-27 against the task table: the lead's liveness label (failed or attention), then `watch stale`, then `AFK spent` (these three only while open), then `<n> unread`, then `<n> question waiting`. The 600 s limit counts 600 itself. A question to another party does not count. `hasProblem` is true whenever the list is not empty | Matches word for word and in order |
| Fix 3 fixtures | payments gives `["1 unread", "1 question waiting"]`, docs gives `["watch stale"]`, and lost gives `[]` (I read fixtures/status.json) | Matches plan.md:81 and ui.md:39 |
| Fix 3, the change to the old sort test (D9) | teams.test.ts:42 spreads payments, which now counts as a problem. The plan clears the two lists and keeps the expected order. Line 41's order `["lost", "payments", "docs"]` stays the same, because all three teams were problems already | Correct |
| Ownership | report owns `crates/teamwire-report/`. msg owns `crates/teamwire-msg/` and the two API docs, plus `preset.rs` read only. ui owns four files under `ui/src/web/`. The lists do not overlap. The only test that reads a doc is preset.rs:419-434, which checks headings only. No other file in `ui/src` reads `hasProblem` or `sortTeams` | No gap |
| Checks | The Rust checks are the crate tests plus the fast gate, as the task asks. The ui check is `bun test` plus typecheck. The fast gate scans only `*.rs` for weakened tests (xtask/src/base.rs:47-50), and its protected paths are all outside `ui/`. So the gate cannot fail on a change under `ui/` alone. The brief still makes the ui member run it | No gap |
| Spec | Base `bench` equals e78c32be. The branches are `fixes3/<role>`. `.worktrees/` is in .gitignore. Every child has `"plugins": {"claude-mem@thedotmack": false}`, as the task's setup line requires | Correct |

## Note with no failure

- D5 (decisions.md:26) gives this reason: "A `send` retried with the same client key reuses the stored message, and it now gets the same honest outcome as `resend`." That reason is wrong. `send` runs `foreign::check` before it stores anything (rules.rs:77), and that check refuses a closing or closed recipient team with `recipient_team_closed` (rules/foreign.rs:41-42). So a retried `send` reaches the new branch only when the team starts closing between that check and `deliver_to_team`. The decision still works and no code depends on the claim. The claim only matters if someone copies it into the docs.
- The later members in D1 (the branch reviewers, the closer and the reporter) have no spec yet. The task's setup line covers them too.

## Questions for the parent

1. The task says "Only a missing copy is recorded undelivered" for a resend to a team that is closing or closed. D4 records undelivered for a copy that exists but whose row is `undelivered` in that team, and msg.md:33 pins this with a test. Do you accept D4's reading, or should an existing `undelivered` copy also avoid the undelivered outcome? See finding 1.

## Decisions I made

- I ran no build or test. My brief names no check, and the planner's fresh-clone logs and prototype logs in `afk/tmp/` already cover the commands the members will run. I judged the plan from the code and those logs.
- I rated the missing fast gate in the ui `check` field as no gap, not as a finding. The reason is that the gate cannot see a change made only under `ui/`.
- I left out one test gap. Fix 2's pending copy is tested only on a closing team, and the read copy only on a closed team. The code path for both states is the same branch, so I found no realistic implementation that would pass these tests and still be wrong.
