# closer report: team closecache

Branches checked: the joined head `afk/closecache` at 024f865. The implementer's branch
`closecache/impl` is the same commit, so each run on 024f865 counts for both. The base is
`bench` at f1ac22c. Every command ran in my temporary clone at
`$TEAMWIRE_HOME/cache/closer/closecache/clone`. Logs are in
`/private/tmp/claude-501/-private-tmp-claude-501-bnruns-bngcwrebut-clone/f7e393a0-19b8-4226-b8d6-62405815aca0/scratchpad/`.

## Verdict

All four task bullets hold on 024f865, and each has a test that fails on the base. The tmux test
failure is caused by this machine's environment, as decision D6 says, and it is the only failing
test in the whole workspace.

One finding should block shipping as is. The wider cache delete from decision D7 follows a
symbolic link at `cache/closer`. Through that link it deletes user folders outside teamwire's
own folders. I reproduced the deletion. The commit before the wider delete (d620ed0) does not
have the problem.

## Findings, most important first

### F1. Wrong conclusion. Owner: impl (code), lead (decision D7). The sweep of left-over caches deletes folders outside `cache/closer/` through a link

- Where: `crates/teamwire-close/src/delete.rs:52-66` (`caches_without_state`), called from
  `delete.rs:73-79` and `team_close.rs:409-414`.
- Mechanism: the sweep calls `read_dir` on `<home>/cache/closer`. `read_dir` follows a link at
  `cache` or at `cache/closer`. The sweep then keeps every entry whose name parses as a team
  name. A team name is any lower-case name of 1 to 12 letters and digits, such as `photos`,
  `src` or `notes`. The sweep deletes each entry whose `teams/<name>` is missing.
- Reproduced: I wrote a probe test in the clone. The probe makes `<home>/cache/closer` a link
  to a folder that holds `photos/a.jpg` and `bravo/clone`. Then the probe closes team `alpha`.

  | commit | plan detail of `delete_state` | after the close |
  | --- | --- | --- |
  | 024f865 (joined head) | `delete .../teams/alpha and the closer cache .../cache/closer/bravo, .../cache/closer/photos` | `photos/a.jpg` deleted, `bravo` deleted, step ok, `stopped_at: None` |
  | d620ed0 (before the sweep) | `delete .../teams/alpha` | `photos/a.jpg` and `bravo` still there |

  The probe file is
  `/private/tmp/claude-501/-private-tmp-claude-501-bnruns-bngcwrebut-clone/f7e393a0-19b8-4226-b8d6-62405815aca0/scratchpad/closer_probe.rs`.
  Its first test is a ready failing test for a fix.
- Second case with the same mechanism: two teamwire homes share one cache folder through a link,
  for example to reuse `target/` folders. A close in home A then deletes the cache of a live
  team in home B, because home A has no `teams/<name>` for that team. In the probe, `bravo`
  stands for that live team.
- Limits: the deletion needs a hand-made link at `cache/closer`, or at `cache` when the target
  holds a shared `closer` folder. The link target must be on the same filesystem as
  `<home>/deleting`. On another filesystem the rename fails with EXDEV, and nothing is deleted
  (decision D8). The dry run does name each path it will delete (`plan.rs:125-129`).
- Why the grade is wrong conclusion: decision D7 kept the sweep because "only folders with
  valid team names are touched". That rule does not stop this case. The deleted folders go
  through `remove_dir_all` with no archive, so nothing can recover them.
- Suggested fix, for the parent to route: run the sweep of other teams' caches only when
  `<home>/cache` and `<home>/cache/closer` are real directories, checked with
  `symlink_metadata`. The closed team's own cache can keep today's path, so the four bullets
  still hold. Reverting 024f865 also removes the problem, as D7 notes.

### F2. Wrong conclusion if reused. Owner: review. The review's safety verdict was written for d620ed0 and is false for the joined head

- `reports/review.md` says close "cannot delete anything outside the folder that
  `<home>/cache/closer/<team>` resolves to". The review table rates "link at `cache` or
  `cache/closer`, same filesystem" as safe. Both statements held for d620ed0, the commit the
  reviewer read. Commit 024f865 came later and made both false (F1).
- The review's note "caches left by closes that ran before this change stay" is also false
  now. The sweep removes those caches.
- Nobody re-reviewed 024f865. The reporter must not quote the review verdict as covering the
  shipped code.

### F3. Wrong detail. Owners: review and impl. No test checks the symbolic-link branch of the cache delete

