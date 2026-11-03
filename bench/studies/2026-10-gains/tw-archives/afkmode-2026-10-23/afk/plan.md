# Plan: AFK mode for teamwire

Written by planner2, 2026-10-23. Task: afk/task.md. Design:
docs/design/2026-10-19-afk-mode.md sections 5 and 6. Decisions: afk/decisions.md.

## Summary

- Precondition met: schema version 5 (`team.afk_budget_ends_at`) is committed
  at fb1fd8f, the base of `main` and `bench` (crates/teamwire-store/src/lib.rs:70,
  older.rs:28, team.rs:49 and :150).
- One wave, four implementers, all sonnet/high, all based on `main` (D13, D14).
  No unit blocks another: each owns different files, and every name another
  unit relies on is fixed in task.md or D3 to D20.
  1. `start` (branch afkmode/start): `teamwire afk start` (5.2), the budget
     refusal and the model-table warning in launch (5.4), and the store's
     `create_afk_team` fourth parameter. Crates: store, launch, cli.
  2. `stophold` (afkmode/stophold): the AFK hold in the lead's Stop hook
     (5.3), `afk_hold_notes` in teamwire-report, its call in cli hook.rs.
  3. `endings` (afkmode/endings): close's `afk_report_incomplete` check,
     `StatusView.afk`/`AfkView`, the status line, `teams` mark and
     `afk_budget_ends_at` in `teams --json`, the ui zod schemas (5.5).
     Crates: close, watch, msg, cli, ui.
  4. `skill` (afkmode/skill): examples/skills/afk/SKILL.md (section 6), the
     main skill's pointer line, install renders the second skill into the
     plugin (doctor's plugin check covers it, D9), ADR 0025, `AFK-`
     requirement ids, and every doc change (D10, D15).
- Blocking edges: none at build time. Two at merge time are only text
  conflicts nobody shares: `start` and `stophold` and `endings` all add a
  test file in crates/teamwire-cli/tests/ and touch different src files.
  crates/teamwire-cli/src/main.rs and args.rs belong to `start` only.
- Integration order onto `afk/afkmode` off `main`: start, endings, stophold,
  skill. `start` first because `endings` and `stophold` tests may create an
  AFK team through the store, and `start` changes that signature (D3); they
  use `create_afk_team` with the new 4-argument form only after start is
  merged, so before merge they call it the 3-argument way in one test
  helper per test crate (D21). `skill` last because its docs describe the other three.
- Checks: each member runs its own crates' tests with
  `--skip herdr_env_wins_over_tmux` (D12); `endings` also runs `bun test` and
  `bun run typecheck` in ui/; `skill` also runs scripts/check-docs.sh. The
  closer runs scripts/gate.sh (full) on each branch and on the joined head.
- Risks: zero-member AFK teams in status, watch, close, verify, metrics and
  the dashboard (design section 7 risks); the hold's "will wake" rule; a
  sonnet implementer misreading the design's skill text. Reviewers check
  each branch against a team with zero members.

## Units in detail

### 1. start (design 5.2, 5.4; D3, D4, D8, D9, D18, D19, D20)

Files: crates/teamwire-store/src/team.rs (create_afk_team gains `extra`),
crates/teamwire-store/src/team/tests.rs (the one caller, :48),
crates/teamwire-store/tests/afk.rs (new); crates/teamwire-launch/src/afk.rs
(new: budget check, model table), hold.rs (lead-process lookup before
create, `ensure_hold` made pub), lib.rs (exports), member.rs (budget refusal
call site), plan_rules.rs or validate.rs (model warning call site),
crates/teamwire-launch/tests/afk.rs (new); crates/teamwire-cli/src/cmd/afk.rs
(new), args.rs and main.rs (the `afk start` subcommand), wiring.rs only if
afk.rs needs a field it does not expose, cmd/doctor.rs (a filtered-checks
helper, D9), cmd/metrics.rs (`parse_duration` made pub(crate)),
crates/teamwire-cli/tests/afk.rs (new).

Order inside the unit: store parameter; launch budget refusal and model
warning; `--add` into a zero-member AFK team (design 5.2 says to check it;
validate.rs:57 accepts an existing open team and does not count members, so
expect no change, but a test must prove it); hold lookup; the CLI command.

### 2. stophold (design 5.3; D6)

Files: crates/teamwire-report/src/afk_hold.rs (new: `afk_hold_notes`, the
note text), lib.rs (export beside `teams_without_watch`),
crates/teamwire-report/tests/afk_hold.rs (new),
crates/teamwire-cli/src/cmd/hook.rs (`lead_stop_in` adds the AFK notes to
the same reply, inside `within(LEAD_STOP_BUDGET, ...)`),
crates/teamwire-cli/tests/lead_hook.rs. lead_stop.rs only if a private
helper there must become `pub(crate)` to reuse the "started member with no
completion for its generation" check.

### 3. endings (design 5.5; D5, D7, D16, D17)

Files: crates/teamwire-close/src/afk.rs (new: the REPORT.md check), lib.rs
and plan.rs (call site: whole-team close only, never `--role`),
crates/teamwire-close/tests/afk.rs (new); crates/teamwire-watch/src/afk.rs
(new: `AfkView`, the decisions count), lib.rs (`StatusView.afk`), status.rs
(fill it, both constructors), crates/teamwire-watch/tests/afk.rs (new);
crates/teamwire-msg/src/directory.rs (`TeamEntry.afk_budget_ends_at`);
crates/teamwire-cli/src/cmd/status.rs (the line, and the fallback
`StatusView` at :300), cmd/teams.rs (the mark), crates/teamwire-cli/tests/
status.rs and teams.rs; ui/src/shared/teamwire.ts, teamwire.test.ts,
ui/fixtures/status.json, ui/fixtures/teams.json.

### 4. skill (design 5.6, 6, 7 member 3; D10, D15)

Files: examples/skills/afk/SKILL.md (new), examples/skills/teamwire/SKILL.md
(one pointer line), examples/README.md; crates/teamwire-harness-claude/src/
lead_setup/skill.rs (`AFK_SKILL_TEXT` include_str), lead_setup/plugin.rs
(render `teamwire/skills/afk/SKILL.md`, update the file-list test at :278),
lead_setup.rs only if an export is needed,
crates/teamwire-harness-claude/tests/lead_setup.rs; docs/adr/0025-afk-runs.md
(new), docs/requirements.md, docs/failure-modes.md, docs/api/cli.md,
docs/api/core.md, docs/api/mcp.md, docs/ARCHITECTURE.md,
docs/conventions.md, README.md.

## Integration

1. Lead makes `afk/afkmode` off `main`.
2. Merge afkmode/start, then afkmode/endings, then afkmode/stophold, then
   afkmode/skill. On a conflict, abort and reprompt the later owner to merge
   the earlier branch into its own.
3. After merging start, any test in endings or stophold that called the
   3-argument `create_afk_team` fails to compile, in one helper per test
   crate (D21). The lead reprompts that owner to add
   `&serde_json::Value::Null` as the fourth argument, or makes that one-line
   change on the integration branch.
   The lead runs `cargo test -p teamwire-close -p teamwire-watch -p
   teamwire-report --locked --no-run` after each merge to catch this.
4. The closer runs scripts/gate.sh on the joined head.

## Not done

Nothing is cut. Out of scope by the design and task: a Codex lead in AFK
mode, dashboard views beyond the schema copy, a token budget, the live check
of design section 7, any push or release.

## Launch

Team afkmode already exists, so launch wave 1 with
`teamwire launch --add --spec <team dir>/afk/spec-wave1.json` (the dry run of
that command exits 0; without `--add` it refuses `team_exists`). The lead adds
the reviewers, the closer and the reporter in their own specs.
