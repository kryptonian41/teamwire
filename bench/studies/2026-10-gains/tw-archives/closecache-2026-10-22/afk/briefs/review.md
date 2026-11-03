# Brief: review — read the finished diff for the closer cache delete

You are the reviewer (kind reviewer) for team `closecache`. The implementer `impl` built the change
on branch `closecache/impl`, cut from `bench` at f1ac22c. After you, `closer` re-runs every check.

Read, in this order:
1. The task: afk/task.md in the team directory
   (/private/tmp/claude-501/bnruns/bngcwrebut/twhome/teams/closecache/afk/task.md).
2. The implementer's brief: afk/briefs/impl.md in the same directory. Section 8 lists the decisions
   already made; do not report a finding against those decisions.
3. `AGENTS.md` at the repository root (/private/tmp/claude-501/bnruns/bngcwrebut/clone).
4. The diff: `git -C /private/tmp/claude-501/bnruns/bngcwrebut/clone diff f1ac22c...closecache/impl`,
   and the code it calls.

Report ranked findings, each with file:line and a concrete failure: inputs or state, then the wrong
result. Look hardest at these:
- Data loss: can close delete anything outside `<home>/cache/closer/<team>`? A team name that
  forms a path, a symbolic link at `cache`, at `cache/closer`, or at the team folder itself, a
  link inside the folder.
- Does the cache stay when the team state stays (keep_state, or an earlier step that failed)?
- Does each new test fail without its fix? Look for a test that passes on the base.
- Stubbed, empty or hard-coded code that makes a check pass.
- AGENTS.md hard rules: no unwrap/expect/panic outside tests, Clock for time, files under 400
  lines, no new dependency.
Findings only, no fixes. Do not edit, commit or message anyone. If the diff is sound, say so
plainly rather than inventing findings. Write your findings to your report file, end it with a
**Decisions I made** list, then record done.
