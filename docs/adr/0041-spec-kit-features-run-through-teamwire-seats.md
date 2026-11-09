# ADR 0041: Spec-kit features run through teamwire's seats

Status: accepted, 2026-11-09. Extends ADR 0038 (ticket refs) with a
provider for spec-kit features. ADR 0029 still holds: pi members keep
`--no-approve` and load no folder prompt file.

## Context

GitHub's spec-kit (https://github.com/github/spec-kit) is a set of prompt
files, Markdown templates and six small shell scripts. `specify init`
installs them into a repository, and a coding agent then runs one command at
a time: `constitution`, `specify`, `clarify`, `plan`, `checklist`, `tasks`,
`analyze`, `implement` and `converge`. No spec-kit code reads `tasks.md`,
and spec-kit runs no task in parallel. It only marks a task `[P]` when the
task could run beside others. The facts here come from a survey of spec-kit
commit c438494 (v1.3.2 and 9 commits, 2026-11-06) and teamwire develop
084e046.

teamwire supplies what spec-kit leaves out: one worktree per writer, file
ownership, waves, a fresh reader before the build, independent checks and
the merge. The user wants teams to run a feature the spec-kit way, with
spec-kit's own commands and with spec-kit unchanged.

Nine things clash today. Spec-kit's commands wait for answers in chat, and
the seat rules forbid that wait. `implement` ticks `tasks.md` and never
commits. Spec-kit finds the active feature through `.specify/feature.json`,
which it git-ignores, so a new worktree has none. A worktree holds only
committed files. pi members skip the folder's prompt files. An extension
with `events:` writes `.codex/config.toml`, which makes Codex launch refuse
the repository, and hooks in `.claude/settings.json` that run unreviewed.
The `git` extension switches branches inside a worktree. The lead skill
said to keep specs outside the repository. Rule LCH-17 lets the seat rules
name only teamwire's own commands.

## Decision

The user made decisions 1 to 7 on 2026-11-09. Each spec-kit command runs in
the existing teamwire seat whose rules fit it. All spec-kit guidance lives
in the lead skill, its reference file, a new preset and the briefs. The seat
rules in `crates/teamwire-launch/contracts/` do not change.

