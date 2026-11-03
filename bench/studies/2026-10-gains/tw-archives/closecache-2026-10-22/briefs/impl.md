# Brief: impl — close deletes the closer's cache folder

## 1. Who you are
You are the implementer (kind implementer) for team `closecache`. You own the whole change.
Other members, added after you finish: `review` (reviewer) reads your finished diff, then
`closer` (closer) re-runs your checks and reverts each fix to prove your tests fail without it,
then `reporter` writes the user's report. Nobody else edits code.

## 2. The end state
A team's closer works in a clone under `$TEAMWIRE_HOME/cache/closer/<team>/`, which can hold a
large `target/` folder. Today `teamwire close` deletes the team's state directory and leaves that
cache folder behind forever. When you finish, in crate `teamwire-close`:

- A whole-team close that deletes the team state
  (`close(ctx, CloseTarget::Team, CloseOpts::default())`) also deletes `<home>/cache/closer/<team>`,
  and the close still reports no stopped step.
- The plan's `delete_state` step (from `plan_close`) names the cache folder's path in its detail
  when the folder exists. When the folder does not exist, the detail stays as it is today.
- With `CloseOpts { keep_state: true, .. }`, the cache folder and its contents stay.
- When the cache folder is a symbolic link, close removes the link only. Nothing in the folder it
  points to is touched.

## 3. Done when
- Each of the four bullets above has a test in `crates/teamwire-close/tests/` that fails on the
  base commit and passes on your branch.
- `cargo test -p teamwire-close --locked` exits 0.
- `cargo test -p teamwire-cli --locked --test close` exits 0 (the CLI pins the close step names).
- `TEAMWIRE_BASE=f1ac22cb scripts/gate.sh fast` exits 0 from your worktree root.
- `TEAMWIRE_BASE=f1ac22cb scripts/test-changed.sh` exits 0.

## 4. Out of scope
- A member close (`--role`) does not touch the cache. Only a whole-team close does.
- No new close step name. The cache goes inside the existing `delete_state` step (decision D4).
- The dashboard in `ui/` does not change (D2). If you find you must change `--json` output, stop
  and ask the lead with `teamwire send closecache --to lead --type question`.
- No new dependencies, no Cargo.lock change, no protected file (AGENTS.md lists them).

## 5. When to do nothing
If the base already deletes `<home>/cache/closer/<team>` on a whole-team close, change nothing.
Write a report that shows the test proving it, and finish.

## 6. Where you are
Your worktree and branch `closecache/impl` are cut from branch `bench` at commit f1ac22c.
This checkout has no `main` branch, so every script that compares against a base needs
`TEAMWIRE_BASE=f1ac22cb`. Rust needs no install step. The first cargo build in a fresh worktree is
slow; that is expected.

## 7. Read first, in this order
1. `AGENTS.md` at the repository root (hard rules: Platform::write_atomic, Clock, no unwrap,
   tests use `teamwire_fakes::TestHome`, files under 400 lines).
2. `crates/teamwire-close/src/lib.rs` (its //! note) and `docs/conventions.md`.
3. `crates/teamwire-close/src/team_close.rs` (`delete_state`, about line 393),
   `crates/teamwire-close/src/delete.rs` (`delete_team_dir`), `crates/teamwire-close/src/plan.rs`
   (the `delete_state` plan step, about line 111).
4. `crates/teamwire-close/tests/close.rs` for how existing close tests build a home and a team.

## 8. Decisions already made (2026-10-22, full log in afk/decisions.md of the team directory)
- D4: the cache folder goes with the team state, in the same `delete_state` step and under the same
  conditions. When the state stays (keep_state, or an earlier step failed), the cache stays too.
- D5: only a whole-team close deletes the cache.
- Symbolic link rule: check with `symlink_metadata`, never `metadata`, and remove a link with
  `remove_file` (Unix). Never call `remove_dir_all` on a path that is a link.
- A cache folder that does not exist is the normal case and is not an error.
- Decide yourself whether a failure to delete the cache fails the `delete_state` step. Write the
  choice and the reason under **Decisions I made**. The task requires that a normal close reports
  no stopped step.

## 9. What already exists
- `delete_team_dir` in `delete.rs` already removes a directory in a way that survives a kill
  partway through. Reuse it, or its pattern, for the cache folder instead of writing a second one.
- The home directory is known to the close context; the state directory path is built from it.
  Build the cache path from the same home, as `<home>/cache/closer/<team>`.

## 10. Units in order, commit after each
1. Delete the cache folder in the `delete_state` step, with tests for bullets 1 and 3.
2. Symbolic link handling, with a test that makes the cache folder a link to a temp folder holding a
   file, closes, and asserts the link is gone and the file and folder are still there.
3. The plan detail names the cache path when it exists, with a test on `plan_close`.
Use the tdd skill for each unit; test only at these seams: `close(ctx, CloseTarget::Team, ...)` and
`plan_close`. If a check fails and the cause is not clear from its output, use the
diagnosing-bugs skill before your second fix attempt.

## 11. Files you own
`crates/teamwire-close/` (src and tests). Also `docs/` only if a doc states what close deletes
and is now wrong. Every other file belongs to nobody in this run: ask the lead before editing one.

## 12. Verification
Run the commands in section 3. Do not run the full `scripts/gate.sh` (without `fast`); the closer
runs that.

## 13. How to finish
No push, no pull request, no merge. Write your report at the path launch gave you. End it with a
**Decisions I made** list, and a progress line per commit. Then record done with `team_done` or
`teamwire done`.

## 14. Open questions you must not answer
None.
