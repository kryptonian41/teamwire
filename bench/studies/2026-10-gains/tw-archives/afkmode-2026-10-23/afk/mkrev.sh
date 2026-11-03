mkrev () {
	role=$1 
	sec="$2" 
	bat > $A/briefs/rev_$role.md <<EOF
# Brief: reviewer of branch afkmode/$role (AFK run "afkmode")

You are a reviewer: you read, you never edit, commit or message anyone. The lead runs this team while the user is away.

- The user's task, word for word: $A/task.md. Its "Names the design does not give" are binding.
- The design: $R/docs/design/2026-10-19-afk-mode.md, especially $sec.
- The implementer's brief: $A/briefs/$role.md. Decisions: $A/decisions.md (they are agreed; do not re-argue them unless one breaks the task).
- The diff: \`git -C $R diff main...afkmode/$role\` and \`git -C $R log main..afkmode/$role\`. Read connected code where needed.
- The implementer's report: the team's reports/$role.md.
- Repository rules: $R/AGENTS.md (Store::write, Platform::write_atomic, Clock, no unwrap/expect/panic outside tests, no new dependencies, only teamwire-cli prints).

Report ranked correctness, security and data-loss findings, each with file:line and a concrete failing scenario. Also: anything the task, the design section or the brief requires that is missing; any test that would pass without its fix (stubbed, empty or hard-coded code that makes a check pass); any deviation from the binding names. No style or naming findings. Do not propose fixes. Do not run the full gate.

Write the report at the team's reports/rev_$role.md: a summary of at most 10 lines on top, then the findings, then a **Decisions I made** list. Then run teamwire done.
EOF
}
