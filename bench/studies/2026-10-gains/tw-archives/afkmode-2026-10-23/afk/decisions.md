# Decisions

D1  2026-10-23 10:05  Which members does this run use?
Chose: a planner (investigator, opus, xhigh) that also writes afk/understanding.md; one plan reviewer (reviewer, opus, xhigh); implementers (sonnet, high) as the plan splits the work; one reviewer per implementer branch (opus, xhigh); the closer (opus, xhigh); the reporter (investigator, opus, high).
Why: the design document already fixes the end state, so one planner can both state the understanding and plan; a separate analyst would mostly re-read the same document. The plan reviewer guards against a misread before several implementers spend their budget. Reviewers, closer and reporter are required or cheap insurance.
Skipped: analyst (folded into the planner, see above); scouts (the planner reads the code and answers the questions itself; implementers can ask the planner, which stays open through the build).
Setup from the task: every member spec has "plugins": {"claude-mem@thedotmack": false}; implementers use "model": "sonnet"; every other member uses "model": "opus".
Reverse by: adding an analyst or scouts with teamwire launch --add before wave 1 of implementers.

D2  2026-10-23 15:20  What to do about the planner whose turns keep dying?
Chose: remove it (close --role planner --unverified --stop) and add planner2 at effort high, which starts from the saved understanding.md and spec-wave1.json and writes one file per step.
Why: three turns in a row ended with "API Error: Your computer went to sleep mid-response" (logged in lead-notes.md); the lead's own short turns were unaffected, so long turns are the failing part.
Reverse by: nothing to reverse; the first planner left no other output.

D3  2026-10-23 20:06  Who changes the store's create_afk_team signature?
Chose: the `start` implementer. It adds the fourth parameter `extra: &serde_json::Value` (task.md), merges `extra` and then `afk_budget_ends_at` into the `team.created` data through `create_team_with`, and updates the one existing caller, crates/teamwire-store/src/team/tests.rs:48.
Why: `teamwire afk start` is the only production caller, so the change and its first user ship on one branch.
Reverse by: dropping the parameter; only cmd/afk.rs and the store tests pass it.

D4  2026-10-23 20:06  How does `afk start` refuse `no_lead_process` with nothing created, when `ensure_hold` only warns?
Chose: launch exports two functions from hold.rs. One looks up the lead's harness process before anything is created, and returns a refusal when `platform.sleep_hold()` is Some and no lead process is found. After the team exists, `afk start` calls `ensure_hold` (made `pub`) to start or reuse the hold and record it on the team row. Launch's own behaviour stays the same.
Why: the task says "Nothing is created" for `no_lead_process`, but the design's order creates the team (step 4) before the hold (step 6). Checking the process first keeps the design's order for the writes.
Reverse by: one call site in cmd/afk.rs.

D5  2026-10-23 20:06  How does close recognise each of the eight REPORT.md headings?
Chose: a line counts as a heading when it starts with one to six `#` characters and a space, or is a numbered list item (`1.` to `8.` and a space), with optional `**` around the text. The heading matches when its text, compared case-insensitively after trimming `*` and spaces, starts with the heading's words: "What you asked", "What I understood", "The result", "Verified", "Decisions made for you", "Not done", "Risks", "Cost". Order is not checked. The refusal is one entry in `ClosePlan.refusals`, which close prints as `REFUSED: <text>` (crates/teamwire-cli/src/cmd/close.rs:113). The text is `afk_report_incomplete: <team dir>/afk/REPORT.md lacks: <heading>, <heading>` or `afk_report_incomplete: <team dir>/afk/REPORT.md is missing`. The check runs in `team_plan` for an open team only (crates/teamwire-close/src/plan.rs:60), never in `member_plan`.
Why: the skill writes the headings as a numbered bold list, and a reporter may write them as Markdown headings. A strict format would refuse good reports.
Reverse by: the match function in crates/teamwire-close/src/afk.rs.

D6  2026-10-23 20:06  What is the exact AFK hold note past the budget?
Chose: `[teamwire:<team>] budget spent at <budget_ends_at>: AFK run: nobody will answer you. Finish what is running, add the reporter, then run `teamwire close <team>` once <team dir>/afk/REPORT.md exists. If a command you started will wake you, stop again.` Before the budget, the note is the design's 5.3 text word for word. `<team dir>` is the absolute path `$TEAMWIRE_HOME/teams/<team>`; `<budget_ends_at>` is the RFC 3339 timestamp as the store writes it.
Why: the design fixes only the start of the spent note. The rest repeats the design's own instruction (finish, report, close).
Reverse by: the note function in crates/teamwire-report/src/afk_hold.rs.

