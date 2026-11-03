# Understanding: AFK mode for teamwire

Written by the planner, 2026-10-23. Sources: `afk/task.md` (the task, binding
names) and `docs/design/2026-10-19-afk-mode.md` (sections 5 and 6 are the scope).

## End state

One branch off `main` (fb1fd8f) holds everything sections 5 and 6 describe,
tested: `teamwire afk start`, the AFK hold in the lead's Stop hook, the budget
refusal and the model-table warning in launch, close's report check, the AFK
line in `status` and the `afk` mark in `teams`, and the `/teamwire:afk` skill
rendered by install and checked by doctor. Schema version 5 is already
committed at fb1fd8f (precondition met: `git show --stat fb1fd8f`).

## Acceptance criteria (inferred, each checkable)

1. `teamwire afk start --team N --title T --task F [--budget 8h]` creates team N
   with `afk_budget_ends_at = now + budget`, no members, the lead fields launch
   records, `afk/task.md` byte-identical to F, the sleep hold on the team row,
   and `team.created` carrying `lead_cwd` and `afk_budget_ends_at`. It prints
   the team, the `afk/` dir, the report path, the budget end and the lid line.
2. It refuses, creating nothing: `no_lead_session`, `afk_already_running`, a bad
   name or title (launch's wording), a taken name, `doctor_failed` (naming the
   check), `no_lead_process` (when a hold is needed and no lead process).
3. `launch --add` into an AFK team with zero members works; past the budget it
   refuses each new implementer with `refused: afk_budget_spent` and starts the
   rest; it warns once per member below the AFK model table.
4. The lead's Stop hook returns the AFK note (design 5.3 wording; a `budget
   spent at <time>:` variant past the budget) when nothing will wake the lead,
   joined with the watch note when both apply; `afk_hold_notes` is exported by
   `teamwire-report` with the task's signature.
5. `teamwire close <afk team>` refuses with `afk_report_incomplete` while
   `afk/REPORT.md` is missing or lacks one of the eight headings; `--role` and
   ordinary teams are unaffected.
6. `status` prints one AFK line; `status --json` has `afk` (`budget_ends_at`,
   `seconds_left`, `spent`, `decisions`) or `null`; `teams` marks `afk`;
   `teams --json` has `afk_budget_ends_at`; the ui zod schemas accept new and
   old output.
7. `examples/skills/afk/SKILL.md` keeps every rule of design section 6;
   install writes `skills/afk/SKILL.md` into the Claude plugin; doctor checks
   it; the main skill has the one pointer line; ADR 0025 and `AFK-` requirement
   ids exist.
8. Each changed crate's tests pass, `bun test` and typecheck pass in `ui/`, and
   `scripts/gate.sh fast` passes.

## Out of scope

A Codex lead in AFK mode (no Codex skill, no Codex-specific code). Dashboard
views beyond the schema copy. A token budget. Any push, pull request, release,
`teamwire install` or `cargo install`. The live check of design section 7
(needs a person). An `afk finish` or `afk status` command.

## Ambiguities and the reading chosen (each is a D entry in decisions.md)

- Who changes the store's `create_afk_team` signature: the `start` unit (only
  caller). D3.
- How close recognises a heading: a Markdown heading line whose text starts
  with the heading's words, any case, any order. D5.
- Exact wording of the budget-spent note, the status line, the teams mark and
  the launch warning: fixed in D6 to D9 so code, docs and skill agree.
- Which docs each unit writes: one owner per doc file, to avoid overlapping
  `owns`. D10.
- The model table for warnings: by member kind, and a model or effort the
  table cannot rank never warns. D8.

## Questions that decided the plan (answered)

- Is there a blocking edge between units? No: schema v5 is on the base, and
  each unit's code lives in different files. One wave. (plan.md)
- Does a cold build per worktree cost enough to share a target dir? No:
  about 90 s with planner2's measurement. (D11, planning-check.md)
- Does any base check fail before work starts? Yes: one environment-bound test
  in `teamwire-cli` fails in panes under tmux. (D12)
