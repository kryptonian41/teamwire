# Task: build AFK mode for teamwire

teamwire needs AFK runs: a lead runs one team on one task while the user is away, with no questions to the user until a report is written.

The design and build plan are in `docs/design/2026-10-19-afk-mode.md`. Read all of it first. Schema version 5 (`team.afk_budget_ends_at`) is already committed, as the plan's "Precondition" says.

End state: everything that sections 5 and 6 of the design describe exists and is tested.

- `teamwire afk start` (section 5.2).
- The AFK hold in the lead's Stop hook (section 5.3).
- The budget and the model table in launch (section 5.4).
- Close and status for AFK teams (section 5.5).
- The AFK skill with the agreed content of section 6, installed the way the other skills are (section 5.6).

Use the command names, flags, refusal codes and messages that the design gives. Where the design leaves a detail open, decide it and say what you decided in your final message.

## Names the design does not give

Use these names, which the rest of the code will rely on:

- `teamwire afk start` refuses with these codes, in addition to the ones the design names:
  - `no_lead_session` when no lead harness session id reaches the command.
  - `no_lead_process` when the platform needs a sleep hold and the lead's process cannot be found. Nothing is created.
  - `afk_already_running` when the calling session already leads an open AFK team.
  - `doctor_failed` when a doctor check fails. The refusal names the check.
- Crate `teamwire-report` exports `afk_hold_notes(home: &Path, session: &SessionId, platform: &dyn Platform, clock: &dyn Clock) -> Vec<String>`, beside `teams_without_watch` and `watch_missing_note`. It returns the AFK hold notes for the open AFK teams that `session` leads, when nothing will wake the lead: no open team the session leads has both a working member and a live watch lease.
- Crate `teamwire-store`: `create_afk_team` takes a fourth parameter, `create_afk_team(at, team: &NewTeam, budget_ends_at, extra: &serde_json::Value)`. The keys of `extra` are merged into the data of the `team.created` event. `teamwire afk start` passes `{"lead_cwd": <the lead's working directory>}`.
- Crate `teamwire-watch`: `StatusView` gets `afk: Option<AfkView>`, `None` for a team that is not an AFK run. `AfkView` has the fields `budget_ends_at: Timestamp`, `seconds_left: u64` (zero once the budget is spent), `spent: bool` and `decisions: u32` (the number of `D<n>` entries in `afk/decisions.md`). `status --json` writes the same field names.
- `teamwire teams --json` gives each team an `afk_budget_ends_at` key: a timestamp string for an AFK team, `null` otherwise. The dashboard's zod schemas in `ui/src/shared/teamwire.ts` accept the `afk` object and this key, and also accept older output without them.

## How to work and finish

- The repository is teamwire, a Rust tool, with a Bun and TypeScript dashboard in `ui/`. Read `AGENTS.md` first, and `ui/AGENTS.md` if you change the dashboard.
- Add tests for each part, in each crate's `tests/` folder for Rust.
- Check your work with the tests of each crate you changed (`cargo test -p <crate> --locked`), `bun test` in `ui/` if you changed it, and `scripts/gate.sh fast` from the repository root. The local `main` branch marks the commit you started from, which the gate compares against.
- Commit the finished work in this repository. Do not push; there is no remote. End your final message with the name of the branch that holds the finished work.
