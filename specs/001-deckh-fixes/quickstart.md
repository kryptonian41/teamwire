# Quickstart: check each story

The run guide for whoever checks the finished branches, such as the closer.
Each story's tests are listed in [research.md](research.md). Expected
output is in [contracts/cli.md](contracts/cli.md).

## Prerequisites

- A worktree on the implementer's branch, with `cargo fetch --locked` done.
- No real herdr or tmux pane and no real agent session. Every test runs on
  `tests/fakes/` and `TestHome`.

## Installing the new build

US1 moves the store to schema version 11. The upgrade only goes forward: a
store the new build has opened cannot be opened by an older build. Install
the new build only after team `deckh` closes. Until then, run the checks
below from the worktree with `cargo test` and `cargo run`, never an
installed `teamwire`.

## US1: delivery and member state

```bash
cargo test -p teamwire-store --locked
cargo test -p teamwire-report --locked
cargo test -p teamwire-msg --locked
cargo test -p teamwire-watch --locked
cargo test -p teamwire-otel --locked
cargo test -p teamwire-launch --locked
cargo test -p teamwire-harness-claude --locked
cargo test -p teamwire-cli --locked --test status --test hook_lead
cargo test -p teamwire-cli --locked --bin teamwire status
(cd ui && bun test src/shared src/web/model && bun run typecheck)
```

Expected results:

- The hook-note tests show `was never typed into your pane` for Claude
  Code, Codex and pi.
- `received <id>` clears the row with no typing.
- The watch prints no `TEAM-COMPLETE` while a member with an undelivered
  result works.
- A note to a member with a `done` result starts a generation, written as
  `member.woken`.
- A note to a member with a `handoff:` result is held, and `status` lists
  it as `held for handoff: <id> -> <role>`.
- A version 10 store opens at version 11 and keeps every event.

Some tests in this story are guard tests: they pass before and after the
fix. tasks.md marks each one. Revert a fix only to check the tests marked
failing.

Read-only check for SC-001 and SC-003, done by the closer: replay deckh's
sequence on the fakes. Block the lead pane, have a member send a question
and record a result, run the lead's hook, and run a watch while that member
works. The hook must name both files at the next tool call, and the watch
must print no `TEAM-COMPLETE` while the member works.

To check that each test fails without its fix, revert the fix commit in a
clone and run the same test.

## US2: the `tasks.md` reader

```bash
cargo test -p teamwire-cli --locked --test speckit_draft
cargo run -p teamwire-cli --locked -- speckit draft specs/001-deckh-fixes/tasks.md
```

Expected results:

- The second command prints the shared files between phases.
- The second command prints `Stories that share files`.
- The second command prints `DRAFT` before the owns lists.

Read-only check for SC-005, done by the closer. Do not make this a test,
because it reads another repository. This needs the knit-3.0 checkout at deckh's base and deckh's
`tasks.md`.

```bash
teamwire speckit draft /Users/apoorvagarwal/Projects/knit-3.0/specs/<deckh feature>/tasks.md
```

The drafts should list these three files:

- `apps/ai/.importlinter`
- `apps/ai/tests/test_harness_targets.py`
- `.github/workflows/evals-harness-replay.yml`

The phase table should hold all four phase pairs of the findings file's
F4 table, with the shared files that deckh's `tasks.md` names. Files that
only `impl`'s commits touched are not expected.

## US3: small fixes

```bash
cargo test -p teamwire-cli --locked --test preset
cargo test -p teamwire-launch --locked --lib contract
cargo test -p teamwire-close --locked --test ignored_files
cargo test -p teamwire-harness-claude --locked --test sessions
cargo test -p teamwire-harness-codex --locked --test session
cargo test -p teamwire-harness-pi --locked --test session
cargo test -p teamwire-watch --locked --test status
```

F10 also changes the dashboard schema. Run the UI checks that
`ui/AGENTS.md` names for a change to `ui/src/shared/teamwire.ts`. The CLI's
`tool:` line has a unit test in `crates/teamwire-cli/src/cmd/status.rs`.

## US4: waivers

```bash
cargo test -p teamwire-verify --locked --test verify
```

Expected results:

- After a commit that leaves the waived file alone, the file's flags print
  `(waived)` and the verdict is `PASS`.
- After a commit or merge that changes the waived file, the flags are not
  waived.
- A waiver for `no-owns` or `uncommitted:<path>` ends at the next commit.

## Before finishing

```bash
scripts/test-changed.sh
```

The lead runs the full gate, `scripts/gate.sh`, once after the wave.
