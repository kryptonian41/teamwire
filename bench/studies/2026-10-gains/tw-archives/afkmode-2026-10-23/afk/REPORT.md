# AFK run report: afkmode ("Build AFK mode")

Written by the reporter on 2026-10-23 at about 19:00Z, after the closer finished.
The run's own files are in `$TEAMWIRE_HOME/teams/afkmode/afk/`, and each member's report is in `../reports/`.

## What you asked

> # Task: build AFK mode for teamwire
>
> teamwire needs AFK runs: a lead runs one team on one task while the user is away, with no questions to the user until a report is written.
>
> The design and build plan are in `docs/design/2026-10-19-afk-mode.md`. Read all of it first. Schema version 5 (`team.afk_budget_ends_at`) is already committed, as the plan's "Precondition" says.
>
> End state: everything that sections 5 and 6 of the design describe exists and is tested.
>
> - `teamwire afk start` (section 5.2).
> - The AFK hold in the lead's Stop hook (section 5.3).
> - The budget and the model table in launch (section 5.4).
> - Close and status for AFK teams (section 5.5).
> - The AFK skill with the agreed content of section 6, installed the way the other skills are (section 5.6).
>
> Use the command names, flags, refusal codes and messages that the design gives. Where the design leaves a detail open, decide it and say what you decided in your final message.
>
> The task also fixed these names: the extra refusal codes `no_lead_session`, `no_lead_process`, `afk_already_running` and `doctor_failed`; `afk_hold_notes(home, session, platform, clock) -> Vec<String>` in `teamwire-report`; a fourth `extra` parameter on the store's `create_afk_team`, with `{"lead_cwd": ...}` from `afk start`; `StatusView.afk: Option<AfkView>` with `budget_ends_at`, `seconds_left`, `spent` and `decisions`; and `afk_budget_ends_at` in `teams --json`, with ui zod schemas that accept both new and old output.
>
> How to work: add tests in each crate's `tests/` folder, check with `cargo test -p <crate> --locked`, `bun test` in `ui/` and `scripts/gate.sh fast`, commit, do not push. Setup: every member gets `"plugins": {"claude-mem@thedotmack": false}`, implementers run on sonnet, everyone else on opus.

The full text, word for word, is in `afk/task.md`.

## What I understood

The planner wrote the end state in `afk/understanding.md`. One branch off `main` (fb1fd8f) holds everything in design sections 5 and 6, with tests:

1. `teamwire afk start --team N --title T --task F [--budget 8h]` creates an AFK team with no members, copies the task file byte for byte into `afk/task.md`, takes the sleep hold, and records `lead_cwd` and the budget end in the `team.created` event.
2. `afk start` refuses and creates nothing on `no_lead_session`, `afk_already_running`, a bad or taken name, `doctor_failed` or `no_lead_process`.
3. `launch --add` works on an AFK team with no members. Past the budget, launch refuses each new implementer with `afk_budget_spent` and starts the other members. Launch warns once for each member below the AFK model table.
4. The lead's Stop hook holds the lead with the AFK note when nothing will wake it. Past the budget, the note starts with `budget spent at <time>:`.
5. `teamwire close` refuses an AFK team with `afk_report_incomplete` until `afk/REPORT.md` holds the eight headings. `close --role` and ordinary teams do not change.
6. `status` prints one AFK line, and `teams` marks AFK teams. The JSON output and the ui schemas carry the new fields.
7. The AFK skill keeps every rule of design section 6. Install writes it into the Claude plugin, and doctor checks it.
8. Each changed crate's tests, `bun test`, the ui typecheck and `scripts/gate.sh fast` pass.

Out of scope: a Codex lead in AFK mode, new dashboard views, a token budget, any push or install, and the design's live check with a person present.

## The result

The finished work is on branch **`afk/afkmode`** at **e85c711**. Its base is `main` at **fb1fd8f** (`git merge-base main afk/afkmode` prints fb1fd8f561a3b5ce0fc51ad30ebc80481f041cd3). The branch changes 58 files, with 2944 lines added and 104 removed.

Nothing was pushed. The repository has no remote. The member branches `afkmode/start`, `afkmode/endings`, `afkmode/stophold` and `afkmode/skill` are kept.

