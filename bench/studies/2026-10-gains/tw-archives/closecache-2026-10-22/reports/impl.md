# impl report: close deletes the closer cache

## Progress
- 2b9c5e5 delete the closer cache in delete_state, tests for team close and keep_state | left: symbolic link handling, plan detail, checks
- 30cb7f7 remove only the link when the closer cache is a symbolic link, also a link to nothing | left: plan detail, checks
- b36fbc7 name the closer cache in the plan's delete_state detail when it exists | left: docs check, section 3 checks
- 8060b8e document in docs/api/cli.md that close deletes the closer cache | left: section 3 checks
- d620ed0 closer cache tests use Platform::write_atomic and unwrap only in tests, so tidy reports no WEAKENED line | left: test-changed, final crate run
- 024f865 fix round 1: a whole-team close also deletes every closer cache whose teams/<team> is gone (review finding 1), plan names them, docs updated | left: none
- cf45e6a fix round 2: sweep other teams' caches only when cache and cache/closer are real folders (closer F1, decision D9); own cache path unchanged; 2 tests | left: none

## What changed
- `crates/teamwire-close/src/delete.rs`: `closer_cache(home, team)` builds
  `<home>/cache/closer/<team>`. `delete_closer_cache(ctx, team)` deletes it.
  A real folder is renamed to `<home>/deleting/<team>.closer.<pid>` and then
  removed, the same kill-safe path the team directory uses (the shared part is
  now `doomed_path`). A symbolic link is checked with `symlink_metadata` and
  removed with `remove_file`; `remove_dir_all` never sees a link. A missing
  folder returns `None` and changes nothing.
- `crates/teamwire-close/src/team_close.rs`: `delete_state` calls it only after
  `delete_team_dir` succeeds, and appends its note to the step detail.
  `keep_state` and every earlier failure return before it, so the cache stays
  whenever the state stays (D4). Member close (`remove.rs`) is untouched (D5).
- `crates/teamwire-close/src/plan.rs`: the `delete_state` detail becomes
  `delete <teams/team> and the closer cache <path>` when the path exists
  (a link to nothing counts). Otherwise the detail is unchanged.
- `docs/api/cli.md`: the close section now says the closer cache goes in
  `delete_state`, stays with `--keep-state`, and a link loses only the link.
- New test file `crates/teamwire-close/tests/closer_cache.rs`, 5 tests.

## Checks (all on d620ed0)
| command | result |
| --- | --- |
| `cargo test -p teamwire-close --locked` | pass (all binaries ok) |
| `cargo test -p teamwire-cli --locked --test close` | pass, 16 tests |
| `TEAMWIRE_BASE=f1ac22cb scripts/gate.sh fast` | exit 0, no WEAKENED or PROTECTED-CHANGED line |
| `cargo clippy -p teamwire-close --all-targets --locked -- -D warnings` | clean |
| `TEAMWIRE_BASE=f1ac22cb scripts/test-changed.sh` | **exit 101**, one failure, see below |

### The test-changed failure comes from this machine, not this change
`test-changed.sh` runs teamwire-cli, teamwire-close, teamwire-crash-tests and
teamwire-e2e (cli is pulled in by the docs/api/cli.md edit). I reran its own
test command with `--no-fail-fast`: 57 test binaries ok, 1 failed:

```
teamwire-cli --test tmux_wiring :: herdr_env_wins_over_tmux
assertion `left == right` failed: refused [a]: host_listing_failed: role a: the host's
agent list could not be read: other: could not run tmux: not_found ...
  left: Some(1)
 right: Some(0)
```

The test's own comment says it holds only when no `tmux` or `herdr` process
is among the test's parents. This session's parent chain is
`zsh -> claude -> tmux`. The same test fails the same way on the base commit
f1ac22cb: I detached to f1ac22cb on a clean tree, ran
`cargo test -p teamwire-cli --locked --test tmux_wiring herdr_env_wins_over_tmux`,
got the identical assertion, and switched back. My diff touches no launch or
host-selection code. A run outside tmux should pass it.

### Every new test fails on the base code
Detached to f1ac22cb with only `closer_cache.rs` copied in: all 5 tests
FAILED (0 passed). On the branch: 5 passed.

| test | brief bullet |
| --- | --- |
| a_team_close_deletes_the_closer_cache_and_no_other_team_cache | 1 (also: `cache/closer/beta` stays) |
| the_plan_names_the_closer_cache_only_when_it_exists | 2 |
| keep_state_keeps_the_closer_cache_until_the_state_goes | 3 (keep_state keeps it; the rerun that deletes the state deletes it, which is why it fails on base) |
| a_closer_cache_that_is_a_link_loses_the_link_only | 4 |
| a_closer_cache_link_to_nothing_is_removed_too | 4 (a dangling link; `Path::exists` would miss it) |

