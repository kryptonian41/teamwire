# Brief: closestat, close and status for AFK teams

## 1. Who you are

You are `closestat`, an implementer in team `afkmode`. Nobody will answer a question from the user
during this run; the lead answers questions, and every open detail below is already decided.

The other members of wave 1, each on its own branch off `afk/base`:

| Role | Kind | Owns (summary) | Who uses its work |
| --- | --- | --- | --- |
| start | implementer | `teamwire afk start`, doctor's launch checks, the launch crate's AFK start | the lead, the skill |
| budget | implementer | launch's budget refusal and model warnings | launch |
| stophold | implementer | the AFK hold in the lead's Stop hook | the lead |
| closestat (you) | implementer | close's report check, the status view and line, the teams key and marker, the zod copy | the lead, the reporter, the dashboard |
| skill | implementer | the AFK skill, its install, every doc (it documents your refusal, line and keys) | the lead |

After you: a reviewer reads your branch, then a closer runs every check on the joined branch, then a
reporter writes the run's report. The exact files each member owns are in the plan.

## 2. The end state

An AFK run's team is a team whose row has `afk_budget_ends_at` set. Design section 5.5 is the
behaviour; D17 to D20, D30 and D35 fix its details.

1. **Close** (D17). A whole-team close of an open AFK team refuses when
   `<home>/teams/<team>/afk/REPORT.md` is missing or lacks one of these eight headings, named exactly:
   What you asked; What I understood; The result; Verified; Decisions made for you; Not done, and
   why; Risks; Cost. A heading is a line of one to six `#` followed by a space; after the `#`s, a
   leading `N.` or `N)`, and `**` are stripped, its text must start with the name, compared without
   case. Order is not checked. Refusal texts, as plan refusals so that `--dry-run`, `close` and the
   `close.refused` event all carry them:

   ```
   afk_report_incomplete: /home/teams/demo/afk/REPORT.md is missing
   afk_report_incomplete: /home/teams/demo/afk/REPORT.md lacks the headings: Verified, Cost
   ```

   The CLI already prints a refusal as `REFUSED: <text>` and exits 1; keep that. `close --role`,
   `--clear-stale-lock`, a team already `closing` or `closed`, and an ordinary team are unaffected.
   No flag overrides the refusal. A full report passes, and the archive keeps the `afk/` files with
   no change to `archive.rs`.
2. **Status** (D18). `teamwire_watch::StatusView` gains `pub afk: Option<AfkView>`, `None` for an
   ordinary team and serialized as `"afk": null`. `AfkView` has exactly `budget_ends_at: Timestamp`,
   `seconds_left: u64` (`budget_ends_at.saturating_since(now)` in seconds, zero once spent),
   `spent: bool` (now at or past the end) and `decisions: u32` (the lines of
   `<team dir>/afk/decisions.md` that match `^\s*D[0-9]+\s`, so an entry indented by white space
   counts too (D35); a missing file is 0; another read error is 0 plus a line in `errors`). `teamwire status <team>` prints one line after
   the watch line, with `<time>` from status.rs's `minutes()`:

   ```
   afk: budget ends 2026-10-28T02:41:00Z (7h59m left), 3 decisions
   afk: budget spent at 2026-10-28T02:41:00Z, 3 decisions
   ```

   `status --json` (and so the MCP `team_status`) writes the same field names. The all-teams summary
   line is unchanged.
3. **Teams** (D19). `teamwire_msg::TeamEntry` gains `pub afk_budget_ends_at: Option<Timestamp>`,
   filled in `describe()`, so `teams --json` and the MCP `team_teams` give a timestamp string for an
   AFK team and `null` otherwise. The text line of an AFK team reads
   `<name> (<state>, afk) <title>: address as lead@<name> or <role>@<name>`; an ordinary team's line
   is unchanged.
4. **Dashboard schema** (D20). In `ui/src/shared/teamwire.ts`: `AfkView = z.object({budget_ends_at:
   z.string(), seconds_left: count, spent: z.boolean(), decisions: count})`; `StatusView` gets
   `afk: AfkView.nullable().optional()`; `TeamEntry` gets
   `afk_budget_ends_at: z.string().nullable().optional()`. Both new output and older output without
   the keys parse. `fixtures/` stays unchanged. ui/AGENTS.md says `src/shared/` changes only when the
   lead asks: this brief is that request, for these two files only.

## 3. Done when

- Every test named in section 10 exists with that exact name and passes. Each test of new
  behaviour fails without its change; a test marked (guard) pins today's behaviour and passes on
  afk/base too (D33).
