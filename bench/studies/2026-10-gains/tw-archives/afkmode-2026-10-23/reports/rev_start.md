# Review of branch afkmode/start (rev_start)

## Summary

- The branch is sound for most of what it must do. It builds `teamwire afk start`, launch's budget refusal, the model-table warning and the store's fourth parameter, as design sections 5.2 and 5.4, task.md and decisions D3, D4, D8, D9, D18, D19 and D20 describe.
- Every refusal code that task.md names exists, uses the binding name, and comes before the first write. Each one has a test.
- I ran the three new test files on a scratch copy of the branch: 18 passed, exit 0, on macOS.
- Finding 1 (medium): `afk start` records `lead_cwd` from its own current directory, while launch records the folder the lead session runs in. When the two differ, `teamwire reopen` resumes the lead in the wrong folder, and the resume finds no session.
- Finding 2 (low, appears only after the `endings` branch is merged): when the task copy fails after the team row is written, the printed advice ("run teamwire close") leads nowhere. Close refuses an AFK team with no REPORT.md, and a second `afk start` refuses `afk_already_running`.
- I found no stubbed or hard-coded test, no new dependency, no write transaction held open across an external call, and no unwrap, expect or panic outside tests.

## Findings, ranked by harm

### 1. Medium: `lead_cwd` is the shell's current directory, not the lead session's folder

- Where: `crates/teamwire-cli/src/cmd/afk.rs:209` and `:229`.
- What the code does: `create` writes `std::env::current_dir()` of the `teamwire afk start` process into `team.created` as `lead_cwd`.
- What launch does: launch's `create_team` (`crates/teamwire-launch/src/member.rs:113-124`) asks the harness for `session_cwd(session)`. That value is the folder the Claude Code session started in. When the harness cannot name the folder, launch records no `lead_cwd` at all.
- Who reads it: `teamwire reopen` uses the recorded `lead_cwd` first, and falls back to `session_cwd` only when nothing is recorded (`crates/teamwire-launch/src/reopen_lead.rs:62-63`). The comment at `reopen_lead.rs:61` says why the folder matters: "A resume only finds a session from there."
- Failing scenario:
  1. The user starts the lead's Claude Code session in `/work/repo`.
  2. During preflight the lead runs `cd crates/teamwire-cli && cargo test`. Claude Code keeps the Bash tool's working directory between calls inside the project.
  3. The lead runs `teamwire afk start ...`. The command records `lead_cwd = /work/repo/crates/teamwire-cli`.
  4. The lead session dies during the run, for example after a restart, and someone runs `teamwire reopen <team>`.
  5. Reopen spawns `claude --resume <session>` in `/work/repo/crates/teamwire-cli`. The resume finds no session there.
- Why the fallback cannot save it: `afk start` always records some folder, so reopen never reaches its `session_cwd` fallback. A launched team would have recorded `/work/repo`, or nothing.
- Brief lines this rests on: task.md says "`teamwire afk start` passes `{"lead_cwd": <the lead's working directory>}`". D19 says "so an AFK team records the same lead fields as a launched team". The implementer lists this choice under their own decisions ("`lead_cwd` is the current directory of `afk start` (not the harness's `session_cwd`)"). I report it anyway, because the choice breaks reopen in the scenario above.
- Test note: `a_start_creates_the_team_the_task_copy_and_the_hold` in `crates/teamwire-cli/tests/afk.rs` asserts that `lead_cwd` equals the directory the test runs the binary from. That assertion pins the current behaviour. The fake session file in that test carries no `cwd`, so the test cannot tell the two sources apart.

### 2. Low, after integration with `endings`: the recovery advice after a half-finished start leads nowhere

- Where: `crates/teamwire-cli/src/cmd/afk.rs:56`, together with `:231-237`.
- What the code does: `create` commits the team row through `create_afk_team`. Then it makes `afk/` and writes `afk/task.md`. When either of those two steps fails, the command prints `team <name> could not be set up: ...; run teamwire close <name> before trying again` and exits 6.
- Failing scenario:
  1. The disk is full, or `teams/<name>/` is not writable, when `afk start` reaches `create_dir_all` or `write_atomic`.
  2. The team row already exists. The team is open, has `afk_budget_ends_at` set, and has this session as its lead.
  3. The lead follows the advice and runs `teamwire close <name>`. After the `endings` branch is merged, close refuses with `afk_report_incomplete`, because `afk/REPORT.md` is missing (decision D5). Close has no flag that skips that check.
  4. The lead runs `afk start` again under a new name. The command refuses with `afk_already_running`, naming the half-made team.
