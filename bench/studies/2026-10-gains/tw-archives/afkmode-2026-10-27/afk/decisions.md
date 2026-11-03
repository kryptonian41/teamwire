# Decisions for AFK run afkmode

D1  2026-10-27 13:15  Which members does this run add?
Chose: planner (investigator, opus, xhigh); plan reviewer (reviewer, opus, xhigh); the implementers the plan names, three by default as in design section 7 (sonnet, high); one reviewer per implementer branch (opus, xhigh); closer (opus, xhigh); reporter (investigator, opus, high).
Skipped: analyst, because the design document and the task's "Names the design does not give" list already fix the end state and the acceptance criteria. Scouts, because the planner reads the code the briefs need, and the design's section 2 table already gives file:line for every existing part.
Why: the design (docs/design/2026-10-19-afk-mode.md section 7) already splits the work into three units in different crates. The planner turns that into briefs against today's code; the plan reviewer catches a misread before three implementers spend their budget.
Reverse by: add an analyst or scouts with teamwire launch --add.

D2  2026-10-27 13:20  Who makes the create_afk_team signature change the task asks for?
Chose: the lead, on branch afk/base, commit 5b86c1e: create_afk_team(at, team, budget_ends_at, extra: &Value), extra's keys merged into team.created; docs/api/core.md updated; store test checks lead_cwd.
Why: the design's step 0 says the lead commits the shared store change before launch, and every implementer that creates an AFK team needs the same signature.
Reverse by: revert 5b86c1e.

D3  2026-10-27 13:20  Which branches?
Chose: members cut from afk/base (bench/main fb1fd8f + 5b86c1e). Integration branch afk/afkmode off afk/base. The design's "develop" is this repository's main/bench, both at fb1fd8f.
Why: the gate compares against main, and main is where the run started.
Reverse by: rebase afk/afkmode on another base.

D4  2026-10-27 13:20  Which plugins map do members get?
Chose: exactly {"claude-mem@thedotmack": false} on every member, as the user's task says, with no other keys, even for reviewers and the closer.
Why: the user named the map for this run. The main skill's longer map for read-only members is a default the user's explicit instruction replaces.
Reverse by: add ponytail and mattpocock-skills keys in a later --add spec.

D5  2026-10-27 13:20  Which model and effort do implementers use?
Chose: model sonnet (the user's instruction), effort high (the AFK skill's table; the design asks high for start).
Why: the user fixed the model; effort is left open, and the AFK table says high.
Reverse by: change effort in the spec.