Note for the closer: reverting only the `symlink_metadata` branch does not
make `a_closer_cache_that_is_a_link_loses_the_link_only` fail, because std's
`remove_dir_all` on a renamed link also removes only the link. The
dangling-link test does fail on that revert (seen on 2b9c5e5). The code still
keeps `remove_dir_all` away from links, as the brief's rule requires.

## Decisions I made
- A failure to delete the closer cache does **not** fail `delete_state`. The
  step stays ok, and its detail says `the closer cache <path> could not be
  deleted: <why>`, with a `tracing::warn`. Reason: the cache is deleted after
  the team directory is gone. A failed step at that point would report a
  stopped close that no rerun can finish, because a closed team with no
  directory is refused as "already closed". The cache can be rebuilt, so it
  is not worth a stopped close.
- The cache is deleted only after the team directory deletion succeeds, so a
  failed team-dir move keeps both (D4).
- The kill-safe name in `deleting/` is `<team>.closer.<pid>`. Team names are
  `[a-z0-9_]`, so it cannot collide with a team's `<team>.<pid>`, and the
  existing sweep of left-over directories still parses the pid after the last dot.
- A `symlink_metadata` error other than not-found is reported as a note, not
  treated as "no cache".
- The empty-store close path (`empty.rs`, a launch killed before any row is
  written) does not delete the cache. No closer can have run for such a store,
  and the brief scopes the change to the `delete_state` step.
- The plan counts a dangling link as an existing cache, because close removes it.
- I updated `docs/api/cli.md`, which states what close deletes.
- Tests write files through `rig.platform.write_atomic` and unwrap only inside
  `#[test]` functions, so the new test file adds no `#[expect(` (tidy flags
  every added `#[expect(` as WEAKENED).
- Status is `blocked`, not `done`: `scripts/test-changed.sh` exits 101 on the
  environment-caused failure above, and the rules say done needs every named
  check to pass.

## Fix round 1 (lead message m001, findings in reports/review.md)

### Finding 1, a close killed between the two deletes leaves the cache: holds, fixed in 024f865
I checked it against the code. The CLI opens the store with
`Store::open_existing`, so once `teams/<team>` is gone no close of that team
can run again, and nothing else looked at `cache/closer/`.

Fix: `delete_state`, after the team directory is deleted, now deletes every
closer cache whose `teams/<name>` is gone (`caches_without_state` and
`delete_closer_caches` in `delete.rs`). The closed team's own cache is one of
them, so the earlier single-cache path is folded into this one. Rules:
- Only folder names that parse as a `TeamName` are considered. Any other
  entry under `cache/closer/` is left alone.
- "Gone" means `symlink_metadata` on `teams/<name>` returns NotFound. Any other
  error keeps the cache.
- A team kept with `--keep-state` still has `teams/<name>`, so its cache stays (D4).
- Each cache uses the same symlink-aware, kill-safe delete as before.
- The plan's `delete_state` detail lists those caches too, after the team's
  own cache: `delete <dir> and the closer cache <a>, <b>`. The text for the
  single-cache case is unchanged.

Test: `a_cache_whose_team_state_is_gone_goes_with_the_next_team_close` makes
`cache/closer/gone` (no `teams/gone`) and `cache/closer/Not-a-team`, checks the
plan detail, closes `alpha`, and asserts `gone` is deleted and `Not-a-team`
stays. It failed before the code change (ran it: 1 failed, 5 passed) and passes
after.

Fixture change: in `a_team_close_deletes_the_closer_cache_and_no_other_team_cache`
I added `teams/beta` so `beta` stays a live team. Without it, `beta`'s cache
now counts as left over and goes, which is correct. No assert changed or was
removed, and tidy reports no WEAKENED line.

### Finding 2, `cache` linked to another filesystem gives EXDEV: holds in theory, not fixed
The reasoning is right: `rename(2)` across filesystems fails with EXDEV, and
the step detail then says `cannot move it: Cross-device link`. I did not
change code, for these reasons:
- No test can fail without a fix here. TestHome and every tempdir are on one
  filesystem, and a test may not mount a volume. The rules require a failing
  test for each behaviour I add.
- The failure is visible: the step detail and a `tracing::warn` name the path
  and the error. No data is lost and the close finishes.
- Nothing in this repository creates `cache/closer/` at all (the reviewer's
  own note), let alone a link to another volume.
- A later fix is small: on `ErrorKind::CrossesDevices`, `remove_dir_all` the
  folder in place. Doing that loses the kill-safety for that case. With the
  finding 1 sweep, a later close would finish a half-deleted cache anyway.
  I recommend that change only together with a test rig that can provide a
  second filesystem.