- Every commit that changes `status --json` or `teams --json` changes `ui/src/shared/teamwire.ts`
  and its test in the same commit (D30).
- The check in section 12 passes on your last commit, with its real output in your report.
- Your report lists every new `pub` item.

## 4. Out of scope

- Dashboard views beyond the schema copy; `ui/fixtures/`, `ui/src/web/`, `ui/src/server/`.
- `teamwire afk start`, launch, the Stop hook, the skill and the docs.
- A flag to override the report check; any change to `archive.rs` or to the `empty.rs` close path.

## 5. When to do nothing

If `TeamRow` on your branch has no `afk_budget_ends_at` field, change nothing, say so in your report,
and finish with status `blocked`.

## 6. Where you are

Your worktree is `/private/tmp/claude-501/bnruns/bn4tu2e1us/clone/.worktrees/` plus your slug, on
branch `afkmode/closestat`, cut from `afk/base` (5b86c1e plus the lead's afk-arguments commit). Setup
already ran `cargo build --workspace --all-targets --locked` and `bun install --frozen-lockfile` in
`ui/`, so run no install command.

## 7. Read first, in this order

1. `/private/tmp/claude-501/bnruns/bn4tu2e1us/twhome/teams/afkmode/afk/plan.md`
2. `/private/tmp/claude-501/bnruns/bn4tu2e1us/twhome/teams/afkmode/afk/decisions.md`, D6, D17 to D20,
   D23, D30, D33, D35.
3. `docs/design/2026-10-19-afk-mode.md`, sections 4, 5.1, 5.5 and section 6's "7. Report and close".
4. `AGENTS.md`, `ui/AGENTS.md`, then `docs/conventions.md` section 6.
5. The code: `crates/teamwire-close/src/lib.rs` and `plan.rs` (`plan_close_with`, `team_plan`),
   `crates/teamwire-watch/src/lib.rs` (`StatusView`) and `status.rs` (`read_rows`, `status_with`),
   `crates/teamwire-cli/src/cmd/status.rs` (`render`, `minutes`), `cmd/teams.rs`,
   `crates/teamwire-msg/src/directory.rs` (`TeamEntry`, `describe`), `ui/src/shared/teamwire.ts` and
   its test.

## 8. Decisions already made (2026-10-27; full text in decisions.md)

- The field names of `AfkView`, `StatusView.afk` and the `afk_budget_ends_at` key are fixed by the
  task; the dashboard and the docs rely on them.
- D17, D18, D19, D20, D35: as in section 2, with the worked examples there.
- D30: AGENTS.md requires a change to `--json` output and its copy in `ui/src/shared/teamwire.ts`
  in the same commit. Units 2 and 3 below each carry their own zod change.
- D33: a test marked (guard) pins today's behaviour; it need not fail without your change.
- D23: `crates/teamwire-cli/tests/tmux_wiring.rs::herdr_env_wins_over_tmux` fails on afk/base inside
  a tmux pane, before any change. Your check skips it by name. Do not edit or skip it any other way.
- An AFK team in a test: `Tx::create_afk_team(at, &new_team, ends, &serde_json::json!({}))` in place
  of `create_team`. Tests through the binary use the real clock, so pick ends far in the past or the
  future.

## 9. What already exists

- `Tx::create_afk_team`, `TeamRow.afk_budget_ends_at` (teamwire-store). Do not change the store.
- `ClosePlan.refusals`, `record_refusal`, the CLI's `REFUSED:` output; `archive.rs` copies every file
  of the team directory.
- `StatusView` derives `Default`; the CLI builds it with `..StatusView::default()`.
  `crates/teamwire-watch/src/status.rs` is 359 lines; put the AFK view in
  `crates/teamwire-watch/src/afk.rs` to stay under tidy's 400-line report.
- `count` in `ui/src/shared/teamwire.ts` (a non-negative integer).

## 10. Units in order, commit after each

Use the tdd skill for each unit; test only at these seams: `plan_close_with` and `close` with the
close tests' rig on a team directory with and without `afk/REPORT.md`; `status_with` with the watch
tests' fixture and `FakeClock`; the `teamwire` binary for `status`, `teams` and `close`; and zod's
`safeParse` in `ui/src/shared/teamwire.test.ts`. If a check fails and the cause is not clear from its
output, use the diagnosing-bugs skill before your second fix attempt.

1. Close's report check: `crates/teamwire-close/src/afk_report.rs`, its call in `plan.rs`. Tests in
   `crates/teamwire-close/tests/afk_report.rs`:
   - `close_refuses_an_afk_team_without_its_report`
   - `close_refuses_an_afk_report_that_lacks_a_heading_and_names_it`
   - `close_of_an_afk_team_with_a_full_report_archives_the_afk_files` (the archive holds
     `afk/REPORT.md` and `afk/task.md`)
   - `removing_one_member_of_an_afk_team_needs_no_report` (guard)
   - `an_ordinary_team_closes_without_a_report` (guard)
   And in `crates/teamwire-cli/tests/close.rs`:
   `close_of_an_afk_team_without_a_report_prints_the_refusal_code` (stderr holds
   `REFUSED: afk_report_incomplete:`, exit 1).
2. The status view, the status line, and their zod copy, in one commit (D30). Tests in
   `crates/teamwire-watch/tests/status.rs`:
   - `an_afk_team_shows_its_budget_and_decision_count` (a decisions.md with `D1` at column 0, `D2`
     indented by four spaces, and a line that only mentions D3 mid-sentence counts 2)
   - `a_spent_budget_shows_zero_seconds_left`
   - `an_ordinary_team_has_no_afk_view`
   In `crates/teamwire-cli/tests/status.rs`:
   `status_of_an_afk_team_prints_the_afk_line_and_json_object`.
   In `ui/src/shared/teamwire.ts`: `AfkView` and `StatusView.afk`. In
   `ui/src/shared/teamwire.test.ts`, a `describe("afk fields", ...)` block with tests for a status
   view with an `afk` object, with `"afk": null`, and without the key.
3. The teams key, the `(open, afk)` marker, and their zod copy, in one commit (D30). Test in
   `crates/teamwire-cli/tests/teams.rs`:
   `teams_marks_an_afk_team_and_its_json_gives_the_budget_end` (and an ordinary team's key is null).
   In `ui/src/shared/teamwire.ts`: `TeamEntry.afk_budget_ends_at`. In the same `afk fields` block,
   tests for a teams entry with and without `afk_budget_ends_at`.

## 11. Files you own, and files owned by others

You own: `crates/teamwire-close/src/afk_report.rs`, `crates/teamwire-close/src/plan.rs`,
`crates/teamwire-close/src/lib.rs`, `crates/teamwire-close/tests/afk_report.rs`,
`crates/teamwire-close/tests/support/rig.rs`, `crates/teamwire-watch/src/lib.rs`,
`crates/teamwire-watch/src/status.rs`, `crates/teamwire-watch/src/afk.rs`,
`crates/teamwire-watch/tests/status.rs`, `crates/teamwire-watch/tests/support/fixture.rs`,
`crates/teamwire-msg/src/directory.rs`, `crates/teamwire-cli/src/cmd/status.rs`,
`crates/teamwire-cli/src/cmd/teams.rs`, `crates/teamwire-cli/tests/status.rs`,
`crates/teamwire-cli/tests/teams.rs`, `crates/teamwire-cli/tests/close.rs`,
`ui/src/shared/teamwire.ts`, `ui/src/shared/teamwire.test.ts`.

Owned by others, never edit: every other file under `crates/teamwire-cli/` (start, stophold, skill);
`crates/teamwire-launch/`, `crates/teamwire-report/`, `crates/teamwire-harness-claude/`,
`crates/teamwire-mcp/`, `examples/`, `docs/`, `README.md` (other members or nobody); `ui/fixtures/`
and the rest of `ui/`; `crates/teamwire-store/`, `tests/fakes/`, `Cargo.toml`, `Cargo.lock`,
`ui/bun.lock`, `scripts/`, `xtask/` (protected or the lead's). For a change in any of them, send the
lead a question and carry on.

## 12. Verification

Your check, from the worktree root:

```bash
cargo test -p teamwire-close -p teamwire-watch -p teamwire-msg -p teamwire-mcp -p teamwire-cli --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux && (cd ui && bun test && bun run typecheck) && scripts/gate.sh fast
```

On afk/base it passes (562 Rust tests, 162 bun tests, typecheck, then fmt, tidy and clippy ok). Do
not run the full suite, `scripts/test-changed.sh` or `scripts/gate.sh` without `fast`.

## 13. How to finish

Do not push and do not open a pull request. Commit each unit. Write your report with one progress
line per commit, the check's real output, every new `pub` item, and a **Decisions I made** list.
Then run `teamwire done` (or `team_done`).

## 14. Open questions you must not answer

None. Decide anything the brief does not cover, and list it under **Decisions I made**.
