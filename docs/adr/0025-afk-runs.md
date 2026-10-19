# ADR 0025: An AFK run is one team, led by a skill, held by the Stop hook

Status: accepted, 2026-10-19.

## Context

The user wants to type one task, leave the machine, and find a result and a
report on return. Members already work with nobody present (ADR 0018 and the
member contract: they run in bypass mode, decide, and list each decision).
The lead does not. The lead skill tells the lead to ask the user, to report
`TEAM-STALLED` to the user, and to wait for approval before a push. A lead
turn that ends on a question ends the run, because nothing wakes the lead
again. The lead also reads every result, so a long run fills its context.

Design: `docs/design/2026-10-19-afk-mode.md`. The numbers below are its
decisions, with the options each one beat.

## Decision

1. **A separate skill starts the run.** Install renders
   `examples/skills/afk/SKILL.md` as a second skill of the teamwire plugin,
   so Claude Code lists `/teamwire:afk <task>`. The skill tells the lead to
   read the main skill first and lists only what changes. The main skill
   gets one line that points at it. Doctor compares the file with the
   plugin's other files.
   - Beaten: a flag on `teamwire launch`. The change is in how the lead
     behaves, and a flag cannot change that.
   - Beaten: a mode in the main skill. The main skill is already past 40 KB,
     and two sets of rules for "ask the user" in one file contradict each
     other.

2. **One AFK run is one team, in the ordinary place.** The team lives in
   `$TEAMWIRE_HOME/teams/<team>/`, with its row in that team's `team.db`.
   The run's shared files go under `teams/<team>/afk/`: `task.md`,
   `understanding.md`, `plan.md`, `decisions.md`, `briefs/`, the wave specs
   and `REPORT.md`. Every member joins with `teamwire launch --add`.
   - Beaten: a separate `afk/` home beside `teams/`, with its own run
     database. Every reader of team state (status, watch, close, reopen,
     verify, metrics, the dashboard, the Stop hook) would then need a second
     lookup, and close would need new code to archive the run's files. The
     user rejected this on 2026-10-19.
   - Beaten: several teams per run. The hold, the archive, the metrics and
     the Stop hook check would spread over several rows. The main skill
     already says to start a new team only for work that is not integrated
     with the first.
   - Files go in `afk/`, not `reports/`, because `reports/` belongs to the
     members and `teamwire verify` checks it per role. Close copies every
     file of the team directory into the archive, so the `afk/` files are
     archived with no new code.

3. **One nullable column marks the team.** `team.afk_budget_ends_at` (schema
   version 5) says that the team is an AFK run and when its budget ends.
   `Tx::create_afk_team` sets it. `NewTeam` gains no field, because 34
   callers construct it.
   - Beaten: a boolean plus a separate budget column. Two columns can
     disagree.
   - Beaten: a table for AFK runs. One value per team needs no table.
   An older binary opens a version 5 database read only (ADR 0015).

4. **`teamwire afk start` creates the team before the first member.** It
   checks the lead session, the names and the checks of `teamwire doctor`,
   creates the empty team, copies the task file to `afk/task.md`, takes the
   sleep hold, and prints the budget end. A `FAIL` refuses the start.
   - Beaten: let the first `launch` create the team. The Mac can sleep
     during planning, before any member exists, and launch takes the hold
     only when the first member starts.

5. **The Stop hook holds an AFK lead once per stop chain.** The hook holds
   the stop when the session leads an open AFK team, nothing will wake the
   lead, and `stop_hook_active` is false. The note tells the lead to decide,
   log the decision and carry on, or to add the reporter and close.
   - Beaten: hold until the team is closed. A lead with a background command
     that will wake it, or one that is truly stuck, could not stop.
   - Beaten: a counter of holds. It needs state the hook does not write,
     and it holds forever or gives up at a number nobody can justify.

6. **A wall-clock budget, enforced at launch.** `--budget` takes `8h` or
   `90m` and defaults to `8h`. Past it, launch refuses each new
   `implementer` with `refused: afk_budget_spent` and accepts every other
   kind, so the run can still review, verify and report.
   - Beaten: a token budget. The user chose wall-clock time.
   - Beaten: refuse every kind past the budget. The reporter must always be
     able to run. New code is what drags a run on, because each implementer
     brings a review, fix rounds and verification.

7. **Members do the research, planning, review, verification and report.**
   The lead picks members from profiles in the skill and records its roster
   as entry `D1` of `afk/decisions.md` before the first launch. Five things
   hold in every run: never wait, protect the lead's context, the closer
   runs once code exists, a reporter writes the report, and the boundaries.
   The planner is an `investigator` on `opus` at `xhigh` that writes only
   under `afk/`.
   - Beaten: a fixed pipeline of phases. Tasks differ too much, and a fixed
     roster either wastes members or skips checks.
   - Beaten: the lead does the planning. Every file the lead reads stays in
     its context for the rest of the run.
   - Beaten: a new member kind for the planner. The investigator seat already
     forbids editing repository files, and `afk/` is outside the repository.
   - The main skill warns against an always-on coordinator member. That
     warning is about a member that decides with less context than the lead
     and then acts. Here each member runs once, writes files, and sends
     work to nobody. The lead makes every call that changes the plan.

8. **Models and effort.** Implementers and investigators run on `opus` at
   `high`. Reviewers, the plan reviewer and the closer run on `opus` at
   `xhigh`. `max` is for one member with a hard task. Every `deadline_s` is
   1.5 times its normal value. Launch prints one warning per member below
   this table and never refuses.

9. **The run ends with `teamwire close`.** Close refuses an AFK team whose
   `afk/REPORT.md` is missing or lacks one of eight headings, with
   `refused: afk_report_incomplete`. The headings are: What you asked, What
   I understood, The result, Verified, Decisions made for you, Not done and
   why, Risks, Cost. The match ignores case and the heading number.
   - Beaten: an `afk finish` command. Close already ends a team in the safe
     order and releases the hold, and a second ending would duplicate it.
   - Beaten: no check. A lead that forgets the report leaves the user
     nothing to read.

10. **No outside effects.** The run never pushes, opens a pull request,
    merges into the base branch or runs `teamwire install`. The result is a
    local branch `afk/<team>` that passed the full gate. The lead calls
    `PushNotification` after close when it has that tool.
    - Beaten: let the run push. A wrong guess about a vague task would then
      reach other people.

11. **Status shows the run.** `teamwire status <team>` gets one `afk:` line
    and an `afk` JSON object, and `teamwire teams` marks the team `afk`.
    - Beaten: an `afk status` command. The run is a team, so the team's
      status is the run's status.

12. **Claude Code leads only.** A Codex lead keeps today's behaviour. Codex
    has its own lead skill and has never run live.

## Consequences

- A reader that assumes an open team has a member must cope with an AFK team
  that has none yet: status, watch, close, reopen, verify, metrics and the
  dashboard.
- A lead that starts a background command of its own is not seen by the
  hook. The hook's note tells the lead to stop again, and the next stop
  passes. A lead that stops twice with nothing running is stuck, and
  `teamwire status` shows the AFK team open with no working member.
- A Claude usage limit pauses every session. AFK mode cannot work around it.
  The report's cost section shows the stop afterwards.
- The skill's section 6 text is the contract. The test that checks the main
  skill names only real commands is extended to the AFK skill when
  `teamwire afk start` is on the integration branch.