D6  2026-10-27 13:30  How many implementers, and which unit does each own?
Chose: five, all in wave 1 off afk/base: start (teamwire afk start, its doctor checks), budget (launch's budget refusal and model-table warnings), stophold (the AFK hold in the lead's Stop hook), closestat (close's report check, the status line and object, the teams marker and key, the zod copy), skill (the AFK skill, its install, and every doc). Owned files are listed in afk/plan.md and never overlap.
Why: the design's endings unit came to about 650 changed lines with tests and its start unit to about 730, both well past the 300-line split rule. Splitting endings along the design's own line (hold versus close and status) and taking the two small launch rules out of start gives units of about 210 to 520 lines in separate files.
Reverse by: merge two briefs and relaunch one member for both.

D7  2026-10-27 13:30  How does the skill member test the AFK skill's commands while the afk subcommand lives on another branch?
Chose: a contract amendment. The lead commits afk/amend-afk-args.patch on afk/base before launch: the clap arguments of `teamwire afk start --team NAME --title TITLE --task FILE [--budget DURATION, default 8h]` in crates/teamwire-cli/src/args.rs and main.rs, and crates/teamwire-cli/src/cmd/afk.rs as a stub that refuses with exit 1. The start member replaces the stub.
Why: the existing check that a skill names only real commands runs `teamwire --help`, so on the skill branch `teamwire afk` must already exist. The patch was checked on a fresh clone: gate fast ok, preset tests 8 passed.
Reverse by: drop the commit; the skill member then checks the AFK skill's commands only after integration.
Amended by D29.

D8  2026-10-27 13:30  Who owns doctor.rs and install.rs, and which doctor checks does afk start run?
Chose: start owns crates/teamwire-cli/src/cmd/doctor.rs and tests/doctor.rs; nobody changes install.rs. start adds `pub(crate) fn launch_checks(inputs: &Inputs<'_>, pick: &HostPick) -> Vec<Check>`: the host (FAIL when the host is none), the herdr session hook (herdr only), `.worktrees ignored`, the harness and host binaries with their probes, and the lead setup checks (teamwire plugin, old lead setup, MCP scope). Disk, database, credential and per-team checks are left out.
Why: install and doctor's plugin check already render and compare every plugin file (lead_setup.rs plugin_problems, PluginFiles::render), so a second skill file is installed and checked with no change to either. tests/doctor.rs compiles doctor.rs by #[path] with a stub Wiring, so launch_checks takes Inputs, which keeps it testable with the fake platform. A host of none passes in doctor but makes launch refuse no_host, so an AFK start fails it early.
Reverse by: move launch_checks into a new file owned by another member.

D9  2026-10-27 13:30  In what order does afk start refuse, with which codes, and how does it print?
Chose: it stops at the first failure, in this order: no_lead_session (no harness session id) or background_lead (launch's code and message); afk_already_running; bad_name, title_missing, title_too_long, team_exists (launch's codes); bad_budget; task_unreadable, task_empty; doctor_failed (names the check: `doctor_failed: <check name>: <detail>`); no_lead_process. Each refusal prints `refused: <code>: <message>` and `nothing was created` on stderr and exits 1. Doctor warn lines print as `warning: <name>: <detail>` and do not refuse. A failure after the team row exists (task copy) prints `failed: ...` naming the team and exits 2.
Why: the design's step order, with the cheap argument checks before doctor, and launch's existing output shape and codes, so the skill and docs name one form.
Reverse by: change the order in crates/teamwire-launch/src/afk.rs and docs/api/cli.md.

D10  2026-10-27 13:30  Which --budget values are accepted?
Chose: the units of `metrics --since` (s, m, h, d, a whole number then one unit), parsed by metrics.rs's parse_duration made pub(crate). Zero, or a value whose end overflows Timestamp::checked_add, is refused with bad_budget. budget_ends_at = Clock::now() + budget.
Why: AGENTS.md says to extend the nearest helper, and parse_duration already exists with tests.
Reverse by: a separate parser in afk.rs.

D11  2026-10-27 13:30  What does afk start record as lead_cwd?
Chose: the harness's session_cwd(session), as launch's create_team does, else the command's current directory. The key is always written.
Why: the task requires the key; session_cwd is launch's source and the current directory is the lead's Bash cwd when the harness cannot say.
Reverse by: write the key only when session_cwd answers.
Amended by D29.

D12  2026-10-27 13:30  What does a successful afk start print?
Chose: on stdout, five lines: `team <team>: AFK run started`, `afk dir: <home>/teams/<team>/afk`, `report: <home>/teams/<team>/afk/REPORT.md`, `budget ends: <RFC 3339 UTC>`, `Keep the lid open and the machine on power: a closed lid still sleeps a Mac.` Hold warnings go to stderr as `warning: ...`. Exit 0.
Why: section 5.2 step 7 lists these facts; fixed text lets the skill and the docs quote it.
Reverse by: edit afk.rs and docs/api/cli.md together.

D13  2026-10-27 13:30  Where does the afk start logic live, and where is it tested?
Chose: everything except argument parsing, doctor and printing lives in crates/teamwire-launch/src/afk.rs and is tested with launch's fakes (FakePlatform with and without a sleep hold, FakeHarness processes). The CLI tests cover the refusals that come before the team exists.
Why: the real binary uses the real platform, which on macOS needs a sleep hold and finds no real lead process in a test, so the success path cannot be reached through the binary. ensure_hold, lead_plan and the title check are crate-private in launch, so the library function can reuse them without widening the API.
Reverse by: none needed; the CLI file stays thin either way.
Amended by D28.

D14  2026-10-27 13:30  How does launch refuse a new implementer past the budget?
Chose: in launch_all (member.rs), before a member is recorded: when the team row's afk_budget_ends_at is set, Clock::now() is at or past it, and the member's kind is implementer, the outcome is Refused("afk_budget_spent: the AFK run's budget ended at <ts>; past it only reviewers, investigators and closers may join"). Nothing is recorded for that member. Validation and --dry-run are unchanged. Every other kind launches.
Why: section 5.4 refuses each implementer and accepts the rest in the same launch, which only a per-member outcome can do; validation errors refuse the whole spec.
Reverse by: move the check into validation.

D15  2026-10-27 13:30  When is a member "below the AFK table"?
Chose: for a member of a team with afk_budget_ends_at set, the table asks opus for every kind, and effort high for implementer and investigator, xhigh for reviewer and closer. Model rank by name, case-insensitive: haiku < sonnet < opus; a name with none of the three is not judged. Effort rank: low < medium < high < xhigh < max; another word is not judged. A missing model or effort counts as below. One warning per member: `role <role> (<kind>): below the AFK table, which asks opus at <effort> or higher; it sets model <model or none>, effort <effort or none>`. The rule lives in plan_rules.rs's planning_warnings.
Why: section 6's table names a model and effort per profile, but launch sees only the kind, so the kind's lowest profile is the rule. A Codex model name cannot be ranked, so it is not judged.
Reverse by: edit plan_rules.rs.

D16  2026-10-27 13:30  What exactly does the AFK hold say, and in what order with the watch note?
Chose: `[teamwire:<team>] AFK run: nobody will answer you. If you were about to ask the user, decide it yourself, add it to <dir>/afk/decisions.md, and carry on. If the work is finished, add the reporter, then run `teamwire close <team>` once <dir>/afk/REPORT.md exists. If a command you started will wake you, stop again.` Past the budget (now at or past afk_budget_ends_at): `[teamwire:<team>] budget spent at <ts>: AFK run: nobody will answer you. Launch no new implementer. Finish what is running, have the reporter write <dir>/afk/REPORT.md, then run `teamwire close <team>`. If a command you started will wake you, stop again.` <dir> is <home>/teams/<team>. In one reply the watch notes come first, then the AFK notes, joined by newlines, both computed inside the one LEAD_STOP_BUDGET call.
Why: the design's note text, with the team tag first because every teamwire line starts with it.
Reverse by: edit teamwire-report's note function and docs/api/cli.md.
Amended by D31.

D17  2026-10-27 13:30  How does close judge afk/REPORT.md?
Chose: for a whole-team close of an open team with afk_budget_ends_at set (dry run included). Required headings, by name: What you asked; What I understood; The result; Verified; Decisions made for you; Not done, and why; Risks; Cost. A heading is a line of one to six `#` and a space; after stripping the `#`s, a leading `N.` or `N)`, and `**`, its text must start with the name, case-insensitive. Order is not checked. Refusal text: `afk_report_incomplete: <path> is missing` or `afk_report_incomplete: <path> lacks the headings: <names in skill order>`, printed by the CLI as `REFUSED: afk_report_incomplete: ...`. --role, --clear-stale-lock and a closing or closed team are unaffected. No flag overrides it.
Why: section 5.5; a team left in `closing` by a partial close already passed the check. Presence only, because a report with the headings out of order is still complete.
Reverse by: edit crates/teamwire-close/src/afk_report.rs.

D18  2026-10-27 13:30  What do status and status --json show for an AFK team?
Chose: StatusView.afk: Option<AfkView>, serialized as `"afk": null` for an ordinary team. AfkView { budget_ends_at, seconds_left (budget_ends_at.saturating_since(now)), spent (now >= budget_ends_at), decisions }. decisions counts the lines of <team dir>/afk/decisions.md matching ^D[0-9]+ followed by white space; a missing file is 0; another read error is 0 plus an entry in errors. `status <team>` prints after the watch line: `afk: budget ends <ts> (<time> left), <n> decisions`, or `afk: budget spent at <ts>, <n> decisions`, with <time> from status.rs's minutes(). The all-teams summary line is unchanged.
Why: the task fixes the field names; null rather than a missing key matches the teams key, which the task says is null.
Reverse by: skip_serializing_if on the field.
Amended by D35.

D19  2026-10-27 13:30  How do teams and teams --json mark an AFK team?
Chose: teamwire_msg::TeamEntry gains `afk_budget_ends_at: Option<Timestamp>` (string or null), filled in describe(); team_teams in the MCP server gets the same key. The text line reads `<name> (<state>, afk) <title>: address as ...` for a team with the column set.
Why: describe() builds the entry for both teams --json and team_teams, so one field covers both.
Reverse by: build the key in teams.rs only.

D20  2026-10-27 13:30  How does the dashboard schema take the new keys?
Chose: ui/src/shared/teamwire.ts: AfkView = z.object({budget_ends_at: z.string(), seconds_left: count, spent: z.boolean(), decisions: count}); StatusView gets `afk: AfkView.nullable().optional()`; TeamEntry gets `afk_budget_ends_at: z.string().nullable().optional()`. Tests go in ui/src/shared/teamwire.test.ts with inline objects; fixtures/ stays unchanged. This brief is the lead's request that ui/AGENTS.md asks for before src/shared changes.
Why: optional and nullable accept both the new output and older output without the keys, as the task asks.
Reverse by: add AFK fixtures later, with the web tests that count teams checked.

D21  2026-10-27 13:30  How is the AFK skill packaged?
Chose: examples/skills/afk/SKILL.md, compiled in as AFK_SKILL_TEXT beside SKILL_TEXT in lead_setup/skill.rs, rendered stamped with skill::stamp to teamwire/skills/afk/SKILL.md in the Claude plugin only. Its frontmatter has name `afk` and no metadata key. Its report section lists the eight headings of D17 as `##` headings with those exact names. The main skill gets, near the top: "For a run the user will not be present for, use `/teamwire:afk`. Only a Claude Code lead can run it."
Why: section 5.6; the plugin version hashes every rendered file, so a changed AFK skill is a new version; a Codex lead is out of scope.
Reverse by: remove the render entry.
Amended by D36.

D22  2026-10-27 13:30  Who writes the docs, and how do docs and tests agree on names?
Chose: the skill member owns every doc this work changes: docs/api/cli.md, docs/api/core.md, docs/api/mcp.md, docs/requirements.md (new section "AFK runs (AFK)", AFK-1 to AFK-12), docs/failure-modes.md, docs/ARCHITECTURE.md, README.md, docs/adr/0025-afk-runs.md. Each code brief fixes the exact test function names that the failure-modes rows cite (afk/plan.md, "Tests and requirement ids").
Why: no test pins those docs, so one owner avoids five members editing cli.md; the failure-modes file says its test names are the names the tickets must use.
Reverse by: hand a doc to a code member and remove it from the skill brief.

D23  2026-10-27 13:30  What do the checks run, given a test that fails on the base?
Chose: every check that runs teamwire-cli tests passes `--no-fail-fast -- --skip herdr_env_wins_over_tmux`. Members run only their check; not scripts/test-changed.sh and not the full gate, which the closer runs on the joined branch.
Why: crates/teamwire-cli/tests/tmux_wiring.rs::herdr_env_wins_over_tmux fails on afk/base when a tmux process is among the test's parents (its own comment says so), and this team runs in tmux panes: exit 2 instead of 0 on a fresh clone. Without --no-fail-fast cargo stops at that binary and skips every later test target. With the skip, teamwire-cli and teamwire-launch pass (403 tests, exit 0).
Reverse by: run the full gate outside tmux.
Amended by D27 and D34.

D24  2026-10-27 13:30  Setup, branches and deadlines?
Chose: setup `cd "$1" && cargo build --workspace --all-targets --locked` for every member, plus `&& cd ui && bun install --frozen-lockfile` for closestat. Branches afkmode/<role>, base afk/base. deadline_s, 1.5 times the normal value: start 8100, skill 8100, stophold 5400, closestat 5400, budget 4050.
Why: a cold build of afk/base in a fresh clone took 21 s, inside launch's 600 s setup limit; bun install took under a second.
Reverse by: edit afk/spec-wave1.json.

D25  2026-10-27 13:30  Do members get promptwire skills?
Chose: none.
Why: searches for rust, clap, zod, sqlite, claude code plugin and claude code hooks found only anthropic-skill-creator and openai-security-threat-model; skill-creator waits for a user to judge evals, and the AFK skill's content is already agreed.
Reverse by: install one into teams/afkmode/skills/ and name it in a brief.

D26  2026-10-27 13:30  Where did the planner's fresh clone go?
Chose: the planner session's scratchpad, not afk/.
Why: close copies every file of the team directory into the archive, and the clone's target/ directory is several gigabytes.
Reverse by: none needed.

D27  2026-10-27 14:00  How does the closer run the full gate, given the test that fails inside tmux?
Chose: the closer runs `scripts/gate.sh` detached from tmux with a double fork, writes its output to /private/tmp/claude-501/bnruns/bn4tu2e1us/twhome/teams/afkmode/afk/closer-gate.log, and waits until the log holds a `gate exit <n>` line. No step and no test is skipped. The members' checks keep D23's skip, which is right for a targeted check.
Why: inside tmux, gate step 4 stops at `herdr_env_wins_over_tmux`, cargo skips every later test target (e2e, crash, mcp and others), and steps 5 and 6 never run (planrev.md finding 1). The double fork makes launchd (pid 1) the gate's parent, so no tmux is among the test's parents and `nearest_host` (crates/teamwire-cli/src/cmd/doctor.rs:137-157) finds none. The planner ran it from a tmux pane on afk/base plus the patch: `cargo test -p teamwire-cli --test tmux_wiring` passed 4 of 4, and `scripts/gate.sh` printed all six steps ok and `gate exit 0`, both on the first patch (test 227 s) and on the final patch with D29's helpers (test 236 s).
Reverse by: run the gate inside tmux and read the stop at that test as known.

D28  2026-10-27 14:00  How are afk start's success output and its exit-2 path tested through the binary?
Chose: two more tests in crates/teamwire-cli/tests/afk.rs. `afk_start_prints_the_run_and_exits_zero`: the five D12 lines on stdout, exit 0, afk/task.md byte-equal to the task file, the team row has afk_budget_ends_at. `afk_start_that_cannot_copy_the_task_exits_two_and_names_the_team`: the same setup, with `<home>/teams/<team>/afk` made a regular file first; exit 2, stderr names the team, the row exists. The setup is doctor's healthy bed (crates/teamwire-cli/tests/doctor.rs `Bed::healthy`, `FakeBins` on PATH, `ignoring_repo()` as the cwd, HERDR_ENV=1, TMUX removed), plus a live child the test spawns (`sleep 60`, killed at the end) and a session file `<claude dir>/sessions/<pid>.json` holding `{"pid": <pid>, "sessionId": "<sid>", "kind": "interactive", "cwd": "<dir>"}` with no startedAt, and the env vars TEAMWIRE_CLAUDE_DIR and CLAUDE_CODE_SESSION_ID=<sid>. start may move the healthy setup into crates/teamwire-cli/tests/support/healthy.rs, which it owns.
Why: D13 said the success path cannot be reached through the binary; it can. `runs_session` (crates/teamwire-harness-claude/src/session.rs:122-130) counts a live pid as the session's when startedAt is missing, and `team_names` (crates/teamwire-store/src/store.rs:115-133) skips a directory without team.db, so a regular file named afk blocks only the task copy. Without these tests, a start that prints other text passes, while the skill and cli.md quote D12's lines (planrev.md finding 2). On macOS the real sleep hold starts and ends with the child; Linux has none.
Reverse by: drop the two tests and keep the library tests of D13.

D29  2026-10-27 14:00  How does start reuse launch's NewTeam fields and lead_cwd lookup without editing budget's member.rs?
Chose: the lead's contract amendment (afk/amend-afk-args.patch) also extracts two helpers from `create_team` in crates/teamwire-launch/src/member.rs: `pub(crate) fn new_team(name: &TeamName, title: &str, lead: &LeadPlan) -> NewTeam` (the title through plan::clean) and `pub(crate) fn lead_cwd(registry: &Registry, lead: &LeadPlan) -> Option<String>`. create_team calls both, so neither is unused. start calls `crate::member::new_team` and `crate::member::lead_cwd` and never edits member.rs; D11's fallback to the current directory lives in start's afk.rs. budget leaves both helpers alone.
Why: the lookup was inline in the private create_team (member.rs:92-120 on afk/base), a file budget owns. Copying it breaks AGENTS.md's rule to extend the nearest helper, and editing it gives one file two owners (planrev.md finding 3). Checked in the planner's clone of afk/base plus the patch: `scripts/gate.sh fast` fmt, tidy and clippy ok; `cargo test -p teamwire-launch -p teamwire-cli --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux` 403 passed, exit 0; `git apply --check` on the repository's afk/base clean.
Reverse by: drop the helpers from the patch; start then asks the lead.

D30  2026-10-27 14:00  In which commits does closestat change `--json` output and the zod copy?
Chose: unit 2 is the status view, the status line, the zod AfkView and StatusView.afk, and their zod tests, in one commit. Unit 3 is the teams key and marker, the zod TeamEntry key and its zod test, in one commit. There is no separate zod unit.
Why: AGENTS.md and design line 179 put a `--json` change and ui/src/shared/teamwire.ts in the same commit; a separate zod unit leaves a commit whose JSON carries a key the schema does not know (planrev.md finding 4).
Reverse by: none needed.

D31  2026-10-27 14:00  What does the AFK hold say past the budget?
Chose: `[teamwire:<team>] budget spent at <ts>: AFK run: nobody will answer you. If you were about to ask the user, decide it yourself, add it to <dir>/afk/decisions.md, and carry on. Launch no new implementer: finish what is running, then add the reporter, and run `teamwire close <team>` once <dir>/afk/REPORT.md exists. If a command you started will wake you, stop again.` The note before the budget is unchanged from D16. The test is `past_the_budget_the_afk_note_says_budget_spent_after_the_team_tag`.
Why: design line 150 makes the budget-spent note the base note with a prefix and one more instruction. D16 dropped "decide it yourself", which a lead past the budget still needs while it waits on the reporter and the closer (planrev.md finding 5). The team tag stays first because every teamwire line starts with it, so the old test name described text the note does not have.
Reverse by: edit teamwire-report's note function and docs/api/cli.md.

D32  2026-10-27 14:00  Which tests cover the AFK skill's install and doctor's check of it?
Chose: two tests in crates/teamwire-harness-claude/tests/lead_setup.rs. `install_writes_the_afk_skill_as_a_second_stamped_skill`: the installed teamwire/skills/afk/SKILL.md is stamped, and the rendered-file count at line 184 becomes 7. `a_plugin_without_the_afk_skill_or_with_an_edited_one_is_a_problem`: the public `ClaudeConfig::plugin_problems(&want, true)` reports a problem when the installed plugin lacks skills/afk/SKILL.md and when its copy is edited (the pattern of `plugin_problems_name_what_is_missing_stale_or_off`). AFK-12 cites these two; a unit test in plugin.rs may stay as an extra.
Why: task.md asks for tests in each crate's tests/ folder, and no test covered design 5.6's "doctor checks it the same way it checks the main skill" (planrev.md finding 6).
Reverse by: none needed.

D33  2026-10-27 14:00  What does "fails without the change" mean for a test that pins today's behaviour?
Chose: every brief's "Done when" says: each test of new behaviour fails without its change; a test marked (guard) pins today's behaviour and passes on afk/base too. The guards: a_team_without_a_budget_is_unaffected and an_ordinary_team_gets_no_model_warning (budget); removing_one_member_of_an_afk_team_needs_no_report and an_ordinary_team_closes_without_a_report (closestat); add_to_an_afk_team_with_no_members_starts_the_member (start; it needs the AFK start function to build its team, and no change to validate.rs to pass). The closer does not revert-check a guard.
Why: these five pass on afk/base by design (planrev.md finding 7); the old wording would push an implementer to bend them or to report the criterion unmet.
Reverse by: none needed.

D34  2026-10-27 14:00  Do the budget and stophold checks run the crates that depend on theirs?
Chose: yes. budget: `cargo test -p teamwire-launch -p teamwire-cli --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux && scripts/gate.sh fast`. stophold: `cargo test -p teamwire-report -p teamwire-mcp -p teamwire-cli --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux && scripts/gate.sh fast`. Every member's check now runs its changed crates and each crate that depends on them, from the Cargo.toml files: launch <- cli; report <- cli, mcp; close <- cli; watch and msg <- cli, mcp; harness-claude <- cli.
Why: budget's launch_all change runs on every `launch --add`, and teamwire-mcp depends on teamwire-report; neither dependent ran in the old checks (planrev.md finding 8). Run once by the planner on afk/base plus the patch: budget's command 403 passed, stophold's 405 passed, each exit 0, then gate fast ok.
Reverse by: edit the two check strings in afk/spec-wave1.json.

D35  2026-10-27 14:00  Which lines of afk/decisions.md count as decisions?
Chose: the lines matching `^\s*D[0-9]+\s`, so leading white space is allowed. closestat's test includes an indented entry. The AFK skill shows the decision-entry example at column 0 inside a fenced block, so a lead copies `D3  ...` with no indent.
Why: design section 6 shows the entry indented by four spaces; a lead that copies it gets `0 decisions` from D18's rule (planrev.md finding 9).
Reverse by: edit crates/teamwire-watch/src/afk.rs.

D36  2026-10-27 14:00  What is the main skill's pointer line?
Chose: the design's sentence only: "For a run the user will not be present for, use `/teamwire:afk`."
Why: design line 187 gives that line; D21's second sentence differed from it (planrev.md, "Also noted").
Reverse by: edit examples/skills/teamwire/SKILL.md.

D37  2026-10-27 14:40  Roster change after planning
Chose: five implementers (start, budget, stophold, closestat, skill) instead of the design's three, as the planner sized them (D6). The lead committed the planner's contract patch on afk/base as fb842b4. The planner stays open until the build ends, so implementers can ask it about the plan. One reviewer per branch follows, then the closer and the reporter.
Why: endings and start each came to over 500 changed lines, past the design's 300-line split rule.
Reverse by: not needed; the units have disjoint owns.

D38  2026-10-27 15:30  Does afk start refuse a Codex lead that launch would refuse with lead_not_locatable?
Chose: no change. A Codex lead in AFK mode is out of scope (design section 7), and the AFK skill says only a Claude Code lead can run it.
Why: revstart note 1 needs a Codex lead outside its own pane on Linux; no supported AFK run reaches it.
Reverse by: run launch's lead_must_be_locatable rule inside afk_check.

D39  2026-10-27 15:30  How does a user get out of an AFK team that never had a member (afk start's exit-2 path, or a run that ends before the first launch)?
Chose: close accepts an AFK team that never had a member without afk/REPORT.md. The AFK skill's report section says so, and tells the lead to say why in its last reply.
Why: revstart note 2: after exit 2 the team stays open with no task copy, a second afk start is refused with afk_already_running, and close refused it until someone wrote eight headings by hand. A team with no members has nothing to report.
Reverse by: drop the member check in crates/teamwire-close/src/afk_report.rs.

D40  2026-10-27 16:30  How was the closer's report acted on?
Chose: findings 1 to 4 fixed by skill (5778a70). Finding 6 (stophold) left as is: the note text is fixed by D16/D31. Finding 5 (plan wording) needs no code change. Every flag that verify raised on the five branches was an #[expect(..., reason = ...)] lint suppression, which AGENTS.md allows, and the lead waived each one.
Why: one fix round per owner after the closer, as the skill allows.
Reverse by: not needed.
