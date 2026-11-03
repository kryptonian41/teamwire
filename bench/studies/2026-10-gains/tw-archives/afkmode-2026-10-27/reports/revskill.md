# Review: branch afkmode/skill (the AFK skill, its install, and the docs)

## Summary

The work is sound. The skill keeps every rule of design section 6. The
install code is correct. The five named tests exist with the plan's exact
names, each fails without its change, and none is stubbed. Every test name
that docs/failure-modes.md cites exists, with that name, on the branch the
plan gives it.

The findings are all in the docs. One is medium: two docs tell a user that
`teamwire close <team>` ends a team left behind by a failed `afk start`.
closestat's close refuses that team. One is low-medium: ADR 0025 makes a
false claim about older binaries. The ADR is protected, so a later fix needs
a new ADR. The rest are low: single sentences that differ from the code
the other branches built.

Check re-run by me on the skill worktree (ef5d4c7):

```
cargo test -p teamwire-harness-claude -p teamwire-cli --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux
  -> test exit 0, 440 passed, 0 failed (all 7 new or changed AFK tests ok)
scripts/check-docs.sh -> docs check: ok
scripts/gate.sh fast  -> fmt ok, tidy ok (PROTECTED-CHANGED docs/adr/0025-afk-runs.md, expected;
                         WEAKENED crates/teamwire-cli/src/cmd/afk.rs:14, the lead's stub), clippy ok, exit 0
```

## Findings, most harmful first

### 1. Medium: the docs say close ends a team left by a failed start, but close refuses it

- `docs/api/cli.md:138-140` says: "A failure after the team row exists, such
  as the task copy, prints `failed: ...` ... The team stays open, and
  `teamwire close <team>` ends it."
- `docs/failure-modes.md:155` says: "the row stays so `teamwire close` can end it".
- The team that a failed start leaves behind is open and has
  `afk_budget_ends_at` set. closestat's close refuses every open AFK team
  that has no `afk/REPORT.md` with all eight headings, and no flag overrides
  the refusal (afkmode/closestat `crates/teamwire-close/src/plan.rs`, the
  call to `afk_report::refusal` in `team_plan`; D17).
- Failure: make `<home>/teams/t/afk` a regular file, which is the setup of
  `afk_start_that_cannot_copy_the_task_exits_two_and_names_the_team`. Run
  `teamwire afk start --team t ...`, which exits 2. Then run
  `teamwire close t`. Close prints
  `REFUSED: afk_report_incomplete: <home>/teams/t/afk/REPORT.md cannot be read: ...`
  (closestat a1236c0) or `... is missing` (f87ba47), and exits 1. The user
  has to remove the file, create `afk/`, and write a report with eight
  headings before close accepts the team. Neither doc says so.

### 2. Low-medium: ADR 0025 says an older binary opens the database read-only, but it refuses it

- `docs/adr/0025-afk-runs.md:100-101` says: "An older binary opens an AFK
  team's database read only and refuses to write it. That is the existing
  rule of ADR 0015."
- ADR 0015 point 4 says: "A newer schema is still refused with both versions
  named." The store refuses a newer schema on a read-only open too
  (`crates/teamwire-store/src/store.rs:158`, `:186`, `:220`,
  `StoreError::SchemaNewer`).
