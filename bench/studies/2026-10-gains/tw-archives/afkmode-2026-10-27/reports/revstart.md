# Review: branch afkmode/start (`teamwire afk start`)

Reviewer: revstart. Diff: `git diff afk/base...afkmode/start`, three commits (0d2db29, d2ad30c, e026e22), 11 files.

## Summary

The branch is sound. I found no correctness, security or data-loss defect in it, and nothing to send back to the implementer.

- Every refusal code, the refusal order, the five success lines and the exit-2 line match D9 to D12 and the brief's section 2.
- All 13 test names from the brief's section 10 exist with those exact names.
- Each test of new behaviour fails without its change. I traced each one by hand.
- The diff touches only files the start member owns. No hard rule of AGENTS.md is broken.
- I ran the real binary from the branch against a scratch home. `afk start` created a team with no members. Every reader I tried works on that team: status, teams, verify, metrics, reopen, close and watch.

Two notes for the lead follow. Neither is a defect in this branch. Each comes from a gap in the brief or a cross-branch effect that the plan already lists.

## Findings, ranked by harm

None that the implementer should fix.

## Notes for the lead (not defects of this branch)

1. **Low, Codex lead only, a gap in the brief.** `teamwire launch` refuses a new team with `lead_not_locatable` when the lead's harness cannot read watch lines and its pane cannot be found (`crates/teamwire-launch/src/validate.rs:98-100`, `lead_must_be_locatable`). Codex is such a harness (`crates/teamwire-harness-codex/src/lib.rs:105`, `reads_watch_lines: false`). `afk_check` does not run that rule (`crates/teamwire-launch/src/afk.rs:91-166`). `launch --add` also skips the rule, because it runs only when the team is new (`validate.rs:98`).
   - Failing case: a Codex lead runs outside its own pane, with `CODEX_THREAD_ID` set, on Linux, which has no sleep hold. The lead runs `teamwire afk start`, then `teamwire launch --add`. Both succeed. Member results are typed into a pane nobody can find, so they reach nobody. Plain `launch` would have refused this lead.
   - The brief's section 4 puts "A Codex lead" out of scope. Its refusal list in section 2.1 does not include `lead_not_locatable`. The implementer followed the brief, so the decision belongs to the lead.
2. **Info, cross-branch, already plan risk 5.** On the exit-2 path the task copy fails after the team row exists (`afk.rs:226-231`). The team stays open with no `afk/task.md` and no sleep hold. A second `afk start` from the same session is then refused with `afk_already_running`. On the joined branch, closestat's close also refuses that team until `afk/REPORT.md` has all eight headings (D17). The only way out is to write those headings by hand. The exit-2 message names the team, as D9 asks, but gives no next step.

## What I checked, and how

### Against the brief, the task, the design and the decisions

| Item | Where | Result |
| --- | --- | --- |
| Refusal order: no_lead_session, background_lead, afk_already_running, bad_name, title_missing, title_too_long, team_exists, bad_budget, task_unreadable, task_empty, then doctor_failed, then no_lead_process (D9) | `afk.rs:91-166` (`afk_check`); `cmd/afk.rs:70-84`; `afk.rs:197-204` | Matches |
| Refusal output `refused: <code>: <message>`, then `nothing was created`, on stderr, exit 1 | `cmd/afk.rs:25-29` | Matches |
| `doctor_failed: <check name>: <detail>`; a warn prints `warning: <name>: <detail>` and does not refuse | `cmd/afk.rs:76-84` | Matches |
| Zero, unparsable or overflowing budget is `bad_budget`, parsed with `parse_duration` made `pub(crate)` (D10) | `afk.rs:128-140`; `metrics.rs:75` | Matches. The only change to metrics.rs is the visibility. |
| Row from `member::new_team`, `create_afk_team(at, &row, budget_ends_at, &json!({"lead_cwd": cwd}))`, and `lead_cwd` from `member::lead_cwd` with the current directory as fallback (D11, D29) | `afk.rs:206-217` | Matches. member.rs is not edited. |
| Task copied byte for byte with `Platform::write_atomic` | `afk.rs:218-225` | Matches |
| Hold taken through `ensure_hold` and recorded on the row | `afk.rs:232-245` | Matches |
| The five stdout lines (D12) | `cmd/afk.rs:90-96` | Matches. The real binary printed `budget ends: 2026-10-27T16:22:58.218Z`. |
| Exit 2 prints `failed: <what>; team <team> exists` | `afk.rs:226-231`; `cmd/afk.rs:100-103` | Matches |
| `launch_checks(inputs: &Inputs<'_>, pick: &HostPick) -> Vec<Check>`: host FAIL when the host is none, herdr hook only for herdr, `.worktrees ignored`, the binaries and their probes, and the lead setup. One `inputs` helper is shared by `run` and the afk command (D8). | `doctor.rs:666-725` | Matches. `doctor` itself still builds the same checks in the same order. |
| `--help` shows the four flags, and the stub text is gone | binary on the branch | Matches |

### Tests

