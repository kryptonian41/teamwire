# teamwire Constitution

## Core Principles

### I. Root Cause First
Each fix names its root cause with file:line evidence before any code changes.

### II. A Failing Test First
Every fix has a test that fails without the fix. The closer proves it by reverting the fix.

### III. Guidance Changes With The Code
The lead skill (`examples/skills/teamwire/`), the presets (`examples/presets/`), the docs (`docs/`) and the README change in the same commit as the code they describe. A change to any `--json` output also updates `ui/src/shared/teamwire.ts` in that commit.

### IV. AGENTS.md's Hard Rules Hold
Every store write runs in `Store::write`. Files outside SQLite are written with `Platform::write_atomic`. Time comes from the `Clock` trait. `Host::submit` takes a `OneLine`. Only `teamwire-cli` prints or exits. No `unwrap`, `expect`, `panic` or `todo` outside tests. A crate depends only on lower layers. No new dependency, and no edit to `Cargo.lock`.

### V. Every Harness
A lead or member feature ships for Claude Code, Codex and pi through the `Harness` trait, never for Claude Code alone.

### VI. Fakes Only In Tests
Tests never type into a real herdr or tmux pane and never start a real agent session. They use `tests/fakes/` and `teamwire_fakes::TestHome`.

### VII. The Smallest Complete Change
Reuse the nearest existing helper. An ADR changes only by a new ADR that supersedes it.

## Governance

Only the user amends this constitution. A spec or plan that conflicts with it is wrong, not the constitution.

**Version**: 1.0.0 | **Ratified**: 2026-11-10 | **Last Amended**: 2026-11-10