- Where: `delete.rs:88-90`, the arm `Ok(meta) if meta.file_type().is_symlink()` that calls
  `remove_file`.
- I deleted that arm only (mutation M2). All 6 tests still pass on 024f865, and all 5 tests
  still pass on d620ed0. The behaviour stays right without the arm. `rename` moves the link
  itself, and the standard library's `remove_dir_all` removes a link and does not follow it.
- What the tests do pin: replacing `symlink_metadata` with `metadata` at `delete.rs:86` (M3)
  makes `a_closer_cache_link_to_nothing_is_removed_too` fail at `closer_cache.rs:101`. A delete
  that resolves the link first (M2b, `canonicalize` before the move) makes
  `a_closer_cache_that_is_a_link_loses_the_link_only` fail at `closer_cache.rs:83`.
- The review says "The `is_symlink` branch at `delete.rs:59` is what makes the link-to-nothing
  test pass". That is wrong. The `symlink_metadata` call is what makes it pass. The impl report
  says the link-to-nothing test "does fail on that revert (seen on 2b9c5e5)". That holds only
  for going back to the `exists()` check of 2b9c5e5, not for removing the arm.
- Bullet 4 still holds. The arm keeps the brief's rule "never call `remove_dir_all` on a path
  that is a link", and no test can see that rule.

### F4. Wrong detail. Owner: impl. A file named like a team under `cache/closer/` gets stuck in `deleting/`

- Reproduced with the second probe test. A regular file at `<home>/cache/closer/notes` is moved
  to `<home>/deleting/notes.closer.<pid>`. `remove_dir_all` then fails with "Not a directory
  (os error 20)". The step note says the file "is left for a later close to remove".
- The left-over sweep (`delete.rs:148-150`) also calls `remove_dir_all`, so no later close can
  remove the file. Every later close logs the same warning again.
- Low impact: the file vanishes from the user's view, and the warning repeats forever.
  Fix: keep only directories and links in `caches_without_state`.

### F5. Wording. Owner: impl. Two closes running at the same time can report a cache as "could not be deleted" when it is gone

- I reasoned this from `delete.rs:73-99` and did not reproduce it. Close A and close B both
  list the same left-over cache. Close A renames the cache first. Close B then gets "No such
  file or directory" from `symlink_metadata` or from `rename`. Close B's step note says
  `the closer cache <path> could not be deleted: ...`, and a `tracing::warn` fires.
- No data is lost, and the step stays ok. Fix: treat NotFound at that point as "already gone".

### F6. Wording. Owner: impl. `team_close.rs` grew past the split limit

- `team_close.rs` has 432 lines at 024f865 and 427 lines at the base. AGENTS.md says to split
  a file past 400 lines. Tidy fails only past 800, so no check stops it. The review reported
  this already.

### F7. Wrong detail. Owner: none in this run (lead decides). The repository's closer preset never creates the cache this change deletes

- `examples/presets/closer.json:12`, `examples/presets/closer.json:32` and
  `crates/teamwire-launch/contracts/kind_closer.md:10` still tell the closer to clone into
  `mktemp -d`.
- The installed `teamwire 0.0.0+919f5f8ca8c3` gives a different preset. That preset puts the
  clone at `$TEAMWIRE_HOME/cache/closer/$TEAMWIRE_TEAM/clone`, and says `teamwire close`
  deletes it.
- So a build from this branch deletes a folder that only the installed build creates. The
  task is still met. `examples/` was outside every member's owned paths.

## Checks I ran (all on 024f865 unless named)

| command | exit | notes |
| --- | --- | --- |
| `teamwire verify closecache` | 0 | "clean". impl PASS, exit 0, 0 flags. review and closer SKIPPED: pane members with no check. |
| `cargo test -p teamwire-close --locked` | 0 | 10 test binaries, 0 failed; `closer_cache` 6 passed |
| `cargo test -p teamwire-cli --locked --test close` | 0 | 16 passed |
| `TEAMWIRE_BASE=f1ac22cb scripts/gate.sh fast` | 0 | fmt, tidy, clippy ok; no WEAKENED or PROTECTED-CHANGED line |
| `TEAMWIRE_BASE=f1ac22cb scripts/gate.sh` (full) | 1 | stops at step 4 (test) on `tmux_wiring::herdr_env_wins_over_tmux`; steps 5 and 6 never ran |
| `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked` (gate step 5, run by hand) | 0 | |
| `scripts/check-docs.sh` (gate step 6, run by hand) | 0 | "docs check: ok" |
| `TEAMWIRE_FAKES_PREBUILT=1 cargo test --workspace --locked --no-fail-fast` | 101 | 172 test binaries ok, 1 failed: `tmux_wiring`, only `herdr_env_wins_over_tmux` |
| `TEAMWIRE_BASE=f1ac22cb scripts/test-changed.sh` | 101 | packages cli, close, crash-tests, e2e; fails on the same tmux test only |

