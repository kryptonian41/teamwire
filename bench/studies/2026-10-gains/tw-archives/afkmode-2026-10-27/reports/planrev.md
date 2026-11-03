# Plan review: AFK mode plan for team afkmode

## Summary

The plan is mostly sound. The five units touch separate files, and the spec's
`owns` lists match the plan's table exactly (58 paths, none twice). The
amendment patch applies cleanly to afk/base (`git apply --check` passed). Every
name in the task's "Names the design does not give" list reaches a brief with
the right spelling and signature. The plan's claims about the code hold where I
tested them. Doctor and install compare every rendered plugin file, so a second
skill needs no doctor change. `--add` accepts an open team with no members.
Close's `team_plan` refusals feed the dry run, the close itself and the
`close.refused` event. `StatusView` and `TeamEntry` are each built in one
place only.

Nine gaps follow, ranked by harm. One is serious. The full gate that the
plan relies on for the end-to-end tests, the crash tests and `cargo doc` stops
early inside tmux, and the plan says to accept that failure. Two are medium.
The output that `teamwire afk start` prints on success has no test, and the
start unit is told to reuse code that sits inside the budget unit's file.

Files referred to below:
- plan = `$TEAMWIRE_HOME/teams/afkmode/afk/plan.md`
- decisions = `$TEAMWIRE_HOME/teams/afkmode/afk/decisions.md`
- briefs = `$TEAMWIRE_HOME/teams/afkmode/afk/briefs/<role>.md`
- design = `docs/design/2026-10-19-afk-mode.md` in the repository

## Findings, most harmful first

### 1. High: the closer's full gate inside tmux stops at the first failing test, so the e2e tests, the crash tests, `cargo doc` and the docs check never run

