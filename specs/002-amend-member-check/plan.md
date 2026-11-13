# Implementation Plan: Amend a running member's check and owns

**Branch**: `dfix2/spec` | **Date**: 2026-11-13 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/002-amend-member-check/spec.md`

## Summary

The plan fixes three findings from team deckh's run (F13, F14, F15) in the
teamwire repository, on `develop` (`7e78c1a`). Each fix comes with a test
that fails without it, and the guidance text changes in the same phase as
its code.

- **US1 (F13).** Launch is the only code that writes a member's check and
  owns: the store sets them once when it inserts the member row
  (`crates/teamwire-store/src/member.rs:111-118`), and launch renders them
  once into the seat rules file `<team dir>/contract/<role>.md`
  (`crates/teamwire-launch/src/member_steps.rs:293-309`). A new lead command,
  `teamwire amend`, changes them:
  - The store gets a setter, `Tx::set_member_scope`.
  - `teamwire-msg` gets `send_with_write`, which stores the note and runs
    the setter in one transaction, then delivers the note.
  - `teamwire-launch` gets `rewrite_contract`, so a handoff or reopen
    session reads the new scope.
  - `close --role` stops counting a verify record made at or before the
    newest `CHECK AMENDED:` note (`crates/teamwire-close/src/remove.rs:79-120`).
  - Verify flags `scope-changed` when the scope changed while it ran, so a
    run that straddles an amend never passes.
  - No event type, no schema change, no MCP tool, no `--json`.
  One new ADR records the decisions: 0046.
- **US2 (F14).** Text only. The `speckit` preset, the skill reference and
  the skill get a rule: the commit that removes spec-kit goes on the pull
  request branch, cut from the implementer's last verified head, never on
  the implementer's own branch. A lead whose removal is already there uses
  `teamwire amend` to swap the check. A preset test pins the rule.
- **US3 (F15).** The skill's role table misses the worktree bans that every
  seat's rules state (`crates/teamwire-launch/contracts/how_to_work.md:29`)
  in four rows, and three of them in the closer's row
  (`examples/skills/teamwire/SKILL.md:284-288`). The seat rules spell out
  each worktree command in full, the table lists every ban, and a new test
  compares the table with the rendered seat rules of each kind.

The root cause, fix, failing test and guidance text for each finding are
in [research.md](research.md). The command's interface is in
[contracts/amend-cli.md](contracts/amend-cli.md).

## Technical Context

**Language/Version**: Rust (edition and toolchain from `rust-toolchain.toml`). The dashboard (`ui/`) does not change.

**Primary Dependencies**: Existing workspace crates only. `clap` (already in `teamwire-cli`) for the arguments. No new dependency, no `Cargo.lock` change.

**Storage**: The SQLite team store (`crates/teamwire-store`), schema version 11, unchanged. One new setter on `Tx`. The seat rules file is outside SQLite and is written with `Platform::write_atomic`.

**Testing**: `cargo test -p <crate> --locked`, with `teamwire_fakes::TestHome`, `FakeHerdrEnv` and the existing rigs: `crates/teamwire-cli/tests/handoff.rs`'s rig shape for the new `tests/amend.rs`, `crates/teamwire-close/tests/support/rig.rs` for the close rule, `crates/teamwire-verify/tests/verify.rs`'s `Fixture` for the flag, `crates/teamwire-msg/tests/send.rs`'s rig for `send_with_write`. `tests/fakes/` does not change.

**Target Platform**: macOS and Linux (`teamwire-platform`).

**Project Type**: Command-line tool and libraries (a Cargo workspace).

**Performance Goals**: None new. `close --role` reads one more listing (the role's lead notes) from the store, once per removal.

**Constraints**: AGENTS.md's hard rules, as the constitution's principle IV states them. No write transaction stays open during delivery or the file rewrite.

**Scale/Scope**: 3 findings, 3 user stories, 10 crates touched (`teamwire-core`, `teamwire-store`, `teamwire-msg`, `teamwire-verify`, `teamwire-close`, `teamwire-launch`, `teamwire-cli`, `teamwire-harness-codex`, `teamwire-harness-pi`, and `teamwire-mcp` only through its existing guard test). Files per story are in [research.md](research.md).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | How the plan meets it | Result |
|-----------|-----------------------|--------|
| I. Root cause first | Each finding in research.md opens with its root cause and file:line on `develop` (`7e78c1a`), read in the code by the spec member. | PASS |
| II. A failing test first | Each finding names its tests and why each fails on the base. Two guard tests are marked as guards. In tasks.md each test task comes before its fix task. | PASS |
| III. Guidance changes with the code | Each story's guidance (skill, reference, AFK skill, preset, docs, README, ADR) is in the same phase as its code. No `--json` output changes, so `ui/src/shared/teamwire.ts` stays. | PASS |
| IV. AGENTS.md hard rules | The scope change and the note are one `Store::write` (through `send_with_write`). Delivery and the seat rules rewrite run after the commit. The rewrite uses `Platform::write_atomic`. Time comes from `Clock` (note `created_at`, record `recorded_at`). Only `teamwire-cli` prints and exits. No new dependency. Layers: the CLI calls store, msg, launch and close; `teamwire-launch` still does not depend on `teamwire-msg`. | PASS |
| V. Every harness | The amend rewrites the seat rules file, which all three harnesses read at handoff and reopen (`teamwire-harness-claude/src/launch.rs:51`, `teamwire-harness-codex/src/launch.rs:44`, `teamwire-harness-pi/src/launch.rs:42-77`). The lead guidance is the shared `SKILL.md` and `reference.md`, which Codex and pi leads get wrapped (`teamwire-harness-codex/src/lead_setup/plugin.rs:62`, `teamwire-harness-pi/src/lead_setup.rs:95-100`); one test per harness pins that the rendered skill names `teamwire amend`. | PASS |
| VI. Fakes only in tests | All tests use `TestHome`, `FakeHerdrEnv`, the close rig, the verify `Fixture` and the message rig. | PASS |
| VII. Smallest complete change; ADR by supersede | No new crate, table, event type or schema step. Each new function sits next to the helper it extends (`set_member_place`, `send_with`, `write_contract`, `head-moved`). One new ADR, 0046, which supersedes nothing. No existing ADR is edited. | PASS |

Re-check after Phase 1 design: PASS, with no violations, so Complexity
Tracking stays empty. The design added the `scope-changed` flag (research
US1 decision 5) instead of a stored start time, which avoids a schema step.

## Project Structure

### Documentation (this feature)

```text
specs/002-amend-member-check/
├── plan.md              # This file
├── research.md          # Root cause, fix, test and guidance per finding
├── data-model.md        # The records amend writes and close reads
├── quickstart.md        # Replay the cases by hand
├── contracts/
│   └── amend-cli.md     # The command, its exit codes, its note and refusal text
├── checklists/
│   └── requirements.md  # specify's checklist, all ticked
└── tasks.md             # /speckit-tasks output
```

### Source Code (repository root)

```text
crates/
├── teamwire-core/src/message.rs           # US1: AMEND_MARKER ("CHECK AMENDED:")
├── teamwire-core/src/lib.rs               # US1: export AMEND_MARKER
├── teamwire-store/src/member.rs           # US1: Tx::set_member_scope
├── teamwire-msg/src/send.rs               # US1: send_with_write; send_with calls it
├── teamwire-msg/tests/send.rs             # US1: one-transaction test
├── teamwire-verify/src/scope.rs           # US1: new, the scope-changed compare
├── teamwire-verify/src/lib.rs             # US1: call scope.rs in the record write
├── teamwire-verify/tests/verify.rs        # US1: scope-changed cannot be waived
├── teamwire-close/src/remove.rs           # US1: verify_gap reads the newest amend note
├── teamwire-close/tests/remove.rs         # US1: close rule tests
├── teamwire-launch/src/contract.rs        # US1: rewrite_contract (or a new contract_rewrite.rs)
├── teamwire-launch/src/lib.rs             # US1: export rewrite_contract
├── teamwire-launch/tests/contract_rewrite.rs       # US1: new
├── teamwire-launch/contracts/how_to_work.md        # US3: spell out the worktree commands
├── teamwire-launch/tests/contract_worktree_rule.rs # US3: expected text changes (line 22)
├── teamwire-launch/tests/role_table_git_bans.rs    # US3: new
├── teamwire-cli/src/args.rs               # US1: AmendArgs, Command::Amend
├── teamwire-cli/src/main.rs               # US1: `mod amend` in the `mod cmd` block (line 16) and the dispatch (line 92)
├── teamwire-cli/src/cmd/amend.rs          # US1: new
├── teamwire-cli/tests/amend.rs            # US1: new
├── teamwire-cli/tests/preset.rs           # US2: the removal rule test
├── teamwire-harness-codex/src/lead_setup/plugin.rs # US1: test, skill names amend
└── teamwire-harness-pi/src/lead_setup.rs  # US1: test, skill names amend
examples/
├── presets/speckit.json                   # US2: REMOVE SPEC-KIT rule
└── skills/
    ├── teamwire/SKILL.md                  # US1 amend text; US2 sentence; US3 role table
    ├── teamwire/reference.md              # US1 amend row and flag; US2 rule in "The flow"
    └── afk/SKILL.md                       # US1: amend for a wrong check
