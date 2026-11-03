# review report: close deletes the closer cache

Diff reviewed: `git diff f1ac22c...closecache/impl` (5 commits, 2b9c5e5..d620ed0).
I read it along with the code it calls: `delete.rs`, `team_close.rs`, `plan.rs`, `empty.rs`, the
team name rules in `teamwire-core/src/names.rs`, and the store open in `teamwire-cli/src/cmd/close.rs`.

## Verdict

The diff is sound. The diff meets all four end-state bullets in the task. Close cannot delete anything
outside the folder that `<home>/cache/closer/<team>` resolves to. The cache stays whenever the
team state stays. I found no stubbed or hard-coded code, and the diff breaks no hard rule.
Two low-severity gaps remain. In both, the cache stays behind when the task wants it gone. Neither
gap loses data.

## Findings, most harmful first

### 1. Low: a close killed between the two moves leaves the cache forever

- Where: `crates/teamwire-close/src/team_close.rs:409`, `crates/teamwire-close/src/delete.rs:30-32`
  and `delete.rs:62-63`.
- State: team `alpha` has a cache at `<home>/cache/closer/alpha` holding a `target/` folder. The
  close process is killed (Ctrl-C, `kill -9`, a machine sleep that ends the session) after
  `move_dir(team_dir, &doomed)` at `delete.rs:30` returns, and before `move_dir(&cache, ...)` at
  `delete.rs:63` runs. The window covers the `remove_dir_all` of the team folder at `delete.rs:32`. It
  also covers the sweep of left-over folders at `delete.rs:90`, and that sweep can take seconds when
  an earlier killed close left a big cache in `deleting/`.
- Wrong result: `teams/alpha` is gone and `cache/closer/alpha` is still there. A rerun of
  `teamwire close alpha` cannot reach the cache, because the CLI opens the store with
  `Store::open_existing` (`crates/teamwire-cli/src/cmd/close.rs:55`), and that fails once the team
  folder is gone. The left-over sweep `remove_left_over` looks only in `<home>/deleting/`. So nothing
  ever deletes the cache. That is the task's original bug, inside this window.
- Note: decision D4 requires the cache to go after the state, so the order in the diff is right.
  The gap is that nothing cleans up a cache whose team no longer exists. The comment at
  `delete.rs:48` says "the same kill-safe way as the team directory". That holds for the cache
  folder alone, which is either whole in place or whole in `deleting/`. It does not hold for the
  pair of team state and cache.

### 2. Low: a `cache` link to another filesystem means the cache is never deleted

- Where: `crates/teamwire-close/src/delete.rs:62-63`.
- State: `<home>/cache`, or `<home>/cache/closer`, is a symbolic link to a folder on another volume.
  A user who wants the big `target/` folders off the main disk would set this up. The final path
  component `cache/closer/<team>` is a real folder.
- Wrong result: `symlink_metadata` follows the links in the middle of the path, so the folder takes
  the `Ok(_)` branch. Then `rename` from the other volume into `<home>/deleting/` fails with
  `EXDEV`. Every close reports `the closer cache ... could not be deleted: cannot move it:
  Cross-device link (os error 18)`. The step stays ok, and the cache is never deleted. Bullet 1 of
  the task fails for that setup.
- Close does report the failure in the step detail and in a `tracing::warn`, so the failure is
  visible. I worked this out from the rule that `rename(2)` returns `EXDEV` across filesystems. I did
  not reproduce it, because a reproduction needs a second mounted volume, outside the temporary
  folders I may write to.

## Answers to the brief's questions

### Can close delete anything outside `<home>/cache/closer/<team>`?

No. I checked each case:

| case | what close does | verdict |
| --- | --- | --- |
| team name that forms a path | `TeamName` is `^[a-z][a-z0-9]{0,11}$` (`teamwire-core/src/names.rs:26`). Every read parses it, including the store's team row (`teamwire-store/src/team.rs:80`). Names cannot hold `/`, `.`, or `..`. | safe |
| link at the team folder `cache/closer/<team>` | `symlink_metadata` sees the link, and `remove_file` removes only the link (`delete.rs:59-60`). A link to nothing is removed too. | safe |
| link inside the folder | The folder is renamed, then removed with `std::fs::remove_dir_all`. On toolchain 1.94.1, that call never follows a link inside the tree. | safe |
| link at `cache` or `cache/closer`, same filesystem | Close follows the link and deletes `<target>/closer/<team>` or `<target>/<team>`. That is the folder the closer's own path resolves to, so close deletes the cache and nothing else. | safe |
| link at `cache` or `cache/closer`, other filesystem | The cache stays. See finding 2. | not deleted |
| the folder is swapped for a link between the check and the rename | `rename` moves the link itself. Then `remove_dir_all` on a link removes only the link. | safe |
| a left-over folder name in `deleting/` | The new name is `<team>.closer.<pid>`. Team names hold no dot, so this name can never equal a team's `<team>.<pid>`. The sweep still reads the pid after the last dot. | safe |

### Does the cache stay when the team state stays?

Yes, in every path I traced through `team_close.rs`:

