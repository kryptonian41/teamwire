# Decisions

D1  2026-10-22 15:10  Which members does this run use?
Chose: planner (investigator, opus, xhigh) -> plan reviewer (reviewer, opus, xhigh) -> three implementers in parallel, one per fix (report, msg, ui; opus, high) -> one reviewer per branch (opus, xhigh) -> closer (opus, xhigh) -> reporter (investigator, opus, high).
Why: the three fixes touch three separate areas (crates/teamwire-report, crates/teamwire-msg, ui/), so the parallel-tickets shape fits with no shared files. The planner reads the code, writes the understanding, the briefs, the checks and the spec, so the lead reads no source. The plan reviewer is cheap insurance against a misread of the resend rules in fix 2.
Skipped: analyst, because the task states each end state exactly and the planner writes afk/understanding.md as part of its job. Scouts, because each area is one small module and the planner answers the open questions in the plan.
Reverse by: add an analyst or scouts with teamwire launch --add if the planner reports questions it cannot settle.

D2  2026-10-22 15:10  Where does the finished work live?
Chose: an integration branch afk/fixes3 cut from the checkout's current branch bench (commit e78c32b), with the three implementer branches merged in.
Why: the task asks for one branch that holds the finished work, and the checkout has no main branch.
Reverse by: the three implementer branches are kept, so each can be used on its own.

D3  2026-10-22 20:50  Where does the shared transcript rule live (fix 1)?
Chose: move the usage reader's inline check (crates/teamwire-report/src/usage.rs:45-49) into one function in the same file, `pub(crate) fn read_transcript(path: &Path, used: u64) -> Option<(u64, Vec<u8>)>`. The usage loop calls it with the bytes read so far. The final-message fallback (final_message.rs:25) calls it with 0. MAX_TRANSCRIPT_BYTES stays in usage.rs.
Why: AGENTS.md says to extend the nearest helper instead of adding a parallel one. A new module would move the constant away from the usage unit tests that use it. A prototype of exactly this passed all 9 test binaries of teamwire-report and the fast gate.
Reverse by: move the function and the constant into a new transcript.rs; both callers change one path.

D4  2026-10-22 20:50  What does a resend report for a receiving-team row the task does not name (fix 2)?
Chose: a row that is `unconfirmed` there reports like `pending`: `Unconfirmed` with reason `TeamClosed`, and the sender's row becomes unconfirmed. A row that is `undelivered` there reports like a missing copy: `Undelivered` with reason `TeamClosed` (the existing `gone` outcome). For the two new outcomes, the error code is `TeamClosed`, and the error detail says the team is closing or closed and that an earlier attempt may have reached it. The row's route is kept.
Why: `unconfirmed` means the line may have arrived, so the row must never become undelivered (INV-5, remote.rs:186-188). `undelivered` means the line surely did not arrive, and the reason the resend cannot help now is that the team is closed.
Reverse by: change the two match arms in the new helper in remote.rs.

D5  2026-10-22 20:50  Where does the fix 2 change go?
Chose: in `deliver_to_team` (crates/teamwire-msg/src/remote.rs:66-71), in the branch where `receive` finds the team not open. `send` and `resend` share this function.
Why: the branch is the one that "records undelivered without looking". For `send`, the branch changes nothing: a new message has no copy in the other team, and `send` refuses a closed recipient team earlier with `refused.recipient_team_closed`. A `send` retried with the same client key reuses the stored message, and it now gets the same honest outcome as `resend`.
Reverse by: pass a flag from resend.rs to deliver_to_team and keep `gone` for send.

D6  2026-10-22 20:50  What does the question label say when n > 1 (fix 3)?
Chose: the label is `<n> question waiting` for every n, word for word as in the task's table, for example `2 question waiting`.
Why: the task gives the label as an exact string, and a closer will check it word for word. Plural wording is a later change to one template string.
Reverse by: change the template string and its test.

