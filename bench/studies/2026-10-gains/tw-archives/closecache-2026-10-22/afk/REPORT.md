# Report: close deletes the closer's cache folder (team closecache)

The finished work is on branch **`afk/closecache`**, at commit `cf45e6a`. The branch `closecache/impl` points at the same commit. Both branches start from `bench` at `f1ac22c`. Nothing was pushed, and nothing was merged into `bench`.

## What you asked

From `afk/task.md`, word for word:

> A team's closer works in a clone kept at `$TEAMWIRE_HOME/cache/closer/<team>/`, which can hold a large `target/` folder. `teamwire close` deletes the team's state directory but leaves that cache folder behind forever.
>
> End state, in crate `teamwire-close`:
>
> - A whole-team close that deletes the team state (`close(ctx, CloseTarget::Team, CloseOpts::default())`) also deletes `<home>/cache/closer/<team>`, and the close still reports no stopped step.
> - The plan's `delete_state` step (from `plan_close`) names the cache folder's path in its detail when the folder exists.
> - With `CloseOpts { keep_state: true, .. }`, the cache folder and its contents stay.
> - When the cache folder is a symbolic link, close removes the link only. Nothing in the folder it points to is touched.
>
> ## How to work and finish
>
> - The repository is teamwire, a Rust tool, with a Bun and TypeScript dashboard in `ui/`. Read `AGENTS.md` first, and `ui/AGENTS.md` for the dashboard.
> - Add a test for each change, in the crate's `tests/` folder for Rust and beside the file as `<name>.test.ts` for the dashboard.
> - Check your work with the tests of each crate you changed (`cargo test -p <crate> --locked`), `bun test` in `ui/` if you changed it, and `TEAMWIRE_BASE=f1ac22cb scripts/gate.sh fast` from the repository root. This checkout has no `main` branch, so the gate needs that variable.
> - Commit the finished work in this repository. Do not push; there is no remote. End your final message with the name of the branch that holds the finished work.
>
> Setup for this run: give every member spec "plugins": {"claude-mem@thedotmack": false}.

## What I understood

The run had four members and no planner (decision D1 in `afk/decisions.md`):

| member | kind | model and effort | job |
| --- | --- | --- | --- |
| `impl` | implementer | Opus, high | wrote the change and its tests |
| `review` | reviewer | Opus, extra high | read the first finished diff |
| `closer` | closer | Opus, extra high | re-ran every check and undid each fix to prove its test fails |
| `reporter` | investigator | Opus, high | wrote this report |

The lead read the task as one change inside one crate. The step that deletes team state already exists, so the cache delete belongs in that same step. The lead fixed four readings before work started:

- The cache goes and stays together with the team state (D4). A close that keeps the state also keeps the cache.
- Only a whole-team close deletes the cache (D5). A close of one member (`--role`) leaves the cache alone.
- The dashboard in `ui/` does not change, because the `--json` output keeps its shape (D2). So `bun test` did not need to run.
- The base is `bench` at `f1ac22c`, because this checkout has no `main` branch (D3).

The setup line was followed. Every member spec in `afk/spec-*.json` turns off `claude-mem@thedotmack`. The specs for `review`, `closer` and `reporter` also turn off two more plugins, `mattpocock-skills@claude-plugins-official` and `ponytail@ponytail`. The task did not ask for those two.

## The result

All four end-state bullets hold on `cf45e6a`. Seven commits change five files, with 351 lines added and 19 removed.

| commit | what it does |
| --- | --- |
| `2b9c5e5` | deletes the closer cache in the `delete_state` step, with tests for a team close and for `keep_state` |
| `30cb7f7` | removes only the link when the cache folder is a symbolic link, also when the link points nowhere |
| `b36fbc7` | names the cache path in the plan's `delete_state` detail when the folder exists |
| `8060b8e` | documents the new behaviour in `docs/api/cli.md` |
| `d620ed0` | rewrites the tests so the lint checker adds no warnings |
| `024f865` | fix round 1: a whole-team close also deletes every other closer cache whose team state folder is gone |
| `cf45e6a` | fix round 2: that wider delete runs only when `cache` and `cache/closer` are real folders, not links |