- All 13 names from section 10 exist: 6 in `crates/teamwire-launch/tests/afk_start.rs`, 1 in `crates/teamwire-cli/tests/doctor.rs`, and 6 in `crates/teamwire-cli/tests/afk.rs`.
- I traced whether each test fails without its change:
  - `afk_start_refuses_a_taken_team_name`: without the team_exists check, the rig has no lead process, so the result would be `no_lead_process`. The test would fail.
  - `afk_start_with_no_lead_process_refuses_and_creates_nothing`: without the early check, the run would succeed with a warning. The test would fail.
  - `afk_start_on_a_platform_without_a_hold_creates_the_team_and_warns`: without the `sleep_hold().is_some()` condition, the run would refuse with `no_lead_process`. The test would fail.
  - `afk_start_refuses_a_session_that_already_leads_an_open_afk_team`: without the check, the second run would succeed. The test would fail.
  - The CLI refusal tests: the old stub on afk/base does not print `refused: <code>:` with these codes, so they would fail there.
  - `afk_start_refuses_when_a_doctor_check_fails`: without doctor, the run would succeed and exit 0. The test would fail.
  - `launch_checks_fail_on_host_none_and_keep_the_plugin_check`: `launch_checks` does not exist on afk/base, so this test does not compile there.
- `add_to_an_afk_team_with_no_members_starts_the_member` is a guard (D33). It passes because the only validate.rs changes are refactors that keep behaviour. `team_title` still checks only a new team, and `title_problem` keeps launch's codes and messages.
- No test was deleted, ignored or loosened. The doctor bed's setup moved into `tests/support/healthy.rs` with the same steps.

### The check output

I did not rerun the check, because cargo would write into the implementer's worktree. The verify log `/private/tmp/claude-501/bnruns/bn4tu2e1us/twhome/teams/afkmode/verify/start.log` shows 42 test binaries, 419 tests passed and 0 failed, then `gate 1/3 fmt: ok`, `gate 2/3 tidy: ok` and `gate 3/3 clippy: ok`. Tidy prints `WEAKENED` six times. Every one of them is a new `#[expect(` lint attribute with a reason, in new files. None marks a weakened test.

### A team with no members, through the real binary

I copied the branch's built binary (`.worktrees/afkmode-start/target/debug/teamwire`, at e026e22) into my scratchpad. I gave it a scratch home, a scratch Claude directory set up by `teamwire install`, and the fake `claude` and `herdr` on PATH. A `sleep` process stood in as the lead, with a session file and `CLAUDE_CODE_SESSION_ID`. Nothing was run against `~/.claude` or the user's home.

| Command | Result |
| --- | --- |
| `doctor` | all ok, exit 0 |
| `afk start --team zero ... --budget 2h` | the five lines, exit 0; `caffeinate -ims -w <lead pid>` running |
| `afk start --team zero2 ...` from the same session | `refused: afk_already_running: ...`, then `nothing was created`, exit 1 |
| `status zero`, `status`, `status zero --json`, `teams`, `teams --json` | correct output with `members: []`, exit 0 |
| `verify zero` | `clean`, exit 0 |
| `metrics zero`, `metrics` | zero counts, exit 0 |
| `reopen zero --dry-run` | lead skipped as open, exit 0 |
| `watch zero` | `WATCHING 0 members` and keeps running. It does not report the team complete. |
| `close zero --dry-run`, then `close zero` | archive holds `afk/`, `team.db` and `SHA256SUMS`; `released sleep hold`; the team is closed, exit 0 |

The status, teams and close output above comes from afk/base code, because this branch does not change those commands. closestat's branch changes them, and its reviewer covers those changes.

### AGENTS.md hard rules

| Rule | Result |
| --- | --- |
| Store writes inside `Store::write`, with no external call inside | `afk.rs:212-217`. The process lookups and the hold start run outside the write. |
| Files written with `write_atomic` | `afk.rs:223` |
| Time from the Clock trait | `afk.rs:133`, `afk.rs:208` |
| Only teamwire-cli prints or exits | the launch library returns values and prints nothing |
| No unwrap, expect, panic or todo outside tests | none |
| Lints suppressed only with `#[expect(..., reason)]` | yes |
| Layering, no new dependencies, Cargo.lock and protected files untouched | yes, per `git diff --stat` |

Files touched: only files the brief's section 11 lists for start.

## Decisions I made

- I did not rerun the start member's check. Running it would write into the implementer's worktree's `target/`. I read the verify log instead and quote it above.
- I ran the branch's binary against a scratch home and a scratch Claude directory to test the empty-team readers. I used no unit test and no change to the repository. The scratch setup is in `/private/tmp/claude-501/-private-tmp-claude-501-bnruns-bn4tu2e1us-clone/6c1c2bad-6c97-49e5-b72d-584698c1582e/scratchpad/zero`. I stopped the stand-in lead process afterwards.
- I reported the Codex `lead_not_locatable` gap as a note for the lead, not as a finding for the implementer. The brief puts a Codex lead out of scope and lists the refusals exhaustively.

## Questions for the parent

None.