- Result: the lead has no command that ends the half-made team or starts a new run. The only way out is to hand-write a REPORT.md with all eight headings.
- Rank: low, because the trigger is rare. The parent should know about it when merging `start` with `endings`.

## Checked and found sound

| Requirement | Where | Result |
| --- | --- | --- |
| Refusal codes `no_lead_session`, `background_lead`, `afk_already_running`, `bad_name`, `title_missing` / `title_too_long`, `team_exists`, `doctor_failed`, `no_lead_process`, `bad_budget` | `cmd/afk.rs:80-157` | Present. Each one comes before `Store::open`, so a refusal creates no team directory. Each one is tested with an assertion that no directory exists. |
| `doctor_failed` names the check | `cmd/afk.rs:130-140` | The message reads `<check name>: <detail>`. The test checks for `.worktrees ignored: `. |
| `no_lead_process` only where the platform has a sleep hold | `teamwire-launch/src/hold.rs:69-80` | `platform.sleep_hold()?` returns early on Linux. Tested in launch with both a fake hold and no hold. |
| `create_afk_team(at, team, budget_ends_at, extra)` | `teamwire-store/src/team.rs:150-168` | Merges `extra`, then `afk_budget_ends_at`. `create_team_with` then writes its own keys over any clash. The INSERT uses `id = 1`, so a second create on the same database fails and overwrites nothing. |
| Lead fields as launch records them | `cmd/afk.rs:197-226` | Harness, host, route (from `can_type`, as in `validate.rs` `lead_plan`) and session with kind all match launch. Tested. |
| `afk/task.md` byte for byte, through `Platform::write_atomic` | `cmd/afk.rs:235-237` | Tested with CRLF and non-UTF-8 bytes. |
| Hold through `ensure_hold` after the team exists, outside any write transaction | `cmd/afk.rs:238-239` | Matches D4. The macOS test checks that the hold is recorded as owned. |
| Success output, five lines | `cmd/afk.rs:62-66` | Matches D20 word for word. Tested. |
| Budget refusal for each new implementer past the budget, other kinds start, PARTIAL exit | `teamwire-launch/src/member.rs:59-68`, `afk.rs:58-67` | Matches D18. Uses `ctx.clock`. Tested both before and after the budget, and on a team that is not AFK. |
| Dry-run warning past the budget | `teamwire-launch/src/plan_rules.rs:81-90` | Present, as `role <r>: afk_budget_spent: ...`. Tested. |
| Model-table warning, once per member, AFK teams only, unknown values never warn | `teamwire-launch/src/afk.rs:17-52` | Matches the D8 table, ranks and text. Tested with six members. |
| `--add` into an AFK team with zero members | `teamwire-launch/tests/afk.rs` | Starts the member. No code change was needed, as the design expected. |
| Store, Clock, write_atomic, no unwrap / panic, no new dependency, no Cargo.lock change, owned files only | whole diff | All met. The diff touches only files the brief's section 11 lists. |

## Commands I ran

- `git diff main...afkmode/start` and `git log main..afkmode/start`: six commits, 16 files.
- `git archive afkmode/start` into my scratch folder, then from that copy: `cargo test -p teamwire-store -p teamwire-launch -p teamwire-cli --locked --no-fail-fast --test afk`. Exit 0, "18 passed (3 suites)". The run was on macOS, so it included the macOS-only `no_lead_process` CLI test.
- I did not run `scripts/gate.sh fast`, `scripts/test-changed.sh` or the full gate. The brief says not to run the full gate, and the implementer reports exit 0 for the other two.

## Decisions I made

- I ran the new tests in a `git archive` export inside my scratch folder, with its own `target/`. I did not check out the branch, because that would change the shared checkout or need a worktree.
- I report the implementer's own `lead_cwd` decision as finding 1. The decision is in their "Decisions I made" list, not in decisions.md, and it breaks `teamwire reopen` for an AFK lead whose shell left its starting folder.
- I report finding 2 even though it appears only once `endings` is merged. The parent merges both branches and is the one who can act on it.
- I left out three things I judged harmless. The reason text appears twice on a real launch past the budget, once as a plan warning and once as the refusal. The budget clock starts before the doctor probes run, so the budget can end some seconds early. A host lookup failure prints "run teamwire close" although nothing was created.
