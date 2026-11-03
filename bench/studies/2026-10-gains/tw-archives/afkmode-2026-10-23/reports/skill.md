- 9bb5bf6 AFK skill text, AFK_SKILL_TEXT constant with front matter test, pointer line in main skill | left: render into plugin, ADR and requirements, docs
- c6a8a25 plugin renders teamwire/skills/afk/SKILL.md (stamped, in version hash); render_with takes the AFK text; tests for file list, version, out-of-date plugin | left: ADR, requirements, docs
- 86c7176 ADR 0025 and AFK-1..AFK-10 in docs/requirements.md | left: api docs, ARCHITECTURE, failure-modes, README, examples/README, corrections from notices
- 19579a7 docs: cli, core, mcp, ARCHITECTURE, failure-modes section 3, conventions, README, examples/README | left: corrections from notices, checks
- cf2fbd6 docs: bad_task refusal added after the start notice; hold, status and teams texts checked against the other branches | left: nothing

## Commands run (exit codes)
- `cargo test -p teamwire-harness-claude -p teamwire-cli --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux`: exit 0 (36 suites ok).
- `scripts/check-docs.sh`: `docs check: ok`, exit 0.
- `scripts/gate.sh fast`: fmt, tidy, clippy ok, exit 0. Tidy prints `PROTECTED-CHANGED docs/adr/0025-afk-runs.md`, the new ADR the brief allows (D15).
- `scripts/test-changed.sh`: exit 101 on `herdr_env_wins_over_tmux` (D12, fails on the base). Rerun on the same packages with that test skipped: one failure, `locked_stores_let_the_stop_through_at_the_bound` in crates/teamwire-cli/tests/lead_hook.rs, a timing test from the stophold branch that is not in my branch's diff... it ran with the machine busy; rerun alone passed (`cargo test -p teamwire-cli --test lead_hook -- locked_stores`, exit 0). Report to the integrator as a timing flake under load.

## Rewordings of design section 6
Text copied from the design with one change: the line listing the reporter's input paths is re-wrapped (no word changed). No rule reworded or dropped. The skill is not shortened.

## Checked against peers' notices
Stop hook note texts, start's output lines and codes, status/teams/close texts match the docs. Added `bad_task` (start's code for an unreadable task file) to cli.md, AFK-1 and the ADR.

## Decisions I made
- Placed the pointer line as its own line right after the paragraph about standalone tasks, near the top of the main skill.
- `render_with` takes the AFK text as a second argument so a test changes it alone, like the reference text.
- Failure-mode rows for AFK cite test files, not function names, where the other branches' test names were not final; the two rows with known functions name them.
- Left crates/teamwire-cli/tests/preset.rs unchanged (main skill only), as the brief says.
- f37f9cc review fixes: AFK-8 cites status/teams/mcp tests, memberless-team failure row states only what is tested, AFK-11 (--add on memberless team) and AFK-12 (unreadable row gives no note) added | left: finding 1 (Codex pointer line) waits on the lead
- c606cc6 Codex plugin render drops the /teamwire:afk pointer line (review finding 1, D25); test tests/lead_skill.rs fails without the drop | left: nothing

## Review round (rev_skill)
- Finding 1 holds: fixed in crates/teamwire-harness-codex/src/lead_setup/plugin.rs per the lead's answer (D25); test crates/teamwire-harness-codex/tests/lead_skill.rs fails when the drop is disabled (checked) and passes with it.
- Finding 2 holds: AFK-8 now cites cli status.rs, cli teams.rs, mcp teams.rs and watch afk.rs (names checked on the endings branch).
- Finding 3 holds: the memberless-team row now names only tests that exist (launch --add, status view, close, Stop hook) and says reopen, verify and metrics have no test yet.
- Finding 4 holds: AFK-11 (--add on a memberless AFK team) and AFK-12 (unreadable row gives no note) added.
- Decision: the AFK pointer is removed by exact line match, so a reword of that line in the main skill needs the same change in the Codex plugin (the test would then fail, which is the alarm).
- 73505c1 lead_skill.rs test returns Result, so tidy no longer reports WEAKENED for a new #![expect] | left: nothing
- Review-round checks: `cargo test -p teamwire-harness-claude -p teamwire-harness-codex -p teamwire-cli --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux` exit 0; `scripts/check-docs.sh` ok; `scripts/gate.sh fast` ok (only the allowed PROTECTED-CHANGED line for the new ADR).
- Decisions I made (review round): wrote the Codex test to return Result, not suppress unwrap, because tidy reports a new `#![expect(` as WEAKENED.
- 023e6ac docs: lead_cwd only when the harness can name the folder (ADR, AFK-2, core.md); close refusal printed as REFUSED: afk_report_incomplete: ... (ADR, AFK-7) | left: nothing

## Closer round (F2, F3, F4)
- F2 holds (checked crates/teamwire-cli/src/cmd/afk.rs on afkmode/start: lines 209-215 and 243-246 pass `{}` when the harness cannot name the folder). Fixed in the ADR, AFK-2 and docs/api/core.md.
- F3 holds (close.rs on afkmode/endings prints `REFUSED: ...`). Fixed in the ADR and AFK-7. docs/api/cli.md already had the real text.
- F4 holds: `git show main:crates/teamwire-cli/tests/lead_hook.rs` has `locked_stores_let_the_stop_through_at_the_bound` at line 175, so the test is on the base, not stophold's. My earlier line was wrong. It failed once in a run made while other members built on the machine, and passed alone; the closer's six runs did not fail. I call it load-sensitive, not stophold's.
- F5 (the spec check misses the Codex crate) is the lead's; I ran the Codex crate in my check below.
- Closer-round checks: check command plus harness-codex tests exit 0; check-docs ok; gate fast ok. Decisions I made (closer round): none.