docs/
├── adr/0046-amend-changes-a-running-members-check-and-owns.md  # US1: new
├── api/cli.md                             # US1
├── api/core.md                            # US1
├── requirements.md                        # US1: LCH-33, VER-9, CLS-9
├── ARCHITECTURE.md                        # US1
└── failure-modes.md                       # US1
README.md                                  # US1
```

**Structure Decision**: The existing Cargo workspace. Each new part goes in
the crate that owns its concern (`docs/ARCHITECTURE.md:55-67`), and the
CLI runs the steps in order. The CLI declares its command modules in the
`mod cmd` block of `crates/teamwire-cli/src/main.rs:16-60`.

## Order of work

1. **US1 first.** US2's text names `teamwire amend`, and the skill test
   (`crates/teamwire-cli/tests/preset.rs:484`) fails when the skill names a
   command that does not exist. Inside US1: store setter, then
   `send_with_write`, then the verify flag, then the close rule, then
   `rewrite_contract`, then the CLI command, then the guidance.
2. **US2** after US1's command exists.
3. **US3** is independent of both. It touches `SKILL.md` lines 282-288 only,
   which US1 and US2 do not touch.

All three stories share `examples/skills/teamwire/SKILL.md` and
`reference.md`, in different sections, so one implementer runs them in
order.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

None.
