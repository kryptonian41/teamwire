D1  2026-10-22 14:05  Which members does this run need?
Chose: impl (implementer, opus high), review (reviewer, opus xhigh), closer (closer, opus xhigh), reporter (investigator, opus high).
Skipped: analyst, scouts, planner, plan reviewer.
Why: the task names its end state in four checkable bullets, in one crate, and the code that deletes team state is one function (crates/teamwire-close/src/team_close.rs delete_state, crates/teamwire-close/src/delete.rs). One implementer reads that code faster than a planner could write it up. A reviewer and the closer still read the finished diff, so a misread of the task gets caught there.
Reverse by: add a planner with teamwire launch --add if the implementer reports the task is bigger than one change.

D2  2026-10-22 14:05  Does the dashboard (ui/) change?
Chose: no.
Why: the task changes only what close deletes. Close's --json output and the dashboard schemas stay the same.
Reverse by: the implementer reports a changed --json shape; then ui/src/shared/teamwire.ts changes in the same commit.

D3  2026-10-22 14:05  Base branch.
Chose: bench at f1ac22c. This checkout has no main branch.
Why: bench is the checked-out branch and the task gives f1ac22cb as TEAMWIRE_BASE.
Reverse by: re-cut the branch from another base.

D4  2026-10-22 14:05  What happens to the cache folder when the step that deletes team state fails or does not run?
Chose: the cache folder goes with the team state, in the same step and under the same conditions. When the state stays (keep_state, a failed earlier step), the cache folder stays too.
Why: the task ties the cache to "a close that deletes the team state". A failed close may be run again, and the closer may still need its clone then.
Reverse by: move the cache delete into its own step.

D5  2026-10-22 14:05  Does a member close (--role) delete the cache?
Chose: no. Only a whole-team close does.
Why: the task says "whole-team close". The closer's clone serves the whole team.
Reverse by: none needed.

D6  2026-10-22 14:18  impl reported blocked: scripts/test-changed.sh fails one test, tmux_wiring::herdr_env_wins_over_tmux, in teamwire-cli.
Chose: treat it as a failure of this machine's environment, not of the change. The closer must confirm it fails the same way on base f1ac22c, and that it is the only failure.
Why: the member sessions run with both HERDR_ENV=1 and TMUX set, and impl reports the base commit fails the same test. The change touches no host-selection code.
Reverse by: the closer finds the test passes on base in the same environment; then the change caused it and goes back to impl.

D7  2026-10-22 14:40  impl's fix for review finding 1 widens the delete: a whole-team close now also deletes every cache under cache/closer/ whose teams/<name> folder is gone, not only the closed team's cache.
Chose: keep it, and have the closer test the wider delete for data loss before it ships.
Why: without it, a close killed between deleting the state and deleting the cache leaves the cache forever, which is the bug the task exists to fix. A cache whose team state folder is gone belongs to a closed team. Only folders with valid team names are touched, and a --keep-state team keeps its teams/ folder.
Reverse by: revert commit 024f865 on closecache/impl. The four task bullets still hold without it.

D8  2026-10-22 14:40  Review finding 2: a cache folder on another filesystem cannot be renamed, so it is never deleted.
Chose: leave it unfixed and list it under Risks.
Why: no test in this repository can make a second filesystem, the close still finishes, and a warning names the error. The closer cache is made by git clone inside $TEAMWIRE_HOME, so it sits on the home's filesystem unless someone links it elsewhere by hand.
Reverse by: fall back to deleting in place when rename fails with EXDEV.

D9  2026-10-22 14:42  The closer's finding F1: the wider delete from D7 follows a link at cache or cache/closer and deletes user folders outside teamwire's folders (reproduced).
Chose: keep the wider delete, but run the sweep of other teams' caches only when <home>/cache and <home>/cache/closer are real directories, checked with symlink_metadata. The closed team's own cache keeps today's handling. Fix round 2 (the last) goes to impl, with the closer's probe as the failing test.
Why: the guard closes the reproduced data-loss path and keeps the fix for a killed close. Reverting 024f865 would also work, but would bring back the left-over cache after a killed close.
Reverse by: revert the guard commit and 024f865 together.
