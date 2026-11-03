# Brief: reviewer of branch afkmode/endings (AFK run "afkmode")

You are a reviewer: you read, you never edit, commit or message anyone. The lead runs this team while the user is away.

- The user's task, word for word: /private/tmp/claude-501/bnruns/bnei8xdqkf/twhome/teams/afkmode/afk/task.md. Its "Names the design does not give" are binding.
- The design: /private/tmp/claude-501/bnruns/bnei8xdqkf/clone/docs/design/2026-10-19-afk-mode.md, especially section 5.5 (close and status for AFK teams), plus the StatusView.afk, teams --json and ui/src/shared/teamwire.ts names in task.md.
- The implementer's brief: /private/tmp/claude-501/bnruns/bnei8xdqkf/twhome/teams/afkmode/afk/briefs/endings.md. Decisions: /private/tmp/claude-501/bnruns/bnei8xdqkf/twhome/teams/afkmode/afk/decisions.md (they are agreed; do not re-argue them unless one breaks the task).
- The diff: `git -C /private/tmp/claude-501/bnruns/bnei8xdqkf/clone diff main...afkmode/endings` and `git -C /private/tmp/claude-501/bnruns/bnei8xdqkf/clone log main..afkmode/endings`. Read connected code where needed.
- The implementer's report: the team's reports/endings.md.
- Repository rules: /private/tmp/claude-501/bnruns/bnei8xdqkf/clone/AGENTS.md (Store::write, Platform::write_atomic, Clock, no unwrap/expect/panic outside tests, no new dependencies, only teamwire-cli prints).

Report ranked correctness, security and data-loss findings, each with file:line and a concrete failing scenario. Also: anything the task, the design section or the brief requires that is missing; any test that would pass without its fix (stubbed, empty or hard-coded code that makes a check pass); any deviation from the binding names. No style or naming findings. Do not propose fixes. Do not run the full gate.

Write the report at the team's reports/rev_endings.md: a summary of at most 10 lines on top, then the findings, then a **Decisions I made** list. Then run teamwire done.
