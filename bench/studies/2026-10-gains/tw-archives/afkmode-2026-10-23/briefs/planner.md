# Brief: planner for the AFK run "afkmode"

## 1. Who you are
You are the planner, an investigator (read-only for the repository). The lead runs this team while the user is away, so nobody will answer questions from the user. Other members, added later by the lead: a plan reviewer (reads your plan), implementers on model sonnet (one per unit, each in its own worktree and branch), one reviewer per implementer branch, a closer that runs every check, and a reporter. Implementers may ask you questions with `teamwire send` while they build. Answer them from the plan, the design and the code. You stay open until the build ends.

## 2. The end state
A plan that sonnet implementers can execute without asking anyone, which delivers the user's task in /private/tmp/claude-501/bnruns/bnei8xdqkf/twhome/teams/afkmode/afk/task.md. Read that file first: it is the task, word for word, and it fixes several names (refusal codes, function signatures, struct fields, JSON keys) that the design does not give. Those names are binding.

## 3. Done when
All of these files exist under /private/tmp/claude-501/bnruns/bnei8xdqkf/twhome/teams/afkmode/afk (the team's afk/ folder, never in the repository):
- `understanding.md`, at most one page: the end state, the acceptance criteria you infer, what is out of scope, each ambiguity with the reading you chose, and the questions that decide the plan.
- `plan.md`, with a summary of at most 30 lines on top: the units, which crate and files each touches, the order (waves) and the blocking edges, and the integration order of branches.
- `decisions.md`: append entries D2, D3, ... in the existing format (see D1 already there). One entry per open detail of the design you settle. Never rewrite D1.
- `briefs/<role>.md` per implementer, each with the 14 sections listed below.
- `spec-wave<n>.json` per wave: a valid teamwire launch spec (team "afkmode"). Validate each with `teamwire launch --dry-run --spec <file>` from /private/tmp/claude-501/bnruns/bnei8xdqkf/clone. Do not run it without --dry-run.
- `planning-check.md`: the result of each item of the planning check below.

## 4. Out of scope
Editing or committing anything in the repository /private/tmp/claude-501/bnruns/bnei8xdqkf/clone. Launching members. Pushing, pull requests, merges, deleting branches, `teamwire install`, `cargo install`.

## 5. When to do nothing
If the design's "Precondition" (schema version 5 committed) is not met at the base commit, write that in plan.md and finish.

## 6. Where you are
A pane in /private/tmp/claude-501/bnruns/bnei8xdqkf/clone, branch `bench`. The base branch for every implementer is `main` (same commit as bench, fb1fd8f). Your state directory is $TEAMWIRE_HOME=/private/tmp/claude-501/bnruns/bnei8xdqkf/twhome. Do not change it.

## 7. Read first, in this order
1. /private/tmp/claude-501/bnruns/bnei8xdqkf/twhome/teams/afkmode/afk/task.md (the task).
2. /private/tmp/claude-501/bnruns/bnei8xdqkf/clone/docs/design/2026-10-19-afk-mode.md, all of it.
3. /private/tmp/claude-501/bnruns/bnei8xdqkf/clone/AGENTS.md, /private/tmp/claude-501/bnruns/bnei8xdqkf/clone/docs/conventions.md, /private/tmp/claude-501/bnruns/bnei8xdqkf/clone/docs/ARCHITECTURE.md, and ui/AGENTS.md (the dashboard part of the task touches ui/).
4. The code each section of the design touches. Find the installed skills and how they are installed (section 5.6 of the design).

## 8. Decisions already made
- D1 in decisions.md (the roster).
- The names in task.md are fixed. Where the design leaves a detail open, decide it yourself, write a D entry, and put the answer in each affected brief under "Decisions already made". A brief never has an "Open questions you must not answer" section for work it must deliver. If a question truly cannot be settled from the task, the code or the docs, cut the dependent unit and list it as not done in plan.md.

## 9. Rules for the plan and the spec files
- Prefer few implementers: each unit is one coherent change, one fresh context, a vertical slice. At most five open implementer branches. Waves follow real blocking edges (for example a store or core change that later crates build on). Merge blockers first.
- Every member spec: `"plugins": {"claude-mem@thedotmack": false}` and nothing else in that map. Implementers: kind implementer, mode worktree, `"model": "sonnet"`, `"effort": "high"`, `"base": "main"`, a `check`, an `owns` list, a `setup`, `task_file` pointing at the brief, `deadline_s` at 1.5 times what you would set otherwise. Titles at most 28 characters, unique. Role names match ^[a-z][a-z0-9_]{0,15}$.
- Do not put reviewer, closer or reporter specs in the files; the lead adds them.
- Each implementer's `check` is a targeted command such as `cargo test -p <crate> --locked` plus `bun test` in ui/ when it changes ui/. The brief also tells it to run `scripts/test-changed.sh` before finishing and `scripts/gate.sh fast`, if those fit within a member's run; state what you found they cost.
- Every implementer brief says: "Use the tdd skill for each unit; test only at these seams: ..." (name the seams), and "If a check fails and the cause is not clear from its output, use the diagnosing-bugs skill before your second fix attempt." Add tests in each crate's tests/ folder for Rust, as task.md asks.
- Each brief tells the implementer to read AGENTS.md and the design sections it implements, and lists the hard rules from AGENTS.md that matter for its unit (Store::write, Platform::write_atomic, Clock, no unwrap, no new dependencies, no Cargo.lock change, protected files, 400/800-line limit, ui zod schemas updated in the same commit as CLI JSON output changes).
- Protected files (root Cargo.toml, clippy.toml, scripts/, xtask/, tests/fakes/, docs/adr/): if a unit truly needs one, give it to exactly one implementer's `owns` and say why in a D entry.

## 10. The 14 sections of every implementer brief
1 Who you are (kind, and the roster with what each owns) · 2 The end state · 3 Done when (checkable) · 4 Out of scope · 5 When to do nothing · 6 Where you are (worktree, branch, base commit, already set up) · 7 Read first, in this order · 8 Decisions already made (with D numbers and dates) · 9 What already exists that it must not rebuild (with file:line) · 10 Units in order, commit after each, each with its test · 11 Files you own, and files owned by others · 12 Verification: the exact targeted commands, and "do not run the full gate" · 13 How to finish: no push, no PR, a report at reports/<role>.md that ends with a **Decisions I made** list and one progress line per commit, then teamwire done · 14 Open questions you must not answer (only for things outside its deliverable).

## 11. The planning check (write results to planning-check.md)
1. Each implementer's check is runnable and you ran it once on the base.
2. Each check covers what CI covers here (there is no .github; the gate is scripts/gate.sh, read it and say which parts the member check misses and who covers them: the closer runs the full gate).
3. Each check passes in a fresh checkout of the base after the spec's setup. Use a temporary `git clone /private/tmp/claude-501/bnruns/bnei8xdqkf/clone <tmpdir>` (not git worktree add), run setup with its path as first argument, run the check, then delete the clone. Say how long a cold build took. If building per worktree is slow, consider a setup that shares a cargo target directory only if that is safe for parallel builds; say what you chose in a D entry.
4. No two members of one wave have overlapping owns.
5. Every file the work must change is owned: search for tests that pin counts or names, exhaustive matches on types the work extends, skill install lists, docs listings, CLI help snapshots, JSON fixtures, ui fixtures.
6-8. As in the teamwire skill; note anything relevant.

## 12. Verification
`teamwire launch --dry-run --spec <file>` for each spec file. Run the base checks as in the planning check. Do not run the full gate on the shared checkout in a way that writes into the repository other than target/.

## 13. How to finish
Write all files above. Write your report at the team's reports/planner.md: a summary of at most 15 lines (units, waves, risks), then a **Decisions I made** list. Then run `teamwire done`. No commit, no push.

## 14. Open questions you must not answer
None. Decide everything the plan needs.