D7  2026-10-23 20:06  What do `status` and `teams` print for an AFK team?
Chose: `status` prints one line after the team line: `afk: budget ends <budget_ends_at>, <H>h<MM>m left, <n> decisions`, or `afk: budget spent at <budget_ends_at>, <n> decisions`. `teams` prints `<name> (<state>, afk) <title>: ...` for an AFK team, where today it prints `<name> (<state>) <title>: ...`. `status --json` writes `"afk": {"budget_ends_at", "seconds_left", "spent", "decisions"}` or `"afk": null`; `teams --json` writes `"afk_budget_ends_at": "<ts>"` or `null`.
Why: the design names the fields but not the text; one fixed form lets code, docs and tests agree.
Reverse by: the two render functions in cmd/status.rs and cmd/teams.rs.

D8  2026-10-23 20:06  Which table does launch's AFK model warning use, and how does it rank models and effort?
Chose: the table of design section 6 part 4, by member kind: implementer opus/high; investigator opus/high; reviewer opus/xhigh; closer opus/xhigh. Models rank haiku < sonnet < opus; effort ranks low < medium < high < xhigh < max. A member whose model or effort is unset, or a value the ranking does not know, gets no warning. The warning text is `AFK run: member <role> (<kind>) runs on <model> at <effort>, below the AFK table (<table model> at <table effort>)`, printed by the CLI as `warning: ...`, once per member. This matches the wording of the teamwire binary installed on this machine (0.0.0+919f5f8ca8c3, schema 7), whose `status` line and `teams` mark also match D7.
Why: the task's own setup runs implementers on sonnet, so the warning must warn and never refuse. An unknown model may be newer than the table.
Reverse by: the table constant in crates/teamwire-launch/src/afk.rs.

D9  2026-10-23 20:06  How does `afk start` run the doctor checks?
Chose: crates/teamwire-cli/src/cmd/afk.rs builds doctor's inputs and calls `doctor::diagnose`, then keeps only the checks a launch needs: the host, the harness binary, the lead setup (plugin), and `.worktrees/` ignored. The `start` implementer owns doctor.rs and may make the needed functions `pub(crate)` or add one helper that returns the filtered checks. A FAIL refuses with `refused: doctor_failed: <check name>: <detail>`.
Why: doctor.rs is 760 lines; a helper in doctor.rs that it already tests keeps afk.rs small. The skill unit does not need doctor.rs, because doctor's plugin check compares every rendered plugin file by text (crates/teamwire-harness-claude/src/lead_setup/plugin.rs:135), so the new skill file is checked with no doctor change.
Reverse by: removing the helper; no other caller.

D10  2026-10-23 20:20  Who writes which docs?
Chose: the `skill` implementer owns every doc file: docs/adr/0025-afk-runs.md (new; protected folder, see D15), docs/requirements.md (`AFK-` ids), docs/failure-modes.md, docs/api/cli.md, docs/api/core.md, docs/api/mcp.md, docs/ARCHITECTURE.md, docs/conventions.md, README.md and examples/README.md. The other three implementers write no docs. Each tells the skill implementer its final names by `teamwire send --type notice` after its commit, and the skill brief carries the names from D3 to D9 so it can write the docs before those notices.
Why: one owner per doc file keeps every wave-1 `owns` list disjoint. Every name the docs need is already fixed in task.md and D3 to D9.
Reverse by: moving a doc into another implementer's owns.

D11  2026-10-23 20:20  Share one cargo target directory between worktrees?
Chose: no. Each worktree builds in its own target/. The setup only runs `bun install --frozen-lockfile` in ui/.
Why: planner2 measured the first check on a fresh clone at 90 s with the cold build included (planning-check.md), which is cheap. A shared target directory serialises four parallel builds on cargo's lock and lets one member's build artefacts mask another's.
Reverse by: a setup that exports CARGO_TARGET_DIR.

D12  2026-10-23 20:20  What about the test that fails on the base?
Chose: every implementer check passes `-- --skip herdr_env_wins_over_tmux`. The test reads the real terminal environment, and it fails in a pane that runs under tmux, as these members do. The closer runs the full gate and reports that test's result separately.
Why: the failure is in the environment, not the code, and exists at fb1fd8f before any change.
Reverse by: removing the `--skip` from each check.

