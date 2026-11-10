# Quickstart: check each story

The run guide for whoever checks the finished branches, such as the closer.
Each story's tests are listed in [research.md](research.md). Expected
output is in [contracts/cli.md](contracts/cli.md).

## Prerequisites

- A worktree on the implementer's branch, with `cargo fetch --locked` done.
- No real herdr or tmux pane and no real agent session. Every test runs on
  `tests/fakes/` and `TestHome`.

## US1: delivery and member state

```bash
cargo test -p teamwire-report --locked
cargo test -p teamwire-msg --locked
cargo test -p teamwire-watch --locked
cargo test -p teamwire-launch --locked
cargo test -p teamwire-harness-claude --locked --test lead_setup
cargo test -p teamwire-cli --locked --test status
```

Expected results:

- The hook-note tests show `was never typed into your pane` for Claude
  Code, Codex and pi.
- `received <id>` clears the row with no typing.
- The watch prints no `TEAM-COMPLETE` while a member with an undelivered
  result works.
- A note to a member with a `done` result starts a generation.
- A note to a member with a `handoff:` result is held.

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

Manual check for SC-005. Do not make this a test, because it reads another
repository. This needs the knit-3.0 checkout at deckh's base and deckh's
`tasks.md`.

```bash
teamwire speckit draft /Users/apoorvagarwal/Projects/knit-3.0/specs/<deckh feature>/tasks.md
```

The drafts should list these three files:

- `apps/ai/.importlinter`
- `apps/ai/tests/test_harness_targets.py`
- `.github/workflows/evals-harness-replay.yml`

The phase table should match the four phase pairs in the findings file.

## US3: small fixes

```bash
cargo test -p teamwire-cli --locked --test preset
cargo test -p teamwire-close --locked --test ignored_files
cargo test -p teamwire-harness-claude --locked --test sessions
cargo test -p teamwire-harness-codex --locked --test session
cargo test -p teamwire-harness-pi --locked --test session
cargo test -p teamwire-watch --locked --test status
```

F10 also changes the dashboard schema. Run the UI checks that
`ui/AGENTS.md` names for a change to `ui/src/shared/teamwire.ts`.

## Before finishing

```bash
scripts/test-changed.sh
```

The lead runs the full gate, `scripts/gate.sh`, once after the wave.