The full gate cannot pass inside a tmux session on this machine. The parent's own gate run will
fail the same way unless it runs outside tmux (see D6 below).

## Decision D6: the tmux test

`cargo test -p teamwire-cli --locked --test tmux_wiring herdr_env_wins_over_tmux`:

| commit | inside this session (HERDR_ENV=1, TMUX set, tmux is a parent process) | detached, parent chain is launchd with no tmux |
| --- | --- | --- |
| f1ac22c (base) | exit 101 | exit 0 |
| 024f865 (joined head) | exit 101 | exit 0 |

Both failures print the same assertion: `could not run tmux: not_found ... left: Some(1) right:
Some(0)` at `crates/teamwire-cli/tests/tmux_wiring.rs:391`. The test's own comment says it holds
only when no tmux or herdr process is a parent. To get a run with no tmux parent, I started
cargo in a double-forked subshell, `( ( cargo test ... ) & )`. D6 is confirmed. No other
test fails anywhere in the workspace (the `--no-fail-fast` run above).

## Each new test fails without its fix

Every test file below is `crates/teamwire-close/tests/closer_cache.rs` at 024f865. Each row
started from a clean `git reset --hard` and `git clean -fd`.

| change made in the clone | tests that fail (line) | tests that still pass |
| --- | --- | --- |
| base source f1ac22c with the new test file | all 6: plan :118, sweep :136, team close :45, link to nothing :101, link :83, keep_state :66 | none |
| M1: remove the `delete_closer_caches` call, `team_close.rs:409-414` | team close :45, keep_state :66, link :83, link to nothing :101, sweep :147 | plan |
| M2: remove only the `is_symlink` arm, `delete.rs:88-90` | none (exit 0) | all 6 (F3) |
| M2 on d620ed0 | none (exit 0) | all 5 |
| M2b: M2 plus `canonicalize` before the move | link :83 | 5 |
| M3: `metadata` for `symlink_metadata`, `delete.rs:86` | link to nothing :101 | 5 |
| M5: base `plan.rs` | plan :118, sweep :136 | 4 |
| M5b: plan always names the own cache, `plan.rs:123` | plan :124, sweep :136 | 4 |
| M6: source from d620ed0, before the sweep (fix for review finding 1) | sweep :136 | 5 |
| M6b: delete only the closed team's own cache | sweep :147 | 5 |
| M7: sweep ignores whether `teams/<name>` exists, `delete.rs:59-62` | team close :46 (`beta` deleted), plan :118 | 4 |
| M8: delete the own cache in the `keep_state` branch, `team_close.rs:395` | keep_state :60 | 5 |

Every test catches at least one realistic mistake. The `is_symlink` arm alone is unpinned (F3).
On the base, the `keep_state` test fails only in its second half, at the rerun (:66). M8
shows that the first half (:60) also catches a cache deleted under `keep_state`.

## The four task bullets

| bullet | code | test | holds |
| --- | --- | --- | --- |
| 1. A whole-team close deletes `<home>/cache/closer/<team>` and reports no stopped step | `team_close.rs:409-414` calls `delete_closer_caches` after `delete_team_dir` succeeds. The closed team's cache is in the sweep because `teams/<team>` was just moved away (`delete.rs:29-31`, `delete.rs:59-61`). A failed cache delete is a note, not an error (`delete.rs:101-110`). | `closer_cache.rs:33-47`, asserts at :43 and :45 | yes |
| 2. The plan's `delete_state` detail names the cache path when the folder exists | `plan.rs:115-136`. The own cache is named when `symlink_metadata` succeeds (`plan.rs:122-123`). The detail is unchanged when no cache exists (`plan.rs:132-133`). | `closer_cache.rs:113-125`, asserts at :118 and :124 | yes |
| 3. With `keep_state`, the cache and its contents stay | `team_close.rs:395-400` returns before any delete | `closer_cache.rs:49-67`, assert at :60 | yes |
| 4. A cache that is a link loses the link only | `delete.rs:86-90`: `symlink_metadata`, then `remove_file` | `closer_cache.rs:69-91` and `:93-105` | yes. The arm itself is unpinned (F3). F1 is a link one level up, which bullet 4 does not cover. |

