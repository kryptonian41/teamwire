# Repository conventions

How the teamwire repository is laid out, which gates run in which order,
the lint and format configuration, and the rules every ticket brief restates.
The wave-1 skeleton ticket (T01 in `docs/build-plan.md`) creates everything
here. `CLAUDE.md` belongs to the lead; section 6 is the proposed text for the
lead to apply.

Sources: the `repopract` investigator's findings m014 and m015 (2026-09-29),
the rust report, and the project's own `CLAUDE.md`.

## 1. Repository tree

```
teamwire/
├── Cargo.toml                 workspace root: members, [workspace.package],
│                              [workspace.dependencies], [workspace.lints]
├── Cargo.lock                 committed; changed only by T01 or with the lead's approval
├── rust-toolchain.toml        pinned toolchain
├── rustfmt.toml               edition and style edition only
├── CLAUDE.md                  one line: @AGENTS.md (lead-owned)
├── AGENTS.md                  commands and non-obvious rules (lead-owned)
├── README.md
├── docs/                      architecture, ADRs, API, plans (this directory)
├── scripts/
│   ├── check-docs.sh          architecture-phase docs check (lead)
│   └── gate.sh                the full gate, in order (created by T01)
├── crates/
│   ├── teamwire-core/         types, traits, sanitiser; no I/O
│   ├── teamwire-store/        SQLite; src/schema/v1.sql
│   ├── teamwire-platform/
│   ├── teamwire-host-herdr/
│   ├── teamwire-host-tmux/
│   ├── teamwire-harness-claude/
│   ├── teamwire-deliver/
│   ├── teamwire-msg/
│   ├── teamwire-report/
│   ├── teamwire-launch/       contracts/*.md: contract text as data
│   ├── teamwire-verify/       rules/*.toml: per-language verify rules
│   ├── teamwire-close/
│   ├── teamwire-watch/
│   ├── teamwire-otel/
│   ├── teamwire-mcp/
│   └── teamwire-cli/          the `teamwire` binary; src/cmd/<subcommand>.rs
├── tests/
│   ├── fakes/                 crate teamwire-fakes: FakeHost, FakeHarness,
│   │                          FakePlatform, FakeClock, TestHome, contract
│   │                          suites; bins fake-herdr, fake-claude
│   ├── crash/                 crate teamwire-crash-tests
│   └── e2e/                   crate teamwire-e2e
└── examples/
    ├── skills/teamwire/SKILL.md   the lead's skill, for the user to install
    ├── presets/*.json             the six team shapes
    └── mcp/lead.json              lead MCP config, for the user to install
```

Each crate keeps its tests in `crates/<crate>/tests/<topic>.rs` (integration)
or in a `#[cfg(test)] mod tests` beside the code (unit). A crate's
`Cargo.toml` names only `workspace = true` dependencies.

## 2. Gates, in order

`scripts/gate.sh` runs these in this order and stops at the first failure:

| # | Command | Why this position |
| --- | --- | --- |
| 1 | `cargo fmt --all -- --check` | Cheapest; fails in a second |
| 2 | `cargo clippy --workspace --all-targets --locked -- -D warnings` | Catches most mistakes before tests run |
| 3 | `cargo test --workspace --locked` | Unit, property, contract, crash and end-to-end tests |
| 4 | `cargo doc --workspace --no-deps --locked` with `RUSTDOCFLAGS=-D warnings` | Broken doc links in the public API |
| 5 | `scripts/check-docs.sh` | Required documents still present |

`--locked` makes a build that would change `Cargo.lock` fail, so no ticket
changes dependencies by accident.

A ticket runs only the check in its build-plan row, which is always a
targeted `cargo test -p <crate> ...`. The lead runs `scripts/gate.sh` once
after integrating each wave.

`cargo-nextest` and `cargo-deny` are not installed on this machine
(checked 2026-09-29), so v1 does not depend on them. Adding either is a user
decision, since it installs a tool.

## 3. Toolchain and format

`rust-toolchain.toml`:

```toml
[toolchain]
channel = "1.94.1"
components = ["rustfmt", "clippy"]
```

1.94.1 is the installed stable (rust report 1.1). `rmcp` 3.5 needs 1.88 or
newer. The first build fetches this pinned toolchain once through rustup; T01
does that.

`rustfmt.toml` (repopract m014: uv and ruff keep only these):

```toml
edition = "2024"
style_edition = "2024"
```

## 4. Workspace manifest and lints

Root `Cargo.toml` (repopract m014; uv `Cargo.toml` lines 413-450, ruff lines
262-317):