D13  2026-10-23 20:20  How many implementers, and how is the work split?
Chose: four implementers in one wave: `start` (design 5.2 and 5.4, plus the store parameter), `stophold` (5.3), `endings` (5.5, with the ui schema), `skill` (5.6, section 6, and all docs). The design's `endings` unit is split in two, because it spans five crates and the ui; the split is along a file boundary, so the owns lists stay disjoint.
Why: schema version 5 is on the base, so no unit blocks another. The design itself asks for a split when the widest unit grows past about 300 lines.
Reverse by: merging `stophold` into `endings` before launch.

D14  2026-10-23 20:20  Which models and effort for the implementers?
Chose: model sonnet, effort high, for all four. Each `deadline_s` is 1.5 times the usual value: start 5400, stophold 3600, endings 5400, skill 4500.
Why: task.md sets sonnet for every implementer, and the brief for this run asks for high effort and 1.5 times the deadline.
Reverse by: the spec fields.

D15  2026-10-23 20:20  Who may write the protected docs/adr/ folder?
Chose: only the `skill` implementer, and only to add the new file docs/adr/0025-afk-runs.md. No implementer touches root Cargo.toml, clippy.toml, scripts/, xtask/ or tests/fakes/.
Why: the design (section 7, member 3) asks for ADR 0025. A new ADR file changes no existing ADR.
Reverse by: dropping the ADR from the skill unit.

D16  2026-10-23 20:20  May `endings` change ui/src/shared/teamwire.ts, which ui/AGENTS.md says only the lead changes?
Chose: yes. `endings` owns ui/src/shared/teamwire.ts, its test, and the fixtures ui/fixtures/status.json and ui/fixtures/teams.json. Every new field is optional or nullable, so older output still parses.
Why: AGENTS.md requires the zod schema to change in the same commit as the CLI's JSON output, and the task asks for it. The lead delegates that change to the member that changes the JSON.
Reverse by: the lead making the schema change on the integration branch.

D17  2026-10-23 20:20  How does `status` count decisions, and when is a budget spent?
Chose: `decisions` counts the lines of `afk/decisions.md` that match `^D[0-9]+\s` (a missing file counts 0). `spent` is true when now >= budget_ends_at. `seconds_left` is budget_ends_at minus now in whole seconds, and 0 once spent. The time comes from the Clock trait.
Why: the entry format starts every entry with `D<n>` and two spaces at the start of a line; a mention of D3 inside a sentence is not an entry.
Reverse by: the counting function in crates/teamwire-watch/src/afk.rs.

D18  2026-10-23 20:35  How does launch refuse an implementer past the budget while the rest start?
Chose: per member, as a `LaunchOutcome::Refused`, checked in the launch step before the member is recorded (crates/teamwire-launch/src/member.rs, the path that ends at :155). The reason text is `afk_budget_spent: the AFK budget ended at <budget_ends_at>; only non-implementer members can join`, so the CLI prints `<role>: refused: afk_budget_spent: ...` and exits with the existing PARTIAL code (crates/teamwire-cli/src/cmd/launch.rs:163). `--dry-run` prints the same text as a warning per refused member. Time comes from `ctx.clock`. The model-table warning (D8) is a plan warning, added in validate through `plan_rules::planning_warnings`.
Why: launch already reports a refused member and starts the others; a whole-spec refusal would also block the reporter, which must always run.
Reverse by: the check in launch's afk.rs and its one call site.

