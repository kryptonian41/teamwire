# teamwire

Rust tool that runs teams of coding agents. Map: docs/ARCHITECTURE.md.
Your task: your brief. A member with a worktree finds it as TASK.md there; any
other member finds it at the path its first message names, under
$TEAMWIRE_HOME/teams/<team>/briefs/ (docs/build-plan.md is the closed first build). Interfaces: docs/api/. Conventions: docs/conventions.md.

## Separate project: never touch the running setup

The user's live Claude Code setup runs on this machine, and teamwire now
runs the user's real agent teams. So:
- Write only inside this repository and temporary directories you create.
  Never create, edit, move or delete anything under ~/.claude/ or ~/.claude.json.
- Never register a hook, MCP server, skill or launch agent outside this repo.
  Example configs live in examples/ as files.
- teamwire agent names are tw_<team>_<role> and never contain a hyphen
  (INV-32). Do not change that.
- teamwire's state lives in $TEAMWIRE_HOME (default ~/.teamwire).
  Tests set it to a temp dir with teamwire_fakes::TestHome.
- Tests never type into a real herdr pane or start a real Claude session.
  Use the fakes in tests/fakes/. (clippy: Command::new is disallowed
  outside teamwire-platform and the test crates)

## Commands (from the repo root)

- While working: cargo test -p <your crate> --locked, then scripts/gate.sh fast
- Before you finish: scripts/test-changed.sh tests your changed crates
  (against $TEAMWIRE_BASE, else main, else origin/main) and every crate that
  depends on them (--dry-run lists them first)
- Your check: the command in your brief
- Full gate (the lead, after each wave): scripts/gate.sh
- GNU tools on this Mac carry a g prefix: gsed -i, gdate, gtimeout.
  Plain sed -i is BSD sed.

## Hard rules (the program that enforces each is in brackets)

- Every store write runs inside Store::write, which starts BEGIN IMMEDIATE.
  No write transaction stays open during an external call. [review; crash tests]
- Files outside SQLite are written with Platform::write_atomic. [clippy]
- Time comes from the Clock trait. [clippy]
- Host::submit takes a OneLine, built with sanitize::one_line. [type system]
- Only teamwire-cli prints or exits. Libraries return thiserror errors;
  anyhow only in teamwire-cli and xtask. [clippy; tidy]
- No unwrap, expect, panic or todo outside tests. [clippy]
- Suppress a lint only with #[expect(lint, reason = "...")]. [clippy]
- A crate depends only on lower layers (docs/conventions.md section 5). [tidy]
- Do not add dependencies or touch Cargo.lock; ask the lead. [--locked]
- Do not edit protected files unless your brief owns them: root Cargo.toml,
  Cargo.lock, clippy.toml, rustfmt.toml, rust-toolchain.toml,
  .cargo/config.toml, scripts/, xtask/, tests/fakes/, docs/adr/.
  [tidy --base prints PROTECTED-CHANGED, also for a protected file moved away
  or added untracked]
- Never delete, #[ignore], move or loosen a test to get green. Fix the code.
  [tidy --base prints WEAKENED, also for a net-removed assert, an added
  non-test #[cfg(, or a test file moved; teamwire verify flags it]

## Where things go

- One crate per concern under crates/. Each lib.rs starts with a short //!
  note: purpose, what it must never do, invariants. Read it first.
- No mod.rs. Split a source file past 400 lines; tidy fails past 800.
- Unit tests at the bottom of the file; integration tests in crates/<crate>/tests/.
- Extend the nearest existing helper instead of adding a parallel one.
- ui/ is a separate Bun and TypeScript project, the read-only dashboard.
  Its rules and commands are in ui/AGENTS.md. Its zod schemas copy the
  CLI's --json output, so a change to that output updates
  ui/src/shared/teamwire.ts in the same commit.

## Comments

- Explain why, not what, for a reader who never saw this session.
- No conversation or ticket history in code. Change an ADR only by adding one
  that supersedes it.