```toml
[workspace]
resolver = "2"
members = ["crates/*", "tests/fakes", "tests/crash", "tests/e2e"]

[workspace.package]
edition = "2024"
rust-version = "1.94"
license = "MIT OR Apache-2.0"
publish = false

[workspace.lints.rust]
unsafe_code = "deny"
unreachable_pub = "warn"
missing_debug_implementations = "warn"

[workspace.lints.clippy]
pedantic = { level = "warn", priority = -2 }
# allowed on purpose; each line says why
module_name_repetitions = "allow"   # crate-prefixed names read better in docs
missing_errors_doc = "allow"        # every error enum is documented at its type
must_use_candidate = "allow"        # too noisy on small getters
# stricter than pedantic
print_stdout = "warn"               # only teamwire-cli prints; it allows this locally
print_stderr = "warn"
dbg_macro = "warn"
exit = "warn"                       # only teamwire-cli's main may exit
get_unwrap = "warn"
unwrap_used = "warn"                # tests allow it with #[allow] at module level
expect_used = "warn"
```

`unsafe_code` is `deny`, stricter than uv's `warn`: teamwire needs no unsafe
code. Reading another process's environment (for the locator) uses the
`sysinfo` crate's `Process::environ`, which does the unsafe system calls
inside that crate. T03 confirms it returns the environment of a same-user
process on macOS. If it does not, the fallback is one module in
`teamwire-platform` with a lead-approved `#[allow(unsafe_code)]` and a comment
saying why.

Every crate's `Cargo.toml` has:

```toml
[lints]
workspace = true
```

Dependencies are declared once, in `[workspace.dependencies]`, with exact
pins for the crates that move fast (`rmcp = "=3.5.0"`, and the OpenTelemetry
set pinned to one 0.33.x together). The list is in the T01 row of the build
plan.

## 5. Rules every ticket brief restates

1. Edit only the paths in your row's **Owns** column. Anything else goes to
   the lead as a question.
2. Add no dependency and do not change `Cargo.lock`. If you need a crate,
   ask the lead.
3. Match `docs/api/core.md` exactly. A signature change is a contract change
   for the lead.
4. Every behaviour you add has a test that fails without it. The tests named
   for your crate in `docs/failure-modes.md` are required.
5. Never add `#[ignore]`, `#[should_panic]` to hide a failure, `#[allow(...)]`
   to silence a lint, or edit lint, format, toolchain or CI files. The verify
   step flags each of these.
6. Tests use `teamwire_fakes::TestHome` (a temporary `TEAMWIRE_HOME`), the
   fakes, and `FakeClock`. Never the real `~/.claude/`, never real herdr or
   Claude.
7. Library crates return typed errors (`thiserror`) and never print. Only
   `teamwire-cli` prints and chooses exit codes.
8. Every call to an external program has a timeout, and runs through
   `teamwire-platform`'s `run`.
9. No write transaction stays open during an external call.
10. On this Mac GNU tools carry a `g` prefix: `gsed -i`, `gdate`, `gtimeout`.
    Plain `sed -i` is BSD sed.
11. Run only your row's **Check**. Do not run the full gate; the lead does.
12. Commit after each unit, and write one progress line per commit in your
    report file.

## 6. Proposed `AGENTS.md` and `CLAUDE.md`

repopract m015: context files help only with non-standard practice and cost
inference on every turn; keep them to commands and non-obvious rules, and put
the architecture in `docs/`. The ETH Zurich study it cites (arXiv 2602.11988)
is second-hand here; I did not read it. The current `CLAUDE.md` is 36 lines,
and the proposal stays near that size.

Claude Code reads `CLAUDE.md`; Codex CLI and others read `AGENTS.md`. The
proposal keeps one copy that counts, `AGENTS.md`, and a `CLAUDE.md` that
imports it with Claude Code's `@path` import. The lead should confirm the
import form against `https://code.claude.com/docs/en/memory` before applying.

`CLAUDE.md`:

```markdown
@AGENTS.md
```

`AGENTS.md`:

```markdown
# teamwire

Rust tool that runs teams of coding agents. Design: docs/ARCHITECTURE.md.
Your ticket: docs/build-plan.md. Interfaces: docs/api/.

## Separate project: never touch the running setup

A Python version of this tooling runs the user's real agent teams. So:
- Write only inside this repository and temporary directories you create.
  Never create, edit, move or delete anything under ~/.claude/ or ~/.claude.json.
- Never register a hook, MCP server, skill or launch agent outside this repo.
- State lives in $TEAMWIRE_HOME (default ~/.teamwire). Tests set it to a temp dir.
- Tests never type into a real herdr pane or start a real Claude session.
  Use the fakes in tests/fakes/.

## Commands

- Full gate (lead, after a wave): scripts/gate.sh
- Your ticket's check: the Check column of your row in docs/build-plan.md
- Format: cargo fmt --all
- Lint: cargo clippy --workspace --all-targets --locked -- -D warnings

## Rules that are not obvious

- Every store write starts with BEGIN IMMEDIATE (Store::write does it).
  Never hold a write transaction during an external call.
- Only teamwire-cli prints or exits. Libraries return typed errors.
- Host::submit takes a OneLine; build it with sanitize::one_line.
- Do not add dependencies or touch Cargo.lock; ask the lead.
- Never #[ignore] a test or #[allow] a lint to get green.
- GNU tools on this Mac are g-prefixed: gsed -i, gdate, gtimeout.
```