### Checks on 024f865
| command | result |
| --- | --- |
| `cargo test -p teamwire-close --locked` | pass, 0 failed binaries |
| `cargo test -p teamwire-cli --locked --test close` | pass, 16 tests |
| `TEAMWIRE_BASE=f1ac22cb scripts/gate.sh fast` | exit 0 (fmt, tidy, clippy ok; no WEAKENED) |
| `scripts/test-changed.sh` | not rerun: the tmux failure is decision D6 and the lead said the closer checks it |

### Decisions I made in fix round 1
- The sweep covers every left-over cache under `cache/closer/`, not only the
  closed team's. That is the only way to reach a cache whose team can no
  longer be closed. It also removes caches left by closes from before this
  change. Only a whole-team close that deletes its state runs it (D4, D5).
- Correction to my earlier decision list: team names are `^[a-z][a-z0-9]{0,11}$`,
  with no underscore. The `<team>.closer.<pid>` name still cannot collide with
  a team directory's name.
- Finding 2 is answered with evidence and not fixed, as described above.
- Status: `done`. The lead's message says the test-changed tmux failure is
  logged as decision D6 for the closer, so the checks left for me all pass.

## Fix round 2 (lead message m003, closer finding F1, decision D9)

### F1, the sweep follows a link at `cache` or `cache/closer`: holds, fixed in cf45e6a
The finding is right. `read_dir` on `<home>/cache/closer` follows a link at
either folder, so any user folder with a team-shaped name was deleted.

Fix, as D9 decides:
- `caches_without_state` returns nothing unless `<home>/cache` and
  `<home>/cache/closer` are both real folders. The check uses
  `symlink_metadata(..).is_dir()`, which is false for a link. The plan uses
  the same function, so the dry run also stops naming those folders.
- `delete_closer_caches(ctx, team)` now deletes the closed team's own cache
  first, by its path, as before 024f865. Then it deletes the other left-over
  caches, leaving out the team itself. So the own cache still goes when
  `cache/closer` is a link (the four brief bullets hold).

Tests, both in `crates/teamwire-close/tests/closer_cache.rs`:
| test | what it pins | fails without |
| --- | --- | --- |
| `a_linked_closer_folder_loses_only_the_closed_teams_cache` (the closer's probe, adapted) | `cache/closer` is a link to a folder with `photos/a.jpg`, `bravo/clone`, `alpha/clone`. The plan names only `alpha`. The close deletes `alpha` and keeps `photos` and `bravo`. | failed on 024f865 code (ran it: plan named bravo and photos) and fails with the `cache/closer` half of the check replaced by `true` |
| `a_linked_cache_folder_loses_only_the_closed_teams_cache` | `cache` is a link to a folder with `closer/photos` and `closer/alpha`. The close deletes `alpha` and keeps `photos`. | fails with the `cache` half of the check replaced by `true` (ran it) |

Docs: `docs/api/cli.md` now says the sweep runs only when `cache` and
`cache/closer` are real folders, not symbolic links.

### Checks on cf45e6a
| command | result |
| --- | --- |
| `cargo test -p teamwire-close --locked` | pass, 0 failed binaries; closer_cache 8 passed |
| `cargo test -p teamwire-cli --locked --test close` | pass, 16 tests |
| `TEAMWIRE_BASE=f1ac22cb scripts/gate.sh fast` | exit 0 (the first run failed on fmt in the new test; fixed with `cargo fmt` before the commit) |
| `scripts/test-changed.sh` | not rerun: the tmux failure is decision D6, which the closer checks |

### Other closer findings, not in this round's scope
The lead routed only F1 to this round. I did not change code for these:
- F3: correct. No test sees the `is_symlink` arm. The arm stays because the
  brief requires that `remove_dir_all` never runs on a link. My earlier note
  "fails on that revert (seen on 2b9c5e5)" meant going back to the `exists()`
  check, not removing the arm.
- F4: a regular file named like a team under a real `cache/closer/` still
  goes into `deleting/` and stays there. The fix is to keep only folders and
  links in `caches_without_state`, which is one filter line, but it needs its
  own test. Left for the lead to route.
- F5: two closes at the same time can report a cache as "could not be
  deleted" when the other close already deleted it. This is wording only.

### Decisions I made in fix round 2
- I added a second test for a link at `cache`, beside the closer's probe for
  `cache/closer`. Each half of the guard has a test that fails without it.
- The own-cache lookup keeps the pre-sweep rule: any `symlink_metadata`
  result other than NotFound counts as a cache, so a read error is reported
  in the step note instead of being skipped.