D19  2026-10-23 20:35  How does `afk start` find the lead and decide `no_lead_session` and `afk_already_running`?
Chose: it takes the lead harness and host the way launch does (the wiring's lead harness and `pick_host`), and asks the harness for `current_session()`. None refuses `no_lead_session`. A `Background` session refuses `background_lead`, with launch's message. It then walks `team_names(home)`, opens each store read-only, and refuses `afk_already_running` (naming the team) when a team is open, has `afk_budget_ends_at` set and has this session as `lead_session`. A store that cannot be read is skipped with a warning. The budget parses with `parse_duration` at crates/teamwire-cli/src/cmd/metrics.rs:75, made `pub(crate)`; a parse error or a zero budget refuses `bad_budget`.
Why: these mirror launch's own lead lookup (crates/teamwire-launch/src/validate.rs:41 and :211), so an AFK team records the same lead fields as a launched team.
Reverse by: cmd/afk.rs only.

D20  2026-10-23 20:35  Is a refusal from `afk start` printed like launch's?
Chose: yes: `refused: <code>: <message>` on stderr, exit code REFUSED, as crates/teamwire-cli/src/cmd/launch.rs:80. On success it prints, one per line: `team <name> created for an AFK run`, `afk dir: <team dir>/afk`, `report: <team dir>/afk/REPORT.md`, `budget ends: <budget_ends_at>`, and `keep the lid open and the machine on power: a closed lid still sleeps a Mac`, plus any hold warning as `warning: ...`.
Why: the lead and the AFK skill parse launch-style output already.
Reverse by: the print lines in cmd/afk.rs.

D21  2026-10-23 20:50  How do `stophold` and `endings` create AFK teams in tests, when `start` changes `create_afk_team` on another branch?
Chose: each of those two members calls `create_afk_team` in exactly one test helper per test crate, `afk_team(...)`, with today's three arguments. After the lead merges `start`, the lead reprompts each of them to add `&serde_json::Value::Null` as the fourth argument in that one helper, or makes that one-line change itself on the integration branch. Production code in those units never calls `create_afk_team`.
Why: a wave 0 for a three-line store change would cost one more implementer start and delay all three others. One helper keeps the fix to one line per crate.
Reverse by: none needed; the helper is test code.

D22  2026-10-23 21:05  How does the Stop hook join the AFK notes with the watch notes?
Chose: `lead_stop_in` (crates/teamwire-cli/src/cmd/hook.rs:179) computes both lists inside the one `within(LEAD_STOP_BUDGET, ...)` call: first the watch notes, one per team, then the AFK notes from `afk_hold_notes`, one per open AFK team this session leads, all joined with "\n" into one `hold_reply`. When the budget runs out, the hook holds nothing, as today. `afk_hold_notes` returns an empty list as soon as any open team this session leads has a started member with no completion for its current generation and a live watch lease (the design's "something will wake the lead"); otherwise one note per open AFK team. The hook does not check the lead harness: only a Claude Code lead can start an AFK team through the skill, so a Codex lead with no AFK team sees no change.
Why: the design says the hook "returns both notes in one reply, as it already joins notes for several teams", and keeps its time budget.
Reverse by: the join in `lead_stop_in`.

D23  2026-10-23 21:20  How does install render the AFK skill, and how does doctor check it?
Chose: crates/teamwire-harness-claude/src/lead_setup/skill.rs gains `AFK_SKILL_TEXT = include_str!("../../../../examples/skills/afk/SKILL.md")`. The plugin render (crates/teamwire-harness-claude/src/lead_setup/plugin.rs:57) adds `teamwire/skills/afk/SKILL.md`, stamped with `skill::stamp` as the main skill is, to the body list, so the plugin version hash changes with it. Doctor needs no code change: its plugin check compares every rendered file by text (plugin.rs:135); a test proves doctor reports the plugin as out of date when the installed afk SKILL.md differs. The skill's frontmatter `name: afk` makes Claude Code list it as `/teamwire:afk`.
Why: this is how the main skill and reference.md reach the plugin today; one more entry in the same list is the smallest change.
Reverse by: removing the entry and the constant.

D24  2026-10-23 17:00  Run a plan reviewer before wave 1?
Chose: no; launch the four implementers at once and keep one reviewer per branch plus the closer.
Why: 3h13m of budget was left and the skill wants the last implementer launched by two thirds of the budget (about 33 minutes from then); the machine keeps sleeping, so an xhigh plan review would push the launch past that point.
Reverse by: none; per-branch reviewers and the closer still read every diff against the design.

D25  2026-10-23 19:10  Where to keep the /teamwire:afk pointer line out of the Codex lead's skill (rev_skill finding 1, question m010)?
Chose: the Codex harness's plugin render drops that line, with a test in crates/teamwire-harness-codex/tests/; the skill member owns those two files for this fix.
Why: the Codex render is the one place that turns the shared skill text into Codex's copy, so the Codex-only rule lives there; install.rs stays harness-neutral and nobody else is editing the Codex crate.
Reverse by: deleting the filter and its test; the line then shows again in Codex's skill.

D26  2026-10-24 00:30  Where does the D21 fix (six test helpers passing &Value::Null as create_afk_team's fourth argument) go?
Chose: one lead commit on afk/afkmode, using the closer's patch.
Why: the endings and stophold branches are cut from main and lack start's four-argument signature, so the fix cannot compile on them; D21 already placed it after start merges.
Reverse by: reverting that one commit (the joined tests then stop compiling again).
