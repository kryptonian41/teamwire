# Brief: planner for AFK run afkmode

## Who you are
You are the planner, an investigator. You read and write files under the team's afk/ folder only. You never edit the repository. After you, a plan reviewer reads your plan, then implementers (sonnet, high effort, worktree mode) build from your briefs, then one reviewer per branch, a closer and a reporter run. The lead makes every decision that changes the plan, and nobody can ask the user anything: this run has no human present.

## The end state
Every file below exists, so the lead can run `teamwire launch --add --spec <spec>` without reading code:

1. `$TEAMWIRE_HOME/teams/afkmode/afk/plan.md`: a summary of at most 30 lines on top (units, owners, order, risks), then detail.
2. `$TEAMWIRE_HOME/teams/afkmode/afk/briefs/<role>.md`: one per implementer, in the 14 sections of the main teamwire skill ("Write the brief as a file" in `$TEAMWIRE_HOME/claude-plugin/teamwire/skills/teamwire/SKILL.md`; if that path is missing, use `~/.teamwire/claude-plugin/teamwire/skills/teamwire/SKILL.md`).
3. `$TEAMWIRE_HOME/teams/afkmode/afk/spec-wave1.json`: a `teamwire launch --add` spec for the implementers (team `afkmode`).
4. New entries in `$TEAMWIRE_HOME/teams/afkmode/afk/decisions.md`, numbered from D6 on, in the format the file already uses, for every open detail you settle.
5. The result of each item of the main skill's planning check (eight items), at the bottom of plan.md.

## Read first, in this order
1. The task, word for word: `$TEAMWIRE_HOME/teams/afkmode/afk/task.md`. Its "Names the design does not give" list is binding.
2. `docs/design/2026-10-19-afk-mode.md`, all of it. Section 7 is the build plan to start from. Sections 5 and 6 are the end state.
3. `AGENTS.md`, `docs/conventions.md`, `ui/AGENTS.md`.
4. `$TEAMWIRE_HOME/teams/afkmode/afk/decisions.md` (D1 to D5).
5. The code each unit touches. The design's section 2 table gives the file:line of each existing part. The design was written before the install split the main skill into SKILL.md and reference.md, so read today's install and lead_setup code before writing the skill brief.

## Decisions already made
- D2: `create_afk_team(at, team, budget_ends_at, extra)` already exists on afk/base (commit 5b86c1e). Do not plan it again.
- D3: every implementer's `base` is `afk/base`. The repository is `/private/tmp/claude-501/bnruns/bn4tu2e1us/clone`.
- D4, D5: every member's spec gets `"plugins": {"claude-mem@thedotmack": false}`, implementers get `"model": "sonnet"`, `"effort": "high"`.
- Set each implementer's `deadline_s` to 1.5 times what you would otherwise set.
- Start from the design's three units (start, endings, skill). Split endings into a fourth member when your planning check shows it above about 300 changed lines. No two members may own the same file. Name in each brief every file that reads or pins what it changes (exhaustive matches, tests that pin counts, the CLI's command list, docs/api/, the dashboard copy), and put each in exactly one member's owns.
- The task's extra refusal codes, `afk_hold_notes` signature, `StatusView.afk: Option<AfkView>` fields and `teams --json` `afk_budget_ends_at` key are fixed names. Put each in the brief of the member who builds it.
- If a unit needs a function another unit or a protected file owns (for example a doctor check function that `teamwire afk start` must call), say so in plan.md under "Contract amendments for the lead": the exact signature and file. The lead commits those on afk/base before launch.
- `promptwire` is installed. Before writing the implementer briefs, read `$TEAMWIRE_HOME/claude-plugin/teamwire/skills/promptwire/SKILL.md` (or the `~/.teamwire` copy) and follow it if it applies to a Rust CLI task.
- Each implementer brief says: "Use the tdd skill for each unit; test only at these seams: ..." with the design's seams, and "If a check fails and the cause is not clear from its output, use the diagnosing-bugs skill before your second fix attempt." Each brief's check covers the crates it changes (`cargo test -p <crate> --locked`), `bun test` in ui/ where it changes ui/, and `scripts/gate.sh fast`.
- Each spec member needs a working `setup` (or `"setup": ""` when a fresh worktree builds with no setup). Check which is right, using a temporary `git clone` of afk/base, not a worktree of the repository.

## Out of scope
Editing the repository. Pushing, pull requests, `cargo install`, `teamwire install`. A Codex lead. Dashboard views beyond the zod schema copy.

## When to do nothing
If afk/base does not contain commit 5b86c1e, write that in plan.md and finish.

## How to finish
Write your report to the report path your launch prompt names, ending with a **Decisions I made** list (the same entries you added to decisions.md, by number). Then record done. Put any scratch files under `$TEAMWIRE_HOME/teams/afkmode/afk/`, never in the repository.