Here is how the code works now. The file that deletes team state, `crates/teamwire-close/src/delete.rs`, first moves a folder into `<home>/deleting/` and then removes the folder there. A kill at any moment leaves either the whole folder or no folder. The closer cache now goes out the same way, renamed to `<home>/deleting/<team>.closer.<pid>`. The `delete_state` step in `crates/teamwire-close/src/team_close.rs` deletes the cache only after the team folder is gone.

A cache folder that is a symbolic link loses only the link. The code checks the path with `symlink_metadata`, which does not follow a link, and removes a link with `remove_file`. A cache that cannot be deleted adds a note to the step detail and a warning to the log. The step still reports success. By then the team state is gone, and a failed step could never be finished by a second close.

The plan's `delete_state` detail now reads like this when a cache exists:

```
delete <home>/teams/alpha and the closer cache <home>/cache/closer/alpha
```

When no cache exists, the detail is the same as before.

The change goes one step past the task. A close killed between the two deletes would leave a cache that nothing could ever reach again. So a whole-team close now also deletes every cache under `cache/closer/` whose team state folder is gone (D7). That wider delete touches only folders with valid team names. The wider delete runs only when `<home>/cache` and `<home>/cache/closer` are real folders, not links (D9). The closed team's own cache is still deleted when those folders are links.

The new test file `crates/teamwire-close/tests/closer_cache.rs` holds 8 tests:

| test | task bullet |
| --- | --- |
| `a_team_close_deletes_the_closer_cache_and_no_other_team_cache` | 1 |
| `the_plan_names_the_closer_cache_only_when_it_exists` | 2 |
| `keep_state_keeps_the_closer_cache_until_the_state_goes` | 3 |
| `a_closer_cache_that_is_a_link_loses_the_link_only` | 4 |
| `a_closer_cache_link_to_nothing_is_removed_too` | 4 |
| `a_cache_whose_team_state_is_gone_goes_with_the_next_team_close` | the wider delete (D7) |
| `a_linked_closer_folder_loses_only_the_closed_teams_cache` | the link guard (D9) |
| `a_linked_cache_folder_loses_only_the_closed_teams_cache` | the link guard (D9) |

## Verified

The closer checked commit `024f865`. Fix round 2, commit `cf45e6a`, came after the closer, to fix the closer's finding F1. Nobody re-ran the closer on `cf45e6a`. The lead re-ran the three main checks on `cf45e6a`. The full gate and `scripts/test-changed.sh` were last run by the closer, on `024f865`.

Checks on the final commit `cf45e6a`, run by the lead in `.worktrees/closecache-impl` at 2026-10-22 14:48 UTC. The source is `afk/final-checks/exit-codes.md`, with logs beside that file.

| command | exit code |
| --- | --- |
| `cargo test -p teamwire-close --locked` | 0. All 10 test files pass, `closer_cache` 8 of 8 (`afk/final-checks/close.log`) |
| `cargo test -p teamwire-cli --locked --test close` | 0. 16 tests pass (`afk/final-checks/cliclose.log`) |
| `scripts/gate.sh fast`, base `f1ac22cb` | 0. Format, tidy and lint checks pass (`afk/final-checks/gatefast.log`) |
| `teamwire verify closecache --role impl` | passed, as the lead reports |

Checks on `024f865`, run by the closer in its clone at `$TEAMWIRE_HOME/cache/closer/closecache/clone`. The source is `reports/closer.md`.

| command | exit code |
| --- | --- |
| `teamwire verify closecache` | 0 |
| `cargo test -p teamwire-close --locked` | 0 |
| `cargo test -p teamwire-cli --locked --test close` | 0 |
| `TEAMWIRE_BASE=f1ac22cb scripts/gate.sh fast` | 0 |
| `TEAMWIRE_BASE=f1ac22cb scripts/gate.sh` (the full gate) | 1. The gate stopped at its test step on one test, `tmux_wiring::herdr_env_wins_over_tmux` |
| the gate's two later steps, `cargo doc` with warnings as errors and `scripts/check-docs.sh`, run by hand | 0 and 0 |
| `TEAMWIRE_FAKES_PREBUILT=1 cargo test --workspace --locked --no-fail-fast` | 101. 172 test files pass, and only the same tmux test fails |
| `TEAMWIRE_BASE=f1ac22cb scripts/test-changed.sh` | 101. Only the same tmux test fails |