## Decision D7: can the wider delete lose data?

| case | result | evidence |
| --- | --- | --- |
| a link at `cache/closer` (or at `cache`) to a folder that holds other things | **deletes every team-named folder there** | F1, reproduced |
| another teamwire home shares the cache folder through a link | **deletes the live caches of the other home's teams** | F1, same mechanism |
| a team mid-launch in the same home | safe. Launch creates `teams/<name>` first (`crates/teamwire-cli/src/cmd/launch.rs:113`, `Store::open` with `create_dir_all`, `teamwire-store/src/store.rs:144-150`). No closer, and so no cache, exists before that. | code read |
| another process closing another team at the same time | no data loss. Both closes can try the same left-over cache, and one reports a false "could not be deleted" (F5). A live pid's folder in `deleting/` is skipped (`delete.rs:145-147`). | reasoned, not reproduced |
| a team whose state lives elsewhere | not possible within one home. The state path is always `<home>/teams/<team>` (`teamwire-store/src/store.rs:113-115`). Across homes, see the shared-cache case. | code read |
| a link at `cache/closer/<team>` | removes the link only | tests :69 and :93, M2b, M3 |
| a dangling link at `teams/<team>` | the cache is kept, because `symlink_metadata` succeeds on a link | `delete.rs:60-61` |
| an entry whose name is not a team name, such as `Not-a-team` | left alone | test :134 and :148-151 |
| a regular file named like a team | moved into `deleting/` and stuck there | F4, reproduced |
| reopen of a closed team | reopen needs `teams/<team>` to exist, so reopen cannot race the sweep | `crates/teamwire-launch/src/reopen.rs:256` |

## Contradictions between members

1. Review verdict against commit 024f865. See F2. The reviewer read d620ed0, and the impl's
   fix round widened the delete afterwards. Nobody reviewed the wider delete.
2. Review "caches left by closes before this change stay; the task did not ask for a sweep"
   against impl's sweep. Decision D7 settled this in favour of the sweep. I list it only
   because the review text now reads wrong.
3. Review and impl disagree on which code keeps the link-to-nothing test green. Both are
   partly wrong (F3).

## Known limits: `shortcut:` comments

The team's branch adds no `shortcut:` comment (`git diff f1ac22c 024f865 | grep shortcut:`
finds none). The only one in the crate, `crates/teamwire-close/src/empty.rs:56`, was already on
the base.

## Claimed but not checked by me

- Impl's count "57 test binaries ok" for the `test-changed.sh` packages. I did not count that
  subset. My `--no-fail-fast` run over the whole workspace, a superset, has one failure only.
- Decision D8, a cache folder on another filesystem. I made no second filesystem.

## Progress
- read brief, task, decisions D1-D8, reports/impl.md, reports/review.md
- made the clone at $TEAMWIRE_HOME/cache/closer/closecache/clone
- teamwire verify closecache: exit 0, clean
- head 024f865: close crate tests exit 0; CLI close test exit 0; fast gate exit 0; full gate exit 1 (tmux test); test-changed exit 101 (tmux test); workspace --no-fail-fast: 1 failure only; doc and check-docs exit 0
- D6: tmux test exit 101 on base and head inside tmux, exit 0 on both outside tmux
- revert checks M1-M8 and base run done (table above)
- D7 probes: link at cache/closer deletes user folders (F1); a file named like a team gets stuck (F4)
- clone reset to a clean 024f865

## Decisions I made

- The full gate stopped at step 4. I ran steps 5 and 6 by hand and the workspace tests with
  `--no-fail-fast`, so this report can say whether anything else fails.
- I ran the tmux test with no tmux parent, using a double-forked subshell, as extra evidence
  for D6. The brief asked only for the in-session runs, and both are recorded.
- The fixes are spread over several commits, so I used targeted source changes (M1 to M8)
  instead of reverting whole commits. M5 and M6 do restore whole files from a commit.
- I graded F1 as wrong conclusion although it needs a hand-made link. The deleted files are
  user data outside teamwire's folders, and nothing can recover them.
- I counted one run on 024f865 for both the implementer's branch and the joined head,
  because the two branches are the same commit.
- I left the clone in place at a clean 024f865, as the closer preset says.
