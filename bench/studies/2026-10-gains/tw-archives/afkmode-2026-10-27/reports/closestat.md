- 09933e1 close refuses an open AFK team without a full afk/REPORT.md (unit 1) | left: units 2 (status view+zod) and 3 (teams key+zod)
- c92ec32 StatusView.afk (AfkView), the afk: status line, ui zod AfkView + StatusView.afk (unit 2) | left: unit 3 (teams key + marker + zod), the full check, report
- f87ba47 TeamEntry.afk_budget_ends_at, the (open, afk) teams marker, ui zod TeamEntry key (unit 3) | left: full check, report

## Result: done

Check, run on my last commit (afkmode/closestat):
- `cargo test -p teamwire-close -p teamwire-watch -p teamwire-msg -p teamwire-mcp -p teamwire-cli --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux`: exit 0, 576 passed, 0 failed.
- `cd ui && bun test`: 166 pass, 3 skip (live tests), 0 fail. `bun run typecheck`: clean.
- `scripts/gate.sh fast`: exit 0 (fmt ok, tidy ok, clippy ok). Tidy prints `WEAKENED crates/teamwire-cli/src/cmd/afk.rs:14`; that file is from the lead's base commit (an `#[expect]` in the afk start stub), not mine, and the gate still exits 0.

New `pub` items:
- `teamwire_watch::AfkView` (fields `budget_ends_at`, `seconds_left`, `spent`, `decisions`)
- `teamwire_watch::StatusView.afk: Option<AfkView>`
- `teamwire_msg::TeamEntry.afk_budget_ends_at: Option<Timestamp>`
- Test support only: `Rig::afk()` (close tests), `Bed::afk(ends)` (watch fixture).
- ui: `AfkView` zod schema; `StatusView.afk` and `TeamEntry.afk_budget_ends_at` keys.
Crate-private: `teamwire_close::afk_report::refusal`, `teamwire_watch::afk::view`.

Tests: all names of section 10 exist. The two guards pass on base behaviour by construction. The CLI close test creates an AFK team straight in the store (no launch), since the refusal needs no members.

Decisions I made:
- The status line prints the timestamp with `Timestamp`'s Display, so `2026-10-28T02:41:00.000Z` (with milliseconds), the same string as `--json`. The brief's example omits the `.000`.
- The report check sits in `team_plan` inside the `team.state == Open` block, after the member refusals, so the dry-run "check" step counts it as a refusal. It is not re-checked under the close lock (the report is a file the lead writes before closing).
- A heading's number strip needs at least one digit before `.`/`)`; `## .Verified` is not stripped.
- `decisions` is a `u32` saturating at `u32::MAX`.
- Test-support: `Rig::build` and `Bed::build` got an extra parameter; the AFK end in the close rig is 2099-01-01.

- a1236c0 close names an unreadable AFK report "cannot be read" not "missing"; the full-report archive test now first sees a refusal (review findings 2 and 1) | left: nothing

## Review round (revclosestat.md)

- Finding 1 (low) holds. `close_of_an_afk_team_with_a_full_report_archives_the_afk_files` passed on afk/base. Fix: the test now first writes an incomplete report and expects one refusal and an open team, then writes the full report and expects the close and the archive. It fails without the check. The test name stays as the brief fixed it.
- Finding 2 (low) holds. `afk_report::refusal` called any read error "is missing". Now only `NotFound` says that; any other error says `afk_report_incomplete: <path> cannot be read: <error>`. New test `close_says_an_unreadable_afk_report_is_unreadable_not_missing` (a REPORT.md of invalid UTF-8) fails on the old code. D17's two texts are unchanged for the missing and incomplete cases.
- No `--json` output changed in this round, so no zod change.

Check on a1236c0: cargo test of the five crates exit 0, 577 passed, 0 failed; bun test 166 pass, 0 fail; typecheck clean; `scripts/gate.sh fast` exit 0.

Decisions I made (this round): the new refusal text for an unreadable report (`cannot be read: <io error>`) is mine; D17 does not define it.

- 8158ee6 close needs no report for an AFK team that never had a member (D39); tests with refusals now add a member | left: nothing

## D39 round

- `afk_report::refusal` now takes the team's members (removed ones included, as `plan.rs` reads them) and returns no refusal when there is none.
- New tests in `crates/teamwire-close/tests/afk_report.rs`: `an_afk_team_that_never_had_a_member_closes_without_a_report` (fails on the old code: it refused) and `an_afk_team_whose_only_member_was_removed_still_needs_a_report` (pins the "removed counts" rule).
- The four existing refusal tests and the CLI test `close_of_an_afk_team_without_a_report_prints_the_refusal_code` now add one member first; their assertions are unchanged.
- No `--json` output changed; no docs are mine (the skill member documents it). Only the module comment of `afk_report.rs` changed.
- Check on 8158ee6: cargo test of the five crates exit 0, 579 passed, 0 failed; bun test 0 fail; typecheck clean; `gate.sh fast` exit 0.

Decisions I made (this round): "never had a member" is judged from the member rows `plan.rs` already reads (`all_or_corrupt`), so a damaged member row makes the plan refuse before this check, as before.