- With `keep_state`, `delete_state` returns at line 395-399, before any delete.
- With no archive, or a failed final check of the archive, the step returns at lines 403-408.
- When the team folder fails to move, `delete_team_dir` returns `Err`, and the `.map` at line 409
  never runs.
- When an earlier step fails, `locked_steps` returns false, `give_up` runs, and `delete_state`
  never runs (lines 185-189).
- A member close goes through `remove.rs`, which the diff does not touch (D5).

The empty-store close path (`empty.rs`) deletes a team folder and leaves the cache. The
implementer recorded that choice. No closer can have run for a store with no member row, so I do
not count it as a finding.

### Does each new test fail without its fix?

| test | fails on base f1ac22c | fails when only its own fix is reverted |
| --- | --- | --- |
| `a_team_close_deletes_the_closer_cache_and_no_other_team_cache` | yes: the cache is left | yes |
| `keep_state_keeps_the_closer_cache_until_the_state_goes` | yes: the second half needs the rerun to delete the cache | yes. The first half also catches a change that deletes the cache even with `keep_state`. |
| `a_closer_cache_that_is_a_link_loses_the_link_only` | yes: base leaves the link | **no**, see below |
| `a_closer_cache_link_to_nothing_is_removed_too` | yes | yes: `Path::exists` is false for a link to nothing |
| `the_plan_names_the_closer_cache_only_when_it_exists` | yes | yes |

The implementer already disclosed the "no" in that table. I confirmed it with a separate program
that replays the first commit's code: `exists()`, then `rename`, then `remove_dir_all`. That
program ran on a link to a real folder:

```
link to folder: renamed and removed; link left = false; keep.txt left = true
link to nothing: skipped: exists() is false; link left = true
```

So `rename` and `remove_dir_all` from the standard library already protect the link target. The
`is_symlink` branch at `delete.rs:59` is what makes the link-to-nothing test pass. The
link-to-folder test still has value: it would catch a delete that walks the folder with `read_dir`.
The closer should expect this result when it reverts commit 30cb7f7. The program is at
`/private/tmp/claude-501/-private-tmp-claude-501-bnruns-bngcwrebut-clone/1e3a25a5-33fe-45fd-9c57-8ba8072c2195/scratchpad/linkexp/main.rs`.

### Stubbed, empty or hard-coded code

I found none. The plan detail is built from the real path (`plan.rs:115-125`), and the tests compare
it with the path built by the test rig. Every test checks the filesystem, and none checks only a
returned string.

### AGENTS.md hard rules

| rule | result |
| --- | --- |
| no `unwrap`, `expect`, `panic`, `todo` outside tests | none in the changed `src/` lines |
| time from `Clock` | the diff reads no time |
| no new dependency, `Cargo.lock` unchanged | `tracing` and `tempfile` were already dependencies; no Cargo file changed |
| no protected file | `docs/api/cli.md` is not protected; no other file outside the crate changed |
| no added `#[expect(` | none added; d620ed0 removed the one an earlier commit added |
| files under 400 lines | `team_close.rs` grew from 427 to 432 lines. Base was already over 400, so the diff did not cross the limit. Tidy fails only past 800. `delete.rs` has 131 lines and `plan.rs` has 263. |
| `--json` shape unchanged (D2) | only the free-text `detail` string changes; nothing in `ui/src` or the CLI parses it |

The new text in `docs/api/cli.md` matches the code: the `deleting/<team>.closer.<pid>` name, the
link rule, the note-not-stop rule, and member close leaving the cache alone.

## Checks I ran

| command (in `.worktrees/closecache-impl` at d620ed0) | result |
| --- | --- |
| `cargo test -p teamwire-close --locked --test closer_cache` | 5 passed, 0 failed |
| `cargo test -p teamwire-cli --locked --test close` | 16 passed, 0 failed |
| the link program above, built with rustc 1.94.1 | output shown above |

I did not run `scripts/gate.sh fast`, `scripts/test-changed.sh`, or the full `teamwire-close`
suite, because the closer re-runs those.

## Notes for the parent

- Nothing in this repository creates `<home>/cache/closer/<team>`. The closer preset
  (`examples/presets/closer.json:12`) and `crates/teamwire-launch/contracts/kind_closer.md` tell
  the closer to clone into `mktemp -d`. The task states that the closer's clone lives at
  `cache/closer/<team>`. So the new delete only fires when something outside this repository makes
  that folder. Examples are a closer brief that names that path, or the user's own setup. I did not
  treat this as a finding, because the task defines the path.
- Caches left by closes that ran before this change stay. The task did not ask for a sweep.

## Decisions I made

- I ranked both findings low. Neither loses data. Each needs a rare state: a kill in a short
  window, or a `cache` link to another volume.
- I did not count the 400-line rule as broken, because `team_close.rs` was already 427 lines on
  base. I listed it in the rules table instead.
- I did not count the empty-store path as a gap. The implementer recorded that choice, and no
  closer can have run for an empty store.
- I worked out the cross-filesystem case from `rename(2)` and did not reproduce it. A reproduction
  needs a mounted volume outside the temporary folders I may write to.
- I ran the two test commands the implementer claimed, as a check on those claims. I left the gate
  and `test-changed.sh` to the closer.
