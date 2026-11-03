# Review of afkmode/closestat (close and status for AFK teams)

## Summary

The branch is sound. I found no correctness, security or data-loss defect. Every name, refusal code,
message, field and test name matches the task, the brief, D17 to D20, D30 and D35. All 18 changed
files are on the brief's owned list. Each `--json` change is in the same commit as its zod copy (D30).
Status, teams and close each work on an AFK team with zero members, and tests cover each case.

Two low findings follow. Neither one blocks a merge.

I did not build or run any test. No verify log for closestat existed. Every statement below comes
from reading the diff `git diff afk/base...afkmode/closestat` (3 commits: 09933e1, c92ec32, f87ba47)
and the code it calls.

## Findings, ranked by harm

### 1. Low: one close test passes on afk/base, and it is not marked as a guard

- File: `crates/teamwire-close/tests/afk_report.rs:50`, `close_of_an_afk_team_with_a_full_report_archives_the_afk_files`.
- Brief line: "Each test of new behaviour fails without its change; a test marked (guard) pins today's
  behaviour and passes on afk/base too (D33)." D33 names only two guards for closestat:
  `removing_one_member_of_an_afk_team_needs_no_report` and `an_ordinary_team_closes_without_a_report`.
- Failing state: revert `crates/teamwire-close/src/plan.rs:75-78`, which is the call to
  `afk_report::refusal`. The test still passes. On afk/base, close never reads `afk/REPORT.md`. The
  rig's AFK team has zero members, so the close reaches `closed`. `archive.rs` copies every file, so
  the archive holds `afk/REPORT.md` and `afk/task.md`.
- Why it matters: the test does check something real once the change is in. Its `FULL` report holds
  the heading forms `## 1.`, `## **...**`, `### verified:` and `## 5)`, so the test proves the parser
  accepts them. But a closer that revert-checks every test that is not a guard will see this test
  pass without the change. The lead should expect that result, or should list the test as a guard.

### 2. Low: close calls an existing but unreadable report "missing"

- File: `crates/teamwire-close/src/afk_report.rs:26-31`. Close treats any failure of
  `read_to_string` as a missing report.
- Failing input: `afk/REPORT.md` exists and holds one byte that is not valid UTF-8, such as `0xFF` in
  pasted tool output. The same happens when the file cannot be read, or when the path is a directory.
  Close refuses with `afk_report_incomplete: <home>/teams/<team>/afk/REPORT.md is missing`, but the
  file is there.
- Why it matters: the AFK lead runs with nobody present. It reprompts the reporter, as the skill says,
  and the reporter finds the file already in place. The status side treats the same kind of error
  differently. `crates/teamwire-watch/src/afk.rs:34` counts only `NotFound` as an absent file, and it
  reports any other read error.
- Likelihood is low. The reporter writes the file with the Write tool, which always writes UTF-8, so a
  rewrite fixes the file. D17 defines only the two texts, "is missing" and "lacks the headings", so
  this finding is about what the refusal tells the lead, not about the agreed shape.

## Checked and found sound

| What | Where | Result |
| --- | --- | --- |
| Close's refusal texts and code | `afk_report.rs:26-46` | Match D17 and the brief's two examples. The missing names come out in skill order. |
| Heading rule | `afk_report.rs:51-65` | Needs one to six `#` and a space. Strips `**`, then a leading `N.` or `N)`. Compares lowercased text with `starts_with`. Order is not checked. Cannot panic. |
| Where the check runs | `plan.rs:72-78` | Only for a whole-team close of an `Open` team. The dry run, `close` and the `close.refused` event all carry it. `--role`, `--clear-stale-lock` (returns before the plan), `closing`, `closed` and ordinary teams skip it. No flag skips it. |
| Empty-store close path | `empty.rs:23-40` | Needs no team row, so an AFK team never takes it. |
| Archive | not changed | The test shows the `afk/` files in the archive. |
| AfkView fields and math | `afk.rs:11-46` | Exactly `budget_ends_at`, `seconds_left` (`ends.saturating_since(now)`), `spent` (`now >= ends`) and `decisions: u32`. The boundary test ages the clock to the exact end and expects `(0, true)`. |
| Decision count | `afk.rs:50-60` | Matches `^\s*D[0-9]+\s` line by line. An indented entry counts (D35). A mid-sentence `D3` does not count. A missing file counts 0. Any other read error counts 0 and adds a line to `errors`. |
| Status line | `cmd/status.rs:124-138` | Printed right after the watch line, in both D18 forms. Uses `minutes()`. The all-teams summary line is unchanged. |
| `"afk": null` for an ordinary team | `teamwire-watch/src/lib.rs:156-157` | The key is present and null. Tests cover both the library and the binary. |
| MCP `team_status` and `team_teams` | `teamwire-mcp/src/reads.rs:26-37, 78-83` | Both serialize the same structs, so both get the new keys with no MCP change. |
| Teams marker and key | `cmd/teams.rs:44-51`, `directory.rs:20-21, 50` | `(open, afk)` appears only when the key is set. `describe()` fills the key, so the CLI and MCP both get it. |
| zod copy | `ui/src/shared/teamwire.ts` | `AfkView`, `StatusView.afk` and `TeamEntry.afk_budget_ends_at` match D20 exactly. Tests cover each key as present, null and absent. `fixtures/` is unchanged. |
| D30 | commits c92ec32 and f87ba47 | Each `--json` change shares its commit with the zod change and the zod test. |
| AGENTS.md hard rules | all new non-test code | No `unwrap`, `expect` or `panic`. Time comes in as a `now` parameter taken from the Clock. Nothing new prints outside teamwire-cli. Nothing writes files. No lint suppression, no new dependency, no protected file. |
| Zero members | status, teams, close tests | `Bed::afk`, `add_afk_team`, `Rig::afk` and the CLI close test all use an AFK team with no members. The watch's `TEAM-COMPLETE` rule needs at least one settled member (`teamwire-watch/src/plan.rs:233`), so a zero-member team never triggers it. The diff does not change that rule. |
| Brief section 10 test names | all files | Every named test exists with its exact name. |
| Skill branch compatibility | `afkmode/skill:examples/skills/afk/SKILL.md:199-219` | The skill tells the reporter to write plain `## <name>` headings, and close's parser accepts them. |

Two recorded decisions of the implementer are fine as they stand:

- The status line prints `2099-01-01T00:00:00.000Z`, with milliseconds. The brief's example shows
  `2026-10-28T02:41:00Z`. The milliseconds come from `Timestamp`'s Display, which every other line
  and the JSON already use. The docs on the skill branch (`docs/api/cli.md:213`) use a `<time>`
  placeholder, so the docs do not conflict.
- The report check runs in the plan, before the close lock is taken, and does not run again under the
  lock. The crate's re-check under the lock (`team_close.rs:245-258`) covers busy and dirty members,
  as CLS-3 requires. A report deleted in the moment between the plan and the lock is the only gap.

## Questions for the parent

None.

## Decisions I made

- I reviewed by reading the code and did not build or run tests. My role does not require a run, no
  closestat verify log existed, and the closer runs every check on the joined branch. I claim no test
  result.
- I checked zero-member behaviour only for the readers this diff changes: status, teams and close. I
  also looked at the watch's `TEAM-COMPLETE` rule. I did not review reopen, verify or metrics, because
  this branch does not touch them.
- I treated the `.000Z` timestamp and the plan-time report check as accepted decisions, not findings.
  Each is recorded in the implementer's report and does no harm.