Where: plan:165-167 ("run it with `TMUX` and `HERDR_ENV` unset in a shell whose
parents hold no tmux, or read that one failure as known (D23)"); plan:180
("Steps 4 ... and 5 ... run only in the closer's full gate on the joined
branch"); decisions:114-117 (D23).

Failure:
- `scripts/gate.sh` step 4 runs `cargo test --workspace --locked` with no
  `--no-fail-fast` and no `--skip`. D23 itself records that cargo then "stops
  at that binary and skips every later test target".
- Cargo runs packages in name order. `teamwire-cli` comes before
  `teamwire-close`, `teamwire-crash-tests`, `teamwire-e2e`, `teamwire-launch`,
  `teamwire-mcp`, `teamwire-msg`, `teamwire-report` and `teamwire-watch`. None
  of those run after `tmux_wiring::herdr_env_wins_over_tmux` fails, and the
  doc tests do not run either.
- `gate.sh` exits at the first failed step, so step 5 (`cargo doc` with
  `-D warnings`) and step 6 (`check-docs.sh`) never run.
- Unsetting `TMUX` and `HERDR_ENV` does not help. The test's own comment
  (`crates/teamwire-cli/tests/tmux_wiring.rs:341-343`) says the test's
  parent processes decide first. A closer in a tmux pane has tmux among its
  parents, and the plan gives it no command that avoids this.
- The result: a closer that "reads that one failure as known" reports a
  full gate on the joined branch, yet nothing from `tests/e2e` and
  `tests/crash` ran. Those are the only tests that the plan's coverage claim
  (plan:180, plan:127) names for the zero-member readers and the new JSON
  keys. The design (line 85) wants this full gate as the evidence.

### 2. Medium: the success output of `teamwire afk start` (design 5.2 step 7) and its exit-2 path have no test

Where: start brief:42-58 (the five stdout lines, `failed: <what>; team <team>
exists`, exit 2); decisions:59-67 (D12, D13); plan:140 (the AFK-4 tests).

Failure: D13 puts the success path in the launch crate. The launch crate
returns data and prints nothing. The CLI tests cover only the refusals that
come before the team exists. No named test runs the CLI's printing of D12's
five lines, the `warning:` lines on stderr, exit 0, or the exit-2 message. The
skill brief (skill brief:29-30) and `docs/api/cli.md` quote D12's lines word
for word. A start implementation that prints `budget ends at:` or writes the
lines to stderr passes every named test, and the skill and the docs then
quote output that the binary does not print. The task's end state (task.md,
"everything that sections 5 and 6 of the design describe exists and is
tested") includes design line 124, step 7.

### 3. Medium: the start unit is told to reuse the `lead_cwd` lookup, but the lookup sits inside a private function in the budget unit's file

Where: start brief:138-139 ("`member::create_team`'s `session_cwd` lookup:
reuse them inside the launch crate"); start brief:184 ("Owned by others, never
edit: `crates/teamwire-launch/src/member.rs` ... (budget)").

Failure: the lookup is inline code in the private `fn create_team`
(`crates/teamwire-launch/src/member.rs:92-120`). The `NewTeam` built from the
lead plan sits in the same function. Start can reach it only in two ways:
- Start moves the code into a `pub(crate)` helper in `member.rs`. That edits
  budget's file, and budget edits `launch_all` at lines 31-81 of the same
  file. The two branches then conflict at merge.
- Start copies the lookup into `afk.rs`. That breaks AGENTS.md:57 ("Extend
  the nearest existing helper instead of adding a parallel one"), so the
  reviewer flags it.

Either way, start cannot finish its unit as briefed.

### 4. Medium-low: closestat's unit order guarantees a commit that changes `--json` output without its zod copy

Where: closestat brief:128 ("Units in order, commit after each"); closestat
brief:147-159 (unit 2 changes `status --json`, unit 3 changes `teams --json`,
and unit 4 is the zod copy: "Commit this with the Rust change of unit 2 or 3
if you prefer").

Failure: AGENTS.md:60-61 and design line 179 both require the change to
`ui/src/shared/teamwire.ts` in the same commit as the `--json` change. Units 2
and 3 are two commits, and each changes JSON output. Unit 4 can join at most
one of them. "If you prefer" also turns a hard rule into an option. Followed
as written, the branch holds at least one commit whose `status --json` or
`teams --json` carries a key that the zod schema in that same commit does not
know about.

### 5. Low: the budget-spent hold note drops the instruction to decide and record

Where: stophold brief:45-49 and decisions:80 (D16, the past-the-budget note).

Design line 150: "Past the budget, the note starts with `budget spent at
<time>:` and tells the lead to finish what is running and write the report."
"The note" is the base note quoted just above it, with a prefix and an added
instruction. D16 rewrites the body instead. The part "If you were about to ask
the user, decide it yourself, add it to <dir>/afk/decisions.md, and carry on"
is gone. Failure: past the budget, a lead that ended its turn on a question to
the user is told "nobody will answer you", but not told to answer the question
itself and log it. Past the budget the lead is still waiting on the reporter
and the closer, so it still makes decisions. Those decisions then go missing
from `decisions.md` and from the report. A second, smaller mismatch: D16's
note starts with `[teamwire:<team>]`, not `budget spent at`. So the
required test name `past_the_budget_the_afk_note_starts_with_budget_spent`
(plan:145) describes text the note does not have.

### 6. Low: the AFK skill's install test lives in `src/`, and nothing tests that doctor checks the AFK skill

Where: skill brief:145-148 ("Test in `plugin.rs`'s tests module:
`the_afk_skill_is_a_second_stamped_skill_in_the_plugin`"); plan:148 (AFK-12:
"doctor's plugin check covers it").

Failure:
- task.md:34 says "Add tests for each part, in each crate's `tests/` folder
  for Rust." The one named install test for the skill unit is a unit test in
  `crates/teamwire-harness-claude/src/lead_setup/plugin.rs`, not in
  `crates/teamwire-harness-claude/tests/`.
- None of AFK-12's four tests checks the doctor part of AFK-12. The doctor
  part would need a test where an installed plugin missing
  `skills/afk/SKILL.md`, or holding an edited copy, makes `plugin_problems`
  report a problem. The requirement row then cites tests that do not cover
  half of what it states, and design 5.6 ("Doctor checks it the same way it
  checks the main skill") is untested.

### 7. Low: the "fails without the change it covers" criterion cannot hold for several named tests

Where: every brief's "Done when": start brief:71-72, budget brief:53-54,
stophold brief:59-60, closestat brief:72-73, skill brief:77-78.

Failure: these named tests check that existing behaviour stays the same, and
they pass on afk/base with no change:
- `a_team_without_a_budget_is_unaffected` (budget brief:115)
- `an_ordinary_team_gets_no_model_warning` (budget brief:120)
- `removing_one_member_of_an_afk_team_needs_no_report` and
  `an_ordinary_team_closes_without_a_report` (closestat brief:142-143)
- `add_to_an_afk_team_with_no_members_starts_the_member` (start brief:161).
  plan:66-68 states that today's code already accepts this case.

An implementer cannot meet the criterion for these tests. It either reports
the criterion as unmet, or bends the test so that it fails on the base.

### 8. Low: two checks skip the crates that depend on the changed crate

Where: budget brief:139 and plan:155 (`cargo test -p teamwire-launch`);
plan:156 (stophold runs `-p teamwire-report -p teamwire-cli`).

Failure: `teamwire-cli` is the only crate that depends on `teamwire-launch`.
Budget's change to `launch_all` runs on every `launch --add`, yet budget never
runs the CLI's launch tests. `teamwire-mcp` depends on `teamwire-report`, and
stophold never runs its tests. AGENTS.md:25 asks each member for
`scripts/test-changed.sh`, and D23 overrides that. So these dependents meet
the change first in the closer's full gate. Finding 1 shows that gate stops
before `teamwire-mcp`. The CLI's `launch.rs` tests sort before `tmux_wiring`,
so they still run there. The mcp tests do not.

### 9. Low: a decision log written in the skill's own example layout counts as zero decisions

Where: decisions:90 (D18: count the lines matching `^D[0-9]+` followed by
white space); design lines 301-304 (the example entry, indented by four
spaces).

Failure: design section 6 shows the decision entry as an indented block
(`    D3  2026-10-19 14:05  ...`). The skill brief keeps section 6's content and
says nothing about column 0. A lead that copies the layout as shown writes
`    D1 ...`. `teamwire status` then prints `0 decisions`, and the `afk`
object in the JSON says `"decisions": 0` for a log that holds entries.

### Also noted, the smallest

D21 and the skill brief (skill brief:34-35) set the main skill's pointer line
to two sentences: "For a run the user will not be present for, use
`/teamwire:afk`. Only a Claude Code lead can run it." Design line 187 gives
the line as the first sentence only. The extra sentence harms nothing, but the
text differs from the design's quoted line.

## Checked and sound

| Claim or rule | Result |
| --- | --- |
| The patch applies to afk/base | `git apply --check` is clean. The patch adds `Afk(AfkArgs)`, which the start brief's "do nothing" grep looks for. |
| No file has two owners | The plan's table matches the spec. 58 paths appear, none twice. |
| Budget can read the team row | `launch_all` can read the row through `ctx.store`. `planning_warnings` can read it through `cx.existing.team` (`validate.rs:24-29`). |
| `--add` accepts a team with no members | Shown at `validate.rs:56-79`. |
| Doctor checks a second skill file with no doctor change | `plugin_problems` calls `plugin_matches`, which compares every rendered file (`lead_setup.rs:188-214`, `plugin.rs:135-140`). |
| Close sees the report check on every path | `close_observed` uses `plan_close_with`'s refusals (`close/src/lib.rs`). |
| A new field cannot break another construction site | `StatusView {` is built only with `..StatusView::default()`. `TeamEntry {` is built only in `describe`. |
| MCP tests and the e2e tests compare whole JSON objects | They do not. They read single keys. |
| tidy fails on `PROTECTED-CHANGED` for the new ADR, or on `WEAKENED` for an `#[expect(` | It does neither. Both are reported with exit 0 (`xtask/src/base.rs:1-6`). |
| `check-docs.sh` checks test names or requirement prefixes | It does not. It checks structure only. |
| Room in `doctor.rs` for `launch_checks` | tidy counts 706 non-test lines, so about 94 lines are left before the 800-line fail. That room is tight but enough for the brief's shape. |
| Codex plugin tests pin the main skill's text | They do not. They use their own `SHARED` fixture. The pointer line cannot break them. |

## Questions for the parent

None.

## Decisions I made

- I ran `cargo xtask tidy` once in the main clone to read the non-test line
  counts for `doctor.rs`, `member.rs` and `lead_setup.rs`. tidy only reads,
  and it reported `tidy: ok`. I ran no cargo tests, no gate and no build of my
  own. My brief names no check, and the role rules leave test runs to the
  members and the closer.
- I judged the plan against the task's rule that every implementer uses
  sonnet (D5). So I did not count the design's opus table as unmet for this
  run.
- I judged that a Codex lead able to run `teamwire afk start` is out of scope,
  as design section 7 says, and left it out of the findings.