The one failing test comes from this machine, not from the change (D6). The test `herdr_env_wins_over_tmux` expects no `tmux` process among its parent processes. Every member here ran inside tmux. The closer ran the test at the base `f1ac22c` and at `024f865`, in two places:

| commit | inside the member session, under tmux | started with no tmux parent |
| --- | --- | --- |
| `f1ac22c` (base) | 101 | 0 |
| `024f865` | 101 | 0 |

The change touches no code that picks the host. So the full gate run inside tmux on this machine will fail the same way for any branch.

The closer also proved each test catches a real mistake. The closer made eleven small targeted changes to the source, ran the tests after each one, and recorded which tests failed. The table is in `reports/closer.md` under "Each new test fails without its fix". All six tests of that time fail on the base code. The implementer showed the same for the two link guard tests on `cf45e6a`. Both of those fail when one half of the guard is removed.

The reviewer read only the first five commits, up to `d620ed0`. Nobody reviewed `024f865` or `cf45e6a` as a diff. The closer read `024f865` and found F1 there. The reviewer wrote that close cannot delete anything outside the folder that `<home>/cache/closer/<team>` resolves to. That sentence was true for `d620ed0`. That sentence is false for `cf45e6a`, because the wider delete also removes other teams' left-over caches. In my reading of the code at `cf45e6a`, close deletes nothing outside the folder that `<home>/cache/closer/` resolves to. Nobody has tested that claim beyond the closer's probe tests.

I read the final `delete.rs` at `cf45e6a` myself. The guard is at the top of `caches_without_state`: the function returns an empty list unless `symlink_metadata` reports both `cache` and `cache/closer` as real folders. I did not run any check myself.

## Decisions made for you

The full list, with the reasons and how to undo each one, is in `afk/decisions.md` (D1 to D9). These five change the result most:

| decision | what was chosen | how to undo it |
| --- | --- | --- |
| D4 | The cache goes in the same step as the team state, under the same conditions. When the state stays, because of `keep_state` or a failed earlier step, the cache stays too. A close that failed can run again, and the closer may still need its clone then. | Move the cache delete into its own step. |
| D6 | The failing tmux test counts as a problem with this machine. The closer then confirmed the test fails the same way on the base and passes with no tmux parent. | Not needed: the closer's evidence settles it. |
| D7 | A whole-team close also deletes the cache of any other team whose `teams/<name>` folder is gone. Without the wider delete, a close killed between the two deletes leaves the cache forever, which is the bug the task exists to fix. The wider delete also removes caches left by closes from before this change. | Revert `024f865`. The four task bullets still hold without it. |
| D8 | A cache folder on another filesystem is left undeleted, and the step reports why. No test here can create a second filesystem. | Fall back to deleting in place when the rename fails with EXDEV, the "cross-device link" error. |
| D9 | The wider delete from D7 runs only when `cache` and `cache/closer` are real folders. This closes the data-loss path the closer reproduced (F1). | Revert `cf45e6a` and `024f865` together. |

The members made these choices themselves, from their own **Decisions I made** lists:

- **impl**: A failed cache delete adds a note to the step and does not fail the step. A failed step would report a stopped close that no second close can finish, because a team with no state folder counts as already closed.
- **impl**: The folder name in `deleting/` is `<team>.closer.<pid>`. Team names hold no dot, so this name cannot clash with a team folder's `<team>.<pid>`.
- **impl**: The close path for an empty store, in `empty.rs`, does not delete the cache. That path runs only when a launch was killed before any member row was written, so no closer can have run.
- **impl**: The plan counts a link that points nowhere as an existing cache, because close removes it.
- **impl**: A read error on the cache path, other than "not found", goes into the step note and is not treated as "no cache".
- **impl**: The implementer updated `docs/api/cli.md` to say what close deletes.
- **impl**: The implementer reported `blocked` after the first run, because `scripts/test-changed.sh` failed on the tmux test. The lead then settled that with D6.
- **review**: The reviewer rated both findings low, because neither loses data.
- **closer**: The closer rated F1 a wrong conclusion, the highest grade, although it needs a hand-made link. The files it deleted were user data outside teamwire's folders, and nothing could bring them back.

## Not done, and why