| Member branch (head) | Commits | What it holds |
| --- | --- | --- |
| `afkmode/start` (c774aed) | 7 | Store `create_afk_team` takes `extra`; launch budget refusal, model-table warning, `check_new_team`, exported `ensure_hold` and `lead_process_missing`; the `teamwire afk start` command and doctor's `launch_checks`; review fix for `lead_cwd` and the task-copy order |
| `afkmode/endings` (0ccf0c3) | 4 | Close refuses without a complete REPORT.md; `StatusView.afk` and `AfkView`; the status line, the `teams` mark, `afk_budget_ends_at`; ui zod schemas, fixtures and tests; review fix for a date-bound test |
| `afkmode/stophold` (e5e8e23) | 4 | `afk_hold_notes` in `teamwire-report`; the Stop hook joins it with the watch notes; review fix for the unreadable-team test |
| `afkmode/skill` (023e6ac) | 9 | `examples/skills/afk/SKILL.md` and the main skill's pointer line; the plugin renders the AFK skill; the Codex render drops the pointer line (D25); ADR 0025, `AFK-1` to `AFK-12`, and the API, architecture and failure-mode docs; closer fixes F2 and F3 |
| lead, on `afk/afkmode` | 1 | 9e45487: the six test helpers pass the fourth argument to `create_afk_team` (D26) |