1. **A spec member writes the spec.** An implementer in its own worktree
   owns `specs/NNN-name/`. It stops once per batch of questions. It runs
   `specify` and stops with its questions, at most 3. The lead asks the user
   and reprompts it with the answers. It writes them into `spec.md`, runs
   `clarify`'s scan, and stops again with clarify's questions, at most 5,
   each with clarify's recommended option. The lead asks the user and
   reprompts again. It records those answers through `clarify`, then runs
   `plan` and `tasks`, and commits after each command. A further stop is
   allowed only when an answer raises a new question. The member never
   answers a spec-kit question itself, unless the lead's reprompt says to
   take the recommended option, and then lists each such answer under
   Decisions I made. Its check fails at a question stop, so it records
   `blocked` with a summary that starts with `N questions for the lead.`,
   which the lead reads as a question round. It records `done` only once
   `tasks.md` exists and its check passes. The user reviews `spec.md` and
   `plan.md`, and ticks any checklist from `checklist` at that review,
   because spec-kit lets only a reviewer tick one. The spec member then
   commits those ticks before any implementer starts. A plan reviewer in a
   pane, with its `cwd` at the spec member's worktree, runs `analyze` and
   reads the briefs. The spec member fixes what holds, once. A finished
   `tasks.md` that the user brings is also a valid start, and the plan
   reviewer still runs `analyze` on it. One spec member runs at a time, and the lead names
   its folder, because spec-kit picks the next number by scanning `specs/`
   and two scans at once pick the same number (spec-kit issue #4270).
2. **One implementer by default.** It runs `implement` phase by phase over
   the whole `tasks.md`, with its branch cut from the spec member's branch.
   A split into one implementer per user story is allowed only when the
   stories after the Foundational phase touch disjoint files. Then Setup
   and Foundational run in wave 1, one member per story in wave 2, and
   Polish last. The integrator merges Foundational first, then the stories
   in priority order.
3. **Each implementer ticks `[X]` only on its own phases' lines.**
   `tasks.md` is in its `owns`. The single implementer's `check` runs the
   tests and greps `tasks.md` for unticked tasks. In a per-story split, each
   story member's `check` is `test -s specs/NNN-name/tasks.md && <its own
   tests>`, and only the integrator's `check` on the joined branch greps the
   whole file. A per-story grep cannot pass in wave 1, and it misses the
   tasks `converge` appends, which carry no story label. Each implementer
   commits at each checkpoint with the task ids in the message, because `implement` has no commit step. It reads
   `.specify/memory/constitution.md`, because the seat rules point only at
   `CLAUDE.md` and `AGENTS.md`. It treats `implement`'s stop on an unticked
   checklist as a question to the lead.
4. **The closer runs `converge`** in its own clone as independent evidence.
   The closer preset stays as it is. The lead adds one step to the closer's
   task: follow the clone's `converge` command file and report its findings
   table. The section converge appends to `tasks.md` stays in the clone, so
   an owner's `tasks.md` has nothing open yet. In the fix round the one
   implementer runs `converge` on its own branch, then `implement` on the
   new phase, then `converge` again. Its own `converge` appends the open
   work to its `tasks.md`. It stops at "Converged" or after two rounds. In
   a per-story split, story owners run no `converge`, because converge
   compares the code with the whole spec and on a story branch reports
   every other story as `missing`. So in a split the fix round runs on a
   branch cut from the integrator's branch, by one implementer, such as the
   Polish member. It runs the same loop on the joined code and is the only
   writer of the Convergence phase. The integrator then merges its branch.
5. **pi members read the prompt file.** teamwire starts pi with
   `--no-approve` (ADR 0029), which skips `.pi/prompts/`. A pi brief says:
   read `.pi/prompts/speckit.<cmd>.md` and follow it, with `$ARGUMENTS` set
   to the scope. Claude Code members run `/speckit-<cmd>`, and Codex
   members run `$speckit-<cmd>`.
6. **No `git` extension and no extension with `events:`** in a repository
   teamwire runs. teamwire owns the branches. Before launch, the base branch
   holds `.specify/`, the command folder of each member harness and a
   written constitution, all committed.
7. **Build order.** This ADR covers step A: the lead guidance and the
   `speckit` preset. One pilot feature then runs through the flow with
   hand-written briefs. Step B (doctor checks, a verify rule for the
   constitution) and step C (a `tasks.md` reader) are planned from what the
   pilot shows.

Also settled by the design:

- A worktree member's `setup` writes `.specify/feature.json` with
  `{"feature_directory": "specs/NNN-name"}`. Spec-kit already git-ignores
  that file, so launch does not hide it in `info/exclude`. A team spec has
  no field for the `SPECIFY_FEATURE_DIRECTORY` variable.
- The team carries the feature as a ref, `{"provider": "speckit",
  "id": "NNN-name"}`, which ADR 0038's validation already accepts. So
  `teamwire decisions --ref speckit:NNN-name` finds every team that worked
  on the feature.
- The lead keeps its own plan, team specs and briefs outside the
  repository. Spec-kit's artifacts in `specs/` are product files that a
  member commits.

## Ruled out

- **Spec-kit's workflow engine.** `specify workflow run` starts
  `claude -p`, `codex exec` or `pi -p` itself. That agent has no teamwire
  identity, no Stop hook and no pane, and teamwire launch refuses a
  background session.
- **A spec-kit preset that changes the templates,** such as owner or wave
  fields in the tasks template. A preset changes what spec-kit writes, and
  the user asked to keep spec-kit as it is. teamwire reads the stock format
  instead.
- **Spec-kit hooks or events to enforce teamwire's rules.** A hook is prose
  that an agent can skip. An `events:` extension writes `.codex/config.toml`
  and so blocks every Codex member.
- **Several spec members at once,** because of the number race above.

## Consequences

- `teamwire preset` lists six team shapes. `speckit` comes after
  `contract-split` and before the `plan-review` step.
- The section "Spec-kit features" in `examples/skills/teamwire/reference.md`
  holds the flow, the harness table, what the base branch must hold and the
  brief rules. Every lead harness reads it, so Claude Code, Codex and pi
  leads get the feature alike.
- Nothing checks spec-kit's preconditions yet. The lead checks the base
  branch by hand until step B.
- The merge of tick lines from parallel implementers, and `converge` across
  parallel branches, are unproven. The pilot is their first test. If tick
  lines conflict at the merge, decision 3 is revisited.
- Spec-kit promises no stable interface. A minor release may rename a
  command or change the `tasks.md` format, and the reference names the
  commit its facts come from.