D7  2026-10-22 20:50  Which questions count, and what is "open" (fix 3)?
Chose: a question counts when its `to` list contains the exact party `lead` and its `age_s` is 600 or more. `lead@<team>` is another team's lead and does not count. A question to the lead and a member counts. "Open" means `status.state === "open"`, so a `null` state (an unreadable team) shows none of the three open-only reasons.
Why: in a team's own status, its lead is the party `lead` (ui/src/shared/teamwire.ts:27-31). The task says "only while the team is open", and `null` is not open.
Reverse by: widen the filter in problemReasons.

D8  2026-10-22 20:50  How does the overview card show the reasons, and how is that tested (fix 3)?
Chose: TeamRow in ui/src/web/Overview.tsx renders one `<span className="flag">` per reason, in order, inside a `<p className="chips">` in the card body. The existing `.flag` style is used, so no CSS changes. A new ui/src/web/Overview.test.ts renders Overview with `renderToStaticMarkup` and `createElement` (a .test.ts file holds no JSX) and checks that the reasons appear.
Why: the task says the card shows the reasons, and only a render test proves that. A prototype of this test passed under `bun test` with the installed react-dom.
Reverse by: delete the paragraph and the test file.

D9  2026-10-22 20:50  The existing sort test breaks once hasProblem counts unread messages and questions (fix 3).
Chose: in ui/src/web/model/teams.test.ts:42, the "quiet" team also clears `unread_deliveries: []` and `unanswered_questions: []`. Its expected order stays `["Zzz", "Alpha", "Zed"]`.
Why: the test builds "quiet" by spreading the payments fixture, which has 1 unread delivery and 1 question to the lead waiting 2100 s. Under the new rule that team is a problem, and the prototype got `["Alpha", "Zed", "Zzz"]`. The test means "a quiet team sorts after a problem team". Clearing the two lists keeps that meaning and the expected value. Nothing is deleted or skipped.
Reverse by: none needed; the old input is no longer quiet.

D10  2026-10-22 20:50  Which promptwire skills do the members get?
Chose: none.
Why: `promptwire search` found no Rust, SQLite or Bun skill. It found two React skills (vercel-react-best-practices, vercel-composition-patterns). Both cover performance and component design for larger apps. The ui member adds one pure function and one list of spans. The promptwire skill says a skill that does not cover what the member builds hurts more than it helps.
Reverse by: install a skill into teams/fixes3/skills and name it in the brief.

D11  2026-10-22 20:50  What does each member's check run?
Chose: report: `cargo test -p teamwire-report --locked && TEAMWIRE_BASE=e78c32be scripts/gate.sh fast`. msg: the same with teamwire-msg. ui: `cd ui && bun test && bun run typecheck`, with setup `cd "$1/ui" && bun install --frozen-lockfile`. The two Rust members have setup "".
Why: the task names the crate tests and the fast gate. The gate covers the Rust code only (ui/AGENTS.md). ui/package.json has no lint script, and neither the gate nor ui/AGENTS.md runs a lint. Each check passed on a fresh clone of bench (plan.md).
Reverse by: edit the check field in spec-wave1.json before launch.

D12  2026-10-22 20:50  Do the implementers run scripts/test-changed.sh?
Chose: no. Each member runs its check and the fast gate only. The lead runs the full gate after integration.
Why: for either Rust crate, test-changed.sh tests teamwire-cli, teamwire-mcp, teamwire-e2e and teamwire-crash-tests too. No test in those crates sends to a closing team or passes a transcript without a final message (grep in plan.md). I ran the dependents on both prototypes. The only failure, `herdr_env_wins_over_tmux`, is caused by the environment (plan.md).
Reverse by: add `scripts/test-changed.sh <crate>` to the brief's verification.

D13  2026-10-22 20:50  The msg member also updates the two resend docs.
Chose: msg owns docs/api/cli.md and docs/api/mcp.md, and adds one sentence to the `teamwire resend` section (cli.md:344-351) and to `team_resend` (mcp.md:110-114) for a team that is closing or closed.
Why: both paragraphs describe a resend to another team. After fix 2 they no longer say what happens when that team is closing. The only test that reads mcp.md checks its `## team_*` headings (crates/teamwire-cli/tests/preset.rs:419-434), which stay.
Reverse by: drop the doc sentence; the code change does not depend on it.