The merges on `afk/afkmode`, in order, are a67076c (start), f90d60a (endings), 2c30913 (stophold), 5d2bbd5 (skill), the lead commit 9e45487, and e85c711 (skill again, after the closer's doc fixes). None of the merges had a conflict.

To try the result:

```bash
git checkout afk/afkmode
cargo build --locked
target/debug/teamwire afk start --help
cargo test --locked -p teamwire-store -p teamwire-launch -p teamwire-cli \
  -p teamwire-report -p teamwire-close -p teamwire-watch -p teamwire-msg \
  -p teamwire-mcp -p teamwire-harness-claude -p teamwire-harness-codex \
  -- --skip herdr_env_wins_over_tmux
(cd ui && bun test && bun run typecheck)
```

Those ten crates are every crate the run changed. The `--skip` drops the one test that fails on this machine for an environment reason (see Verified).

## Verified

The lead ran these on the final joined branch e85c711 (`afk/lead-verification.md`):

| Command | Where | Exit |
| --- | --- | --- |
| `scripts/gate.sh` (full) | afk/afkmode e85c711 | 1. Steps fmt, tidy and clippy pass. The test step fails only at `herdr_env_wins_over_tmux`, and the gate stops there (`afk/gate-joined.log`) |
| `TEAMWIRE_FAKES_PREBUILT=1 cargo test --workspace --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux` | e85c711 | 0. 171 test binaries ok, 0 failed (`afk/test-joined-skip1.log`) |
| `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked` (gate step 5) | e85c711 | 0 |
| `scripts/check-docs.sh` (gate step 6) | e85c711 | 0 |
| `bun test` in `ui/` | e85c711 | 0. 163 pass, 3 skip, 0 fail |
| `bun run typecheck` in `ui/` | e85c711 | 0 |
| `cargo test --locked -p teamwire-cli --test tmux_wiring` | `main` fb1fd8f | `herdr_env_wins_over_tmux` FAILED at `crates/teamwire-cli/tests/tmux_wiring.rs:347`, the same failure as on the joined branch |
| `teamwire verify afkmode` | all four member branches | PASS. The lead read and waived each flag: the `#![expect(...)]` headers of new test files, and the Codex files that D25 gave to `skill` |

The full gate exits 1 only at `herdr_env_wins_over_tmux`. That test fails the same way on `main`. The machine has both `HERDR_ENV` and `TMUX` set, and the test reads the real terminal environment. The team did not cause it (D12).

The closer ran these in its own clone (`reports/closer.md`):

| Where | Check | Exit |
| --- | --- | --- |
| joined 5d2bbd5, as first merged | `scripts/gate.sh` | 1. Clippy failed: 15 test targets did not compile (finding F1, fixed by 9e45487) |
| joined 5d2bbd5 plus the F1 patch | `scripts/gate.sh` | 1, only at `herdr_env_wins_over_tmux` |
| joined plus the F1 patch | workspace tests, nothing skipped | 101. 1727 passed, 1 failed: `herdr_env_wins_over_tmux` |
| joined plus the F1 patch | `cargo doc`, `check-docs.sh`, clippy again, `bun test`, typecheck | 0 each |
| start c774aed | gate 1 (herdr test only); workspace tests with that test skipped 0 (1693 passed); doc 0; check-docs 0; member check 0; `gate.sh fast` 0 | |
| endings 0ccf0c3 | gate 1 (herdr test only); workspace tests 0 (1688 passed); doc 0; check-docs 0; member check 0; `bun test` 0; typecheck 0; `gate.sh fast` 0 | |
| stophold e5e8e23 | gate 1 (herdr test only); workspace tests 0 (1688 passed); doc 0; check-docs 0; member check 0; `gate.sh fast` 0 | |
| skill 73505c1 | gate 1 (herdr test only); workspace tests 0 (1677 passed); doc 0; check-docs 0; member check plus the Codex crate 0 (505 passed); `gate.sh fast` 0 | |

The closer also ran 36 revert checks across the four branches. For each check, the closer broke one line of a fix and ran the test binary. The new test failed every time, so no new test passes without its fix. The four tests that only pin existing behaviour were not revert-checked, because they have no fix to break. The table of all 36 is in `reports/closer.md` under "Revert checks".

The closer checked skill at 73505c1. The final skill head 023e6ac adds one doc commit after that. The lead's runs on e85c711 cover it.

## Decisions made for you

Every decision is written out with its reason and how to reverse it in `afk/decisions.md` (D1 to D26). These change the result the most:

- **D1, roster.** One planner, implementers, one reviewer per branch, the closer and the reporter. No analyst and no scouts, because the design already fixes the end state.
- **D2.** The first planner's turns kept dying when the machine slept, so the lead removed it and started `planner2`.
- **D24, no plan reviewer.** With 3h13m of budget left, an extra review would have pushed the implementer launch past two thirds of the budget.
- **D25.** The Codex harness's plugin render drops the `/teamwire:afk` pointer line, because a Codex lead has no AFK skill. A test in `crates/teamwire-harness-codex/tests/lead_skill.rs` checks the drop.
- **D26.** The lead fixed the six test helpers on `afk/afkmode` in one commit, 9e45487, using the closer's patch. The fix could not go on the member branches, which lack the four-argument store call.
- **D4.** `afk start` looks for the lead's process before it creates anything, so `no_lead_process` leaves nothing behind.
- **D5, report headings.** Close counts a heading when a line is a `#` heading or a numbered item 1 to 8, with optional `**`, and its text starts with the heading's words in any case. Close does not check the order.
- **D6, the spent note.** Past the budget, the hold note starts `budget spent at <time>:` and then repeats the design's instruction to finish, report and close.
- **D7, output text.** `status` prints `afk: budget ends <ts>, <H>h<MM>m left, <n> decisions` or `afk: budget spent at <ts>, <n> decisions`. `teams` prints `(open, afk)`.
- **D8, model table.** Launch ranks members by kind against design section 6 part 4. A model or effort level the table does not know never warns. Launch only warns and never refuses.
- **D9.** `afk start` runs only the doctor checks a launch needs: host, harness binary, lead setup and `.worktrees/` ignored.
- **D12.** Member checks skip `herdr_env_wins_over_tmux`.
- **D13 and D14.** Four sonnet implementers at high effort ran in one wave. The design's "endings" unit was split into `stophold` and `endings`.
- **D17.** `decisions` in status counts lines that start with `D<n>` and a space. Time left is zero once the budget is spent.
- **D18.** Past the budget, launch refuses each new implementer on its own and starts the other members, so a reporter can always join.
- **D19 and D20.** `afk start` finds the lead the way launch does. `afk start` prints refusals as `refused: <code>: <message>` and exits with launch's REFUSED code.
- **D21.** The `stophold` and `endings` branches created AFK teams in tests through one helper per crate, so the fix after merge was one line per helper.
- **D22.** The Stop hook prints the watch notes first and the AFK notes second, in one reply, within the hook's time budget.
- **D23.** Install renders the AFK skill as one more plugin file. Doctor's existing plugin check covers that file with no code change.

The members made these choices on their own (from each member's "Decisions I made" list):

- `start` added a refusal code, `bad_task`, for a task file that cannot be read. The skill docs include it.
- `start` records `lead_cwd` only when the harness can name the lead session's folder, as launch does. Otherwise `team.created` gets no `lead_cwd` key. This follows review finding 1.
- `start` copies the task into `afk/` before it writes the team row, so a failed copy leaves no team. Design 5.2 lists the two steps the other way round.
- `endings` refuses an unreadable REPORT.md as `afk_report_incomplete: <path> cannot be read: <error>`.
- `endings` made the ui fields `.nullish()`, so they accept a missing key as well as `null`.
- `endings` put the status and teams changes into one commit, because the ui files hold both.
- `stophold`'s `afk_hold_notes` stops reading teams at the first team with a working member and a live watch.
- `skill` placed the pointer line in the main skill right after the paragraph about standalone tasks.

## Not done, and why

- **The design's live check with a real lead (design section 7).** Nobody ran `/teamwire:afk` in a real Claude Code session. That check needs `cargo install` and `teamwire install`, and the AFK rules forbid both. You need to run it yourself.
- No test runs the `teamwire close` binary on an AFK team. Library tests cover the refusal. The CLI prints the refusal through code that already existed (`crates/teamwire-cli/src/cmd/close.rs:113`).
- No test checks that the archive holds `afk/REPORT.md`. The archive copies the whole team folder (`crates/teamwire-close/src/archive.rs:78`), so the closer judged that it does by reading the code.
- `reopen`, `verify` and `metrics` on a team with no members have no test. `docs/failure-modes.md` says so.
- The verify check for `skill` (closer F5) does not include the Codex crate. The closer ran the Codex tests by hand, and they passed (505 passed). The full test run on e85c711 also covers them.

## Risks

- **The sleep hold did not stop the machine from sleeping.** Three member turns died with "API Error: Your computer went to sleep mid-response" (11:41Z, 12:30Z and 14:34Z), and the watch heartbeat once went 10 minutes old. The lead saw wall-clock jumps. The hold is `caffeinate` with `PreventUserIdleSystemSleep`, which does not stop sleep on a closed lid or a forced sleep (`afk/lead-notes.md`). The hold code works as designed, but this run shows the hold is not enough on its own. Nobody worked out whether the lid was closed.
- **One test fails on this machine for environment reasons.** `herdr_env_wins_over_tmux` fails on `main` and on every branch, because `HERDR_ENV` and `TMUX` are both set. The full gate stops at that test, so on this machine the gate never reaches its doc and docs steps. The lead and the closer ran those steps one by one.
- **A crash can leave an AFK team that cannot close.** If the process dies after `afk start` writes the team row, the lead holds an open AFK team. Close refuses it with `afk_report_incomplete`, and a second `afk start` refuses with `afk_already_running`. The only way out is to write a REPORT.md with all eight headings by hand, because close has no override flag. The closer graded this a rare risk.
- **`crates/teamwire-cli/src/cmd/doctor.rs` grew from 760 to 786 lines.** Tidy fails a file past 800 lines, so the next change to that file will need a split first.
- **The Codex drop matches the pointer line exactly** (`crates/teamwire-harness-codex/src/lead_setup/plugin.rs:28-30`). Rewording that line in the main skill needs the same change in the Codex constant. The Codex test fails until someone makes that change.
- **The hold's "will wake" rule misses the lead's own background commands.** The design accepts this. A lead that waits on its own command must stop again when that command wakes it.
- **A team in the `closing` state gets no AFK hold.** The hold checks for an open team only, as design 5.3 and D22 say (from the stophold reviewer).
- **The run went past its budget.** The budget ended at 18:03:49Z, and the closer and this report ran after that. The skill allows that work past the budget.
- `locked_stores_let_the_stop_through_at_the_bound` (a test on `main`) failed once for `skill` while other members were building. It passed alone and in all six of the closer's runs. It may be sensitive to load.

## Cost

Wall time: the team was created at 2026-10-23T10:03:49Z. This report was written at about 19:00Z, so the run took about 8h56m. The budget was 480 minutes and ended at 18:03:49Z, so the run went about 56 minutes past it.

Tokens per member, from `teamwire metrics afkmode`:

| Member | Model, effort | Output | Input | Cache write | Cache read |
| --- | --- | ---: | ---: | ---: | ---: |
| planner2 | opus, high | 55,344 | 162 | 129,391 | 10,462,559 |
| start | sonnet, high | 57,325 | 154 | 408,400 | 13,232,899 |
| stophold | sonnet, high | 25,625 | 94 | 208,374 | 5,373,480 |
| endings | sonnet, high | 38,396 | 122 | 278,571 | 7,730,834 |
| skill | sonnet, high | 44,799 | 168 | 322,600 | 12,933,427 |
| rev_start | opus, xhigh | 32,273 | 74 | 158,052 | 4,926,944 |
| rev_stophold | opus, xhigh | 21,525 | 60 | 103,518 | 3,051,018 |
| rev_endings | opus, xhigh | 28,380 | 84 | 128,299 | 4,956,183 |
| rev_skill | opus, xhigh | 25,473 | 82 | 146,373 | 5,230,172 |
| closer | opus, xhigh | 81,775 | 322 | 277,777 | 30,113,879 |
| **Total** | | **410,915** | **1,322** | **2,161,355** | **98,011,395** |

The metrics list no tokens for the first planner, which the lead removed (D2), or for this reporter, which was still running. The lead's own session is not in the table either.

Other counts from the metrics: 12 members started, 9 implementer completions (first results plus review rounds), 8 reprompts from the lead, and 1 question from a member.