- Failure: a schema-4 binary runs `teamwire status <afk team>` on a schema-5
  database. The binary refuses with `SchemaNewer` (exit 6, "schema newer than
  this binary"). The binary never reads the database read-only.
- Why this matters more than a normal doc error: docs/adr/ is protected, and
  AGENTS.md says an ADR changes only through a new ADR that supersedes it.

### 3. Low: the skill misstates the budget-spent hold note instead of quoting D31

- Brief, section 2 item 1: "The budget-spent hold note is D31's text."
- `examples/skills/afk/SKILL.md:189-191` gives a paraphrase instead of the
  text: "Past the budget, the note starts with `budget spent at <time>:` and
  tells you to launch no new implementer, finish what is running, and close
  once the report exists."
- The real note starts with the team tag:
  `[teamwire:<team>] budget spent at <ts>: AFK run: ...`. D31 says this ("The
  team tag stays first"), and so does afkmode/stophold
  `crates/teamwire-report/src/afk_hold.rs`, `note()`. The paraphrase also
  leaves out "add the reporter", which the real note says.
- `docs/api/cli.md` quotes D31 correctly. Only the skill is wrong.

### 4. Low: failure-modes says doctor warns about an edited AFK skill, but doctor fails

- `docs/failure-modes.md:178` (AFK-12) says: "A problem is reported, so
  doctor warns".
- `crates/teamwire-cli/src/cmd/lead_setup_check.rs:51-55` turns any
  `plugin_problems` entry into `Outcome::Fail`. Through start's
  `launch_checks`, that FAIL then refuses `afk start` with `doctor_failed`.
- Failure: install, then edit one line of the installed `skills/afk/SKILL.md`.
  `teamwire doctor` prints a FAIL for `teamwire plugin` and exits 2.
  `teamwire afk start` prints
  `refused: doctor_failed: teamwire plugin: ...`. Neither command gives a warning.

### 5. Low: the skill says every `failed:` line names a team that now exists

- `examples/skills/afk/SKILL.md:35-37` says: "A line that starts with
  `failed:` exits 2 and names a team that now exists without its task copy."
- afkmode/start `crates/teamwire-launch/src/afk.rs:207` and `:217` also return
  `AfkError::Failed`, which the CLI prints as `failed: ...` with exit 2.
  The two messages are "the team's database could not be created" and "the
  team could not be recorded". In both cases no team row exists, and the
  line does not name the team.
- Failure: the database write fails, for example on a full disk. The lead
  follows the skill and tells the user that team `<name>` exists without its
  task copy. No such team exists. The lead's action (tell the user and stop)
  is still right, so the harm is only the wrong message to the user.

### 6. Low: lines of docs/api/cli.md that differ from the joined code

Each of these is one sentence in cli.md that a reader would act on.

- `docs/api/cli.md:215` says the status count is lines "that start with `D`,
  a number and white space". D35 and closestat's `decision_lines` also count
  an entry with leading white space (`^\s*D[0-9]+\s`). Example: an entry
  indented four spaces counts as a decision, but cli.md says it does not.
  `docs/api/core.md` gives the rule correctly.
- `docs/api/cli.md:335` lists two forms of `afk_report_incomplete`.
  closestat's latest commit a1236c0, which came after the skill branch, adds
  a third form: `<path> cannot be read: <error>`. The skill member could
  not have seen it. The closer should add it to the doc.
- `docs/api/cli.md:105` says the start "stops at the first step that fails,
  in this order". The table lists every code except `unknown_harness`
  (afkmode/start `crates/teamwire-launch/src/afk.rs:98`) and `unknown_host`
  (`lead_plan`, `validate.rs:232`). Both codes come from start's own code.
  A misconfigured lead can get them before the name check.
- `docs/api/cli.md:16`, the exit-code table, still lists for code 2 only
  launch, reopen, verify, close, install and uninstall. `afk start` now
  exits 2 too.
- `docs/api/cli.md:514`, the uninstall row, still says the plugin directory
  holds "both skill files". The directory now holds three skill files.

### 7. Low: wrong citations or detection text in the new docs

- `docs/adr/0025-afk-runs.md:3` cites "ADR 0001 (one team is one directory
  and one database)". ADR 0001 is "Process model — one binary, no daemon".
  The one-database-per-team rule is ADR 0002 ("Storage — one SQLite database
  per team").
- `docs/failure-modes.md:149` says a taken name is detected when "The team
  directory exists". afkmode/start `afk_check` checks that
  `database_path(home, team)` is a file. A team directory without `team.db`
  is not treated as taken.

## What I checked and found sound

- Section 6 rules: every rule is in the skill. I compared each paragraph of
  design section 6 with `examples/skills/afk/SKILL.md`. The implementer's
  table in reports/skill.md is accurate. The frontmatter is the design's text
  with no `metadata` key. The decision example is at column 0 in a fence
  (D35). The eight `##` headings match D17 and closestat's `HEADINGS`. The
  pointer line is D36's sentence only.
- Quoted output matches the other branches:
  - The five success lines (D12) and `refused: <code>: <message>` with
    `nothing was created` match start's `cmd/afk.rs`.
  - `afk_budget_spent` text and the model warning match budget's
    `member.rs` and `plan_rules.rs`. Launch prints the refusal with a
    `<role>: ` prefix in front of the quoted text.
  - Both hold notes in cli.md match stophold's `note()`, character for
    character.
  - Both status lines, `(open, afk)`, `afk_budget_ends_at`, and `"afk": null`
    match closestat.
  - `REFUSED: afk_report_incomplete:` matches close's printer.
  - `--budget` units s/m/h/d with default 8h match `parse_duration` and
    args.rs.
- Install code: `plugin.rs` stamps `AFK_SKILL_TEXT` the same way it stamps
  the main skill. The file is included in the version hash and compared by
  `plugin_matches`. The test that deletes and edits the AFK file would fail
  without the render change: `remove_file` panics on a missing file.
- Tests: `the_afk_skill_lists_the_eight_report_headings` asserts the exact
  order. `the_main_skill_points_at_the_afk_skill` fails without the line.
  The command test reads fenced and inline commands through the existing
  `shown_commands` helper. None of the tests is a guard, and none passes
  without its change.
- AGENTS.md hard rules: there is no unwrap or expect outside tests, and no
  printing in a library. Every changed file is in the skill's owns list. The
  one protected file is ADR 0025, which the brief allows. The diff adds no
  dependency.
- Zero-member AFK team: the skill branch adds no code that reads team state,
  so no reader of an empty team is involved.

## Note for the lead (inherited from the design, not a defect of this branch)

The skill's preflight item 4 (`SKILL.md:52`) says "Then start step 2 in the
same turn." Item 2 of the same list is "run `teamwire afk start`". The
design's text has the same ambiguity. If a lead reads "step 2" as that item,
it runs the start again and gets `afk_already_running`. The AFK hold then
fires once and tells the lead to carry on, so the run recovers. I list this
for the lead to decide; it is not a finding against the implementer.

## Decisions I made

- I re-ran the skill branch's check myself, on its worktree, as a closer step,
  because the verify directory has no skill.log. I edited nothing and
  committed nothing.
- I compared the docs with each branch's head at review time (start e026e22,
  budget 34098e1, stophold 94e4675, closestat a1236c0). Finding 6's third
  form of the close refusal comes from closestat a1236c0, which was committed
  after the skill branch.
- I report the "start step 2" ambiguity as a note, not a finding, because the
  design's agreed text has it and the brief says to keep that text.
