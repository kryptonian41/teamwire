# Brief: planner for team fixes3

## Who you are
You are the planner, an investigator. You read code and write plan files. You never edit or commit anything in the repository. Later members: one plan reviewer reads your plan; three implementers (roles `report`, `msg`, `ui`) each build one fix from the brief you write for it; a reviewer per branch; a closer; a reporter.

## The task
The user's task, word for word, is in /private/tmp/claude-501/bnruns/bne68n09a4/twhome/teams/fixes3/afk/task.md. Read it first. It holds three independent fixes: (1) crate teamwire-report, the Stop hook's final-message fallback must refuse a non-regular or over-64 MiB transcript; (2) crate teamwire-msg, `resend` to a closing or closed team must read that team's stored delivery row; (3) ui/, a new `problemReasons` in ui/src/web/model/teams.ts, used by `hasProblem` and shown on the overview card.

## The end state
These files exist in /private/tmp/claude-501/bnruns/bne68n09a4/twhome/teams/fixes3/afk/ (write nothing inside the repository):
1. `understanding.md`, at most one page: the end state of each fix, the acceptance criteria you infer, what is out of scope, and each ambiguity with the reading you chose.
2. `plan.md` with a summary of at most 30 lines on top: per fix, the files to change, the existing helper to reuse (for fix 1, the usage reader's regular-file and MAX_TRANSCRIPT_BYTES rule; extend it rather than copy it), the test to add and where, and every other file that reads or pins what changes (tests, fixtures, exhaustive matches, the ui zod schema in ui/src/shared/teamwire.ts if the CLI JSON changes, the overview card component for fix 3).
3. `decisions.md`: append entries D3, D4, ... in the format already used in that file, for every open question you settle. Do not rewrite D1 or D2.
4. `briefs/report.md`, `briefs/msg.md`, `briefs/ui.md`: one brief per implementer with these 14 sections: Who you are (with the roster above), The end state, Done when, Out of scope, When to do nothing, Where you are, Read first in this order, Decisions already made, What already exists, Units in order (commit after each, each with its test), Files you own and files owned by others, Verification (exact targeted commands; say "do not run the full suite or the repository-wide gates" except as below), How to finish (no push, no pull request, a report ending with **Decisions I made**, then `teamwire done`), Open questions you must not answer (should be empty for work the member must deliver). Put these two sentences in each brief: "Use the tdd skill for each unit; test only at these seams: <name the seams>." and "If a check fails and the cause is not clear from its output, use the diagnosing-bugs skill before your second fix attempt." Tell each implementer to write any scratch files under /private/tmp/claude-501/bnruns/bne68n09a4/twhome/teams/fixes3/afk/, never in the repository. The task requires each implementer to also run `TEAMWIRE_BASE=e78c32be scripts/gate.sh fast` from its worktree root before it finishes; include that.
5. `spec-wave1.json`: a teamwire launch spec for team `fixes3` with the three implementers. Each: kind implementer, mode worktree, repo /private/tmp/claude-501/bnruns/bne68n09a4/clone, branch `fixes3/<role>`, base `bench`, model opus, effort high, deadline_s 5400, a `check`, an `owns` list, a `setup` (a worktree has no dependencies; the ui member needs its bun install; say `"setup": ""` if a member needs nothing), `task_file` pointing at its brief, a `title` of at most 28 characters that differs from its siblings, and `"plugins": {"claude-mem@thedotmack": false}`. Repeat the plugins map at the team level too. No two members may have overlapping `owns`. Start from `teamwire preset parallel-tickets`. Validate it with `teamwire launch --dry-run --spec <file>`.

## Done when
- Each file above exists, and `teamwire launch --dry-run --spec .../spec-wave1.json` exits 0.
- Your planning check results are in plan.md: for each implementer, its `check` and `setup` run green on a fresh copy of the base. Make the fresh copy with `git clone` of /private/tmp/claude-501/bnruns/bne68n09a4/clone into a temporary directory under /private/tmp/claude-501/bnruns/bne68n09a4/twhome/teams/fixes3/afk/tmp/, check out `bench`, run `setup` with that path as its first argument, run the check, record the exit code, then delete the temporary clone. Each check must cover what the task names (`cargo test -p <crate> --locked`, `bun test` in ui/) and what the CI or gate script runs for that area (read scripts/gate.sh and ui/AGENTS.md; for ui, include its typecheck and lint if gate or ui/AGENTS.md runs them).
- For each implementer, plan.md lists the files outside the obvious one that read or pin what it changes, and each is in exactly one member's owns.

## Out of scope
Editing, committing or formatting anything in the repository. Fixing anything beyond the three fixes. Launching members.

## Read first
/private/tmp/claude-501/bnruns/bne68n09a4/clone/AGENTS.md, ui/AGENTS.md, docs/conventions.md, then the code of each area.

## How to finish
Write your own report at the report path launch gave you, with a five-line summary at the top and a **Decisions I made** list at the end. Then run `teamwire done`. Keep your final chat message to five lines.

## Vendor skills
Before you write the implementer briefs, follow $TEAMWIRE_HOME/claude-plugin/teamwire/skills/promptwire/SKILL.md (the `promptwire` tool is installed). Put any skill it picks into the affected brief.