D-L1  2026-10-22 15:25  How to treat the teamwire-cli test herdr_env_wins_over_tmux, which fails on unchanged bench e78c32be?
Chose: treat it as a failure that exists before this run. The closer runs it on the base commit too and records both exit codes. No member changes that test.
Why: the planner found it fails on the unchanged base when run under tmux and herdr (comms/m001.md, afk/tmp/bench-tmux-wiring.log). The test's own comment says it holds only with no tmux or herdr parent process. Changing it is outside the task.
Reverse by: run the full gate from a shell with no tmux or herdr parent.

D-L2  2026-10-22 15:50  Does a resend to a closing or closed team keep D4, where a receiving-team row that is `undelivered` is recorded undelivered?
Chose: keep D4.
Why: the task's sentence "Only a missing copy is recorded undelivered" contrasts the new rule with today's rule, which records every recipient as undelivered without looking. A row that is `undelivered` in the receiving team means the line surely did not arrive. Reporting it as unconfirmed would tell the sender that the line may have arrived, which is false. The plan reviewer raised this as finding 1 in reports/planrev.md.
Reverse by: change that one match arm in the new helper in crates/teamwire-msg/src/remote.rs and its test in tests/cross_team.rs.

D14  2026-10-22 21:40  The plan reviewer's question: does D4 hold for a receiving-team row that is `undelivered` (reports/planrev.md, finding 1)?
Chose: yes, and the lead already decided it as D-L2. My answer changes no brief beyond what D-L2 asks. briefs/msg.md now carries D-L2 under "Decisions already made", names the test `a_copy_that_surely_never_arrived_stays_undelivered_when_its_team_closes`, and requires a comment on that match arm in remote.rs.
Why: the only states a resend can report are handed off, unconfirmed and undelivered. An `undelivered` row there means the line surely did not arrive, so `handed_off` and `unconfirmed` would both state something false. The task's "only a missing copy" sentence contrasts the new rule with today's, where no row is read at all.
Reverse by: as D-L2 says, change that one match arm and its test.

D15  2026-10-22 21:40  How does the overview test find one team's card (reports/planrev.md, finding 2)?
Chose: take a named team's card as its link element, `<a class="team…" href="#/team/<name>">…</a>`, with a non-greedy match up to the first `</a>`. Check each reason as `>reason<` on its own card and on no other card, with at least two teams (the fixtures give payments, docs and lost).
Why: a card holds no other link, so the match is exactly one card. Splitting the page on `<li>` cuts the `lost` card in two, because it lists its errors as `<li>` items. I ran the test on a clone with the ui prototype. It passed on the correct card. It failed with payments' reasons also on the docs card, and it failed with the reasons only in the page heading (afk/tmp/Overview.per-card.test.ts).
Reverse by: export TeamRow and render one card at a time.

D-L3  2026-10-22 16:40  Should this run write a new ADR to supersede ADR 0016's sentence about a team that closes?
Chose: no. The report lists it as not done.
Why: docs/adr/0016-messages-between-teams.md:89-90 still says a team that closes leaves the sender's row undelivered. After fix 2 that is true only when the closing team holds no copy, or a copy that is undelivered. docs/adr/ is protected, AGENTS.md allows an ADR change only through a new superseding ADR, and the task does not ask for one. The resend reviewer raised it in reports/revmsg.md.
Reverse by: add a new ADR that supersedes that sentence of ADR 0016.

D-L4  2026-10-22 17:20  Which unread deliveries count toward the dashboard reason `<n> unread` (closer finding F1)?
Chose: count only unread rows whose recipient is in this team, meaning a `to` with no `@team` suffix. This is the closer's option (a).
Why: the sender's copy of a message to another team stays `handed_off` forever by design (ADR 0016, remote.rs:10-11), and the status view lists it as unread forever. Counting it would keep the sender's team under "needs a look" forever, even after the team closes. The watch's own UNREAD alert already skips recipients outside the team (teamwire-watch snapshot.rs:144-145), so the dashboard now follows the same rule. Changing the status view itself (option b) would reach outside the task and change what `teamwire status` shows.
Reverse by: drop the filter in problemReasons in ui/src/web/model/teams.ts.
