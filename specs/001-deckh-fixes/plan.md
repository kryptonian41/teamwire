# Implementation Plan: Fixes from team deckh's live run

**Branch**: `dfix/spec` | **Date**: 2026-11-10 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/001-deckh-fixes/spec.md`

## Summary

The plan fixes ten findings from team deckh's run (F1, F2, F3, F4, F5,
F6, F8, F10, F11, F12) in the teamwire repository, on `develop` (`eda2e0b`).
Each fix comes with a test that fails without it, and the guidance text
changes in the same commit.

- **US1 (F1, F2, F3).** These three findings share one cause. The watch and
  status work out a member's state from its newest completion in its current
  generation, and only a `reprompt` starts a generation
  (`crates/teamwire-watch/src/snapshot.rs:103-105`,
  `crates/teamwire-msg/src/send.rs:311`). Two things happen outside that
  rule: a turn that another message wakes, and a message the lead read from
  its file. The fix records both:
  - The lead's hooks list each undelivered item to the lead.
  - `received` accepts message ids.
  - An `answer --re` settles its question.
  - The watch looks at the session behind an undelivered result.
  - A waking message starts a generation, and a message to a member that
    asked for a handoff is held for the handoff.

  One new ADR records these decisions: 0043.
- **US2 (F4, F5).** A new read-only command, `teamwire speckit draft`, lives
  in `teamwire-cli` and reads a spec-kit `tasks.md`. The command prints the
  files that phases and stories share, the tasks that can run apart, and
  one `owns` draft per story. Each draft also holds the repository files
  that mention the task paths. A new ADR, 0044, supersedes ADR 0042's
  decision 1 in part.
- **US3 (F6, F8, F10, F11).**
  - F6 and F8 are text changes in the presets and the reference. The tests
    for each check the preset content.
  - F10 adds one `Harness` method, with a default, that reports the current
    tool call. The method ships for Claude Code, Codex and pi.
  - F11 makes the close dry run name symlinks and their targets.
- **US4 (F12).** Verify counts a waiver only at the HEAD where the lead
  recorded it (`crates/teamwire-verify/src/lib.rs:310-317`), as ADR 0009
  point 2 says. The fix carries a waiver for a flag that names one file to
  a later HEAD while that file's content there matches the waived HEAD.
  Each file flag gets its path, a git helper compares the file's blob at
  the two HEADs, and a new module, `waivers.rs`, holds the rule. A new ADR,
  0045, supersedes ADR 0009 point 2 for file flags only. The store and the
  output do not change.

The root cause, fix, failing test and guidance text for each finding are
in [research.md](research.md).

## Technical Context

**Language/Version**: Rust (edition and toolchain from `rust-toolchain.toml`); the dashboard is TypeScript on Bun (`ui/`), touched only for F10's status field.

**Primary Dependencies**: Existing workspace crates only. `globset` is reached through `teamwire-verify` (already a dependency of `teamwire-cli`). The YAML reader comes from `crates/teamwire-cli/src/cmd/doctor_speckit/yaml.rs`. No new dependency, and no `Cargo.lock` change.

**Storage**: The SQLite team store (`crates/teamwire-store`). No schema migration. A held message is an `undelivered` delivery row with a new error code, `held_for_handoff`. `error_code` is free `TEXT` with no CHECK (`crates/teamwire-store/src/schema/v1.sql:74`).

**Testing**: `cargo test -p <crate> --locked`, with `teamwire_fakes::TestHome`, `FakeHarness` and `FakeHost` (`tests/fakes/`). The watch's `Bed` fixture is used for watch tests and the message rig for message tests. The CLI tests run the binary on a temporary git repository (`crates/teamwire-cli/tests/preset.rs:45-53, 238-246`).

**Target Platform**: macOS and Linux (`teamwire-platform`).

**Project Type**: Command-line tool and libraries (a Cargo workspace), plus a read-only dashboard.

**Performance Goals**: The lead's hooks stay inside `LEAD_HOOK_BUDGET` (`crates/teamwire-cli/src/cmd/hook/lead.rs`). `status --role` reads at most one session-file tail per member, as `context_fill` already does.

**Constraints**: AGENTS.md's hard rules, as the constitution's principle IV states them. No process types a message after the command that sent it has exited (ADR 0003 point 7, INV-8).

**Scale/Scope**: 10 findings, 4 user stories, about 12 crates touched. The crate list is in [research.md](research.md) under "Files per story".

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | How the plan meets it | Result |
|-----------|-----------------------|--------|
| I. Root cause first | Each finding in research.md opens with its root cause and file:line on `develop` (`eda2e0b`). For F1-F3 the causes come from rca's report, re-read in the code by the spec member. F12's cause was checked against deckh's waiver rows and the knit-3.0 commits `6a05efad1a` and `0e05a21fac`. | PASS |
| II. A failing test first | Each finding names its test and the assertion that fails without the fix. In tasks.md each test task comes before its fix task. | PASS |
| III. Guidance changes with the code | Each finding lists the skill, preset, reference, doc and `ui/src/shared/teamwire.ts` text in the same task as its code. F10 is the only `--json` change, and it updates `ui/src/shared/teamwire.ts` in that task. F12 changes no output: `teamwire verify` has no `--json`, and `Flag` keeps its fields. | PASS |
| IV. AGENTS.md hard rules | The new writes run inside `Store::write`: the ack by `received`, the ack by `--re`, the woken bump, and the held row. The F11 symlink check reads with `std::fs::symlink_metadata` and writes nothing. The F12 blob comparison runs git through the existing `Git` helper, which goes through `Platform::run`, and writes nothing. Only `teamwire-cli` prints. No new dependency. | PASS |
| V. Every harness | The F1 hook note lives in shared code that all three lead harnesses reach (`lead.rs` `lead_stop_in` and `lead_tool_in`). F10 adds a `Harness` method implemented for Claude Code, Codex and pi. F3 does not depend on the harness. | PASS |
| VI. Fakes only in tests | All tests use `tests/fakes/`, `TestHome`, the watch `Bed` and the message rig. F10 fixtures are session-file lines in each harness's `tests/fixtures/`. | PASS |
| VII. Smallest complete change; ADR by supersede | No new crate. Three new ADRs (0043, 0044, 0045) supersede named points. ADR 0003, 0007, 0009, 0035 and 0042 are not edited. F12 reuses `all_waivers` and needs no schema change. The changes reuse `ack_delivery`, `bump`, `doctor_speckit/yaml.rs`, `Owns` and the `context_fill` tail readers. | PASS |

Re-check after Phase 1 design: PASS, with no violations, so Complexity Tracking stays empty. The design added a new error code and not a new delivery state, which avoids a schema migration and a dashboard enum change.

## Project Structure

### Documentation (this feature)

```text
specs/001-deckh-fixes/
├── plan.md              # This file
├── research.md          # Root cause, fix, test and guidance per finding
├── data-model.md        # Entities and state changes
├── quickstart.md        # How to check each story end to end
├── contracts/
│   └── cli.md           # The changed and new command-line output
├── checklists/
│   └── requirements.md  # specify's checklist
└── tasks.md             # Written by speckit-tasks
```

### Source Code (repository root)

The crates, files and lines each story changes. The full table with the
reason for each line is in [research.md](research.md), under "Files per
story".

```text
crates/
├── teamwire-core/src/           # message.rs (HeldForHandoff code), harness.rs (current_tool), tool_run.rs (new type)
├── teamwire-store/src/          # delivery.rs (held rows, lead undelivered query), message.rs
├── teamwire-msg/src/            # send.rs, attempt.rs, resend.rs, rules.rs; tests/
├── teamwire-deliver/src/        # message_line (held list after reprompt text)
├── teamwire-report/src/         # received.rs (message ids), lead_stop.rs (undelivered_to_lead), lib.rs
├── teamwire-watch/src/          # plan.rs, status.rs, snapshot.rs, lib.rs (MemberView), new tool.rs; tests/
├── teamwire-launch/src/         # handoff.rs (held messages to the fresh session); contracts/talking_head.md
├── teamwire-close/src/          # member_close.rs (symlinks); tests/ignored_files.rs
├── teamwire-verify/src/         # lib.rs, owns.rs (Owns made public); inspect.rs (flag paths), git.rs (blob), new waivers.rs; tests/verify.rs
├── teamwire-harness-claude/src/ # lead_setup/plugin.rs (matcher), current_tool from the transcript
├── teamwire-harness-codex/src/  # current_tool from the rollout
├── teamwire-harness-pi/src/     # extension.rs (tool stamp), session.rs (current_tool)
└── teamwire-cli/src/            # cmd/received.rs, cmd/hook/lead.rs, cmd/status.rs, args.rs, main.rs, cmd/speckit.rs, cmd/speckit/*.rs; tests/
docs/
├── adr/0043-*.md, adr/0044-*.md, adr/0045-*.md # new ADRs
├── requirements.md, failure-modes.md, api/cli.md
examples/
├── skills/teamwire/SKILL.md, reference.md
└── presets/speckit.json, closer.json
ui/src/shared/teamwire.ts        # F10's MemberView field only
```

**Structure Decision**: The repository's existing workspace layout. Each
fix goes into the crate that owns the behaviour today. The new command is a
`teamwire-cli` module beside `doctor_speckit.rs`, because all spec-kit code
and its YAML reader already live there. `docs/conventions.md` section 5 lets
`teamwire-cli` use every lower layer, and no other crate needs this logic.

## Complexity Tracking

No constitution violations to justify.
