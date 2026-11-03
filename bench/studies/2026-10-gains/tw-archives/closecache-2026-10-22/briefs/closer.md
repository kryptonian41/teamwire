# Brief: closer — verify team closecache

You are the closer (kind closer) for team `closecache`. The repository is /private/tmp/claude-501/bnruns/bngcwrebut/clone.
Members: `impl` (implementer, branch `closecache/impl`, head 024f865, base `bench` at f1ac22c)
and `review` (reviewer, report reports/review.md). The joined head is branch `afk/closecache`,
which equals closecache/impl (a fast-forward of bench).

Read first: afk/task.md (the user's task), afk/briefs/impl.md, afk/decisions.md (D1 to D8),
reports/impl.md, reports/review.md, all in .

Do this, following `teamwire preset closer` (clone at "$TEAMWIRE_HOME/cache/closer/closecache/clone",
made once with git clone from /private/tmp/claude-501/bnruns/bngcwrebut/clone; fetch, check out, git reset --hard and git clean -fd per check;
never git worktree add; never copy a target/ folder in; the member setup is empty, so cargo builds
from scratch in the clone):

1. Run `teamwire verify closecache` and read every line.
2. On afk/closecache, run and record each exit code:
   - cargo test -p teamwire-close --locked
   - cargo test -p teamwire-cli --locked --test close
   - TEAMWIRE_BASE=f1ac22cb scripts/gate.sh fast
   - TEAMWIRE_BASE=f1ac22cb scripts/gate.sh   (the full gate)
   - TEAMWIRE_BASE=f1ac22cb scripts/test-changed.sh
3. Decision D6: impl says test-changed.sh fails only tmux_wiring::herdr_env_wins_over_tmux, because
   sessions here have both HERDR_ENV and TMUX set, and that the base fails the same way. Run that
   test on f1ac22c and on afk/closecache in the same environment and record both exit codes. Say
   whether any other test fails.
4. Revert each fix behind each new test in crates/teamwire-close/tests/closer_cache.rs (and the
   test impl added for finding 1) and confirm the test then fails.
5. Decision D7: the close now also deletes caches of other teams whose teams/<name> folder is gone.
   Look hard for data loss: can it delete a cache of a live team (mid-launch, mid-close by another
   process, a team whose state is elsewhere), anything outside cache/closer/, or follow a link?
   Grade what you find.
6. Check each of the four task bullets against the code and tests, with file:line.
7. Grade every finding (wrong conclusion, wrong detail, wording) and name its owner. List new
   `shortcut:` comments with file:line.

Write one integrated report to your report file, most important first. Do not edit source, commit
or message any member. Keep your final chat message to five lines and point at the report.
