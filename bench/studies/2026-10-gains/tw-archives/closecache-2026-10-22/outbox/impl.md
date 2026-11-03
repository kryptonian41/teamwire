<!-- teamwire:generated completion=01M5J1AX74J6QYP887DTGQFNA6 -->
# Result from impl (done)

- Team: closecache
- Status: done
- Generation: 3
- Completion: 3
- Time: 2026-10-22T14:48:10.212Z
- Final message from: hook
- Member report: /private/tmp/claude-501/bnruns/bngcwrebut/twhome/teams/closecache/reports/impl.md

## Summary

fix round 2: cf45e6a runs the sweep of other teams' closer caches only when cache and cache/closer are real folders (D9); own cache still deleted; 2 new tests fail without each half of the check. Crate tests, cli close test, gate fast pass. See reports/impl.md

## Task (first 300 characters)

# Brief: impl — close deletes the closer's cache folder ## 1. Who you are You are the implementer (kind implementer) for team `closecache`. You own the whole change. Other members, added after you finish: `review` (reviewer) reads your finished diff, then `closer` (closer) re-runs your checks and r…

## Final message

Fix round 2 is done in commit cf45e6a. The closer's finding F1 is fixed: a close can no longer delete user folders through a symbolic link at `cache` or `cache/closer`.

**What changed:**
- The cleanup of other teams' left-over caches now runs only when `<home>/cache` and `<home>/cache/closer` are both real folders. When either one is a symbolic link, that cleanup does nothing, as decision D9 asks. The dry-run plan uses the same check, so it no longer lists those folders either.
- The closed team's own cache is deleted by its own path first, the same way as before the cleanup was added. It still goes when `cache/closer` is a link.
- `docs/api/cli.md` now states the real-folder rule.

**Tests:** two new tests, one for each place a link can sit.
- `a_linked_closer_folder_loses_only_the_closed_teams_cache` is the closer's probe, adapted. It failed on the old code, where the plan named `bravo` and `photos` for deletion.
- `a_linked_cache_folder_loses_only_the_closed_teams_cache` covers a link at `cache`.
- I removed each half of the new check in turn and ran the tests. Each time, the matching test failed.

**Checks on cf45e6a:** `cargo test -p teamwire-close --locked`, `cargo test -p teamwire-cli --locked --test close` and `TEAMWIRE_BASE=f1ac22cb scripts/gate.sh fast` all pass. I did not rerun `scripts/test-changed.sh`, because its tmux failure is decision D6 and the closer checks it.

**Not done, outside this round:** finding F4 remains. A plain file named like a team under a real `cache/closer/` still gets stuck in `deleting/`. The fix is one filter line plus a test, and the report leaves it for you to route. F3 and F5 are answered in the report and need no code change.

The full details are in `/private/tmp/claude-501/bnruns/bngcwrebut/twhome/teams/closecache/reports/impl.md`.