- **The closer did not re-check `cf45e6a`.** The lead re-ran three checks there instead, and all three passed. The closer's undo-each-fix checks cover `024f865` only.
- **Nobody reviewed the last two commits as a diff.** The reviewer's verdict covers `d620ed0`.
- **The full gate never passed.** The full gate fails on the tmux test inside any tmux session on this machine (D6). The closer ran the gate's later steps by hand, and they passed.
- **Finding F4 is not fixed.** A regular file under `cache/closer/` with a team-like name, such as `notes`, is moved into `deleting/` and stays there. Each later close logs the same warning again. The fix is one filter line plus a test. The lead did not send it to a fix round. I confirmed the code at `cf45e6a` still has no file-type filter in `caches_without_state`.
- **Finding F5 is not fixed.** Two closes running at the same time can both try the same left-over cache. The second close then reports "could not be deleted" for a cache that is already gone. Only the wording is wrong, and no data is lost.
- **Finding F3 is not fixed.** No test fails when the code branch that removes a link with `remove_file` is deleted. The standard library's `rename` and `remove_dir_all` already leave the link target alone. So the behaviour stays right, but no test pins that branch.
- **Finding F6 is not fixed.** `team_close.rs` has 432 lines. The repository rules say to split a file past 400 lines. The file had 427 lines on the base, and the tidy check fails only past 800 lines.
- **Finding F7 is outside this run.** The closer preset in this repository, `examples/presets/closer.json`, still clones into `mktemp -d`. So a build from this branch deletes a folder that only the installed teamwire build creates. No member owned `examples/`.
- **Nothing was pushed or merged**, as the task asked.

## Risks

- **The final commit has less checking than the commit before it.** The closer's full set of checks covers `024f865`. Commit `cf45e6a` got the three main checks and the verify run from the lead, all passing. The full gate and `scripts/test-changed.sh` last ran on `024f865`, and both failed only on the tmux test. A later `scripts/gate.sh` run outside tmux would close this gap.
- **A cache on another filesystem is never deleted (review finding 2, D8).** Picture a user who links `<home>/cache` or `<home>/cache/closer` to another disk to keep big `target/` folders off the main disk. The rename into `<home>/deleting/` then fails with EXDEV. Every close reports `the closer cache ... could not be deleted: cannot move it: Cross-device link (os error 18)`. The close finishes, and no data is lost, but bullet 1 of the task fails for that setup. The reviewer worked this out from how `rename` behaves and did not reproduce it.
- **The wider delete removes folders that are not teamwire's in one case.** With the D9 guard, the wider delete only looks inside a real `cache/closer` folder under the home. Any team-named folder in there with no `teams/<name>` folder is deleted, with no archive. A user who keeps their own folders inside `<home>/cache/closer/` would lose them. The closer's tests show user folders behind a link are now safe.
- **The closed team's own cache still follows a link one level up (my inference, not tested).** When `cache/closer` is a link, close still deletes `<link target>/<team>`, as the task requires. Suppose two teamwire homes share one cache folder through a link, and both have a team with the same name. Closing that team in one home then deletes the other home's cache for it. The closer found the same mechanism for other team names in F1. The D9 guard fixed that case, but this same-name case remains.
- **A file named like a team gets stuck in `deleting/` (F4)**, as described under "Not done". The file disappears from the user's view, and the warning repeats on every close.

## Cost

The run took about 45 minutes. The lead recorded decision D1 at 14:05 UTC and ran the final checks at 14:48 UTC. This report was written after that, at about 14:50 UTC.

Run time per member, from `teamwire metrics closecache`:

| member | run time |
| --- | --- |
| `impl` | 675 s for the first run, which ended `blocked`. The metric reads 157 s for the two fix runs together. |
| `review` | 414 s |
| `closer` | 1,015 s |
| `reporter` | not in the metrics yet, because this run is still going |

Tokens per member, from the same command:

| member | output | input | cache writes | cache reads | total |
| --- | --- | --- | --- | --- | --- |
| `impl` (Opus, high) | 51,975 | 146 | 142,875 | 8,756,400 | 8,951,396 |
| `closer` (Opus, extra high) | 55,674 | 110 | 156,040 | 6,885,893 | 7,097,717 |
| `review` (Opus, extra high) | 33,740 | 54 | 114,535 | 2,743,884 | 2,892,213 |
| **all three** | 141,389 | 310 | 413,450 | 18,386,177 | **18,941,326** |

The metrics hold no token count for the reporter or for the lead.
