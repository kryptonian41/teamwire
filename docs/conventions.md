# Repository conventions

How the teamwire repository is laid out, which gates run in which order, the
lint and format configuration, and the rules every ticket brief restates.
The wave-1 skeleton ticket (T01 in `docs/build-plan.md`) creates the files
here; T20 completes the structure checker. `CLAUDE.md` belongs to the lead;
section 7 is the proposed text for the lead to apply.

Sources: the `repopract` report (`reports/repopract.md`, requirements R1-R38,
and its findings m014 and m015, 2026-09-29), the rust report, and the
project's own `CLAUDE.md`. What was adopted from repopract and what was not
is listed in section 8.

The principle behind every section: **a rule that must always hold is
checked by a program**. Prose explains the rule; it is never the only thing
enforcing it (repopract R5).

## 1. Repository tree

```
teamwire/
├── AGENTS.md                  shared agent rules and commands (lead-owned)
├── CLAUDE.md                  one line: @AGENTS.md (lead-owned)
├── README.md
├── Cargo.toml                 virtual manifest: members, [workspace.package],
│                              [workspace.dependencies], [workspace.lints], profiles
├── Cargo.lock                 committed; every gate step uses --locked
├── rust-toolchain.toml        pinned compiler, rustfmt, clippy
├── rustfmt.toml               two lines, stable options only
├── clippy.toml                disallowed methods with reasons; test allowances
├── .cargo/config.toml         the `cargo xtask` alias
├── docs/                      architecture, ADRs, API, plans (this directory)
├── scripts/
│   ├── check-docs.sh          architecture-phase docs check (lead)
│   └── gate.sh                the full gate, in order; `gate.sh fast` for steps 1-3
├── xtask/                     `cargo xtask tidy`: structure, layering, protected files
├── crates/                    flat; folder name equals crate name
│   ├── teamwire-core/         types, traits, sanitiser; no I/O
│   ├── teamwire-store/        SQLite; src/schema/v1.sql, v2.sql
│   ├── teamwire-platform/     the only crate that spawns processes, reads the
│   │                          clock, or writes files outside SQLite
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
    ├── skills/teamwire/SKILL.md   the lead's skill; `teamwire install` embeds it
    ├── presets/*.json             the six team shapes
    └── mcp/lead.json              the lead MCP server, which `teamwire install`
                                   registers for the user; never merged into
                                   a project's .mcp.json (ADR 0012)
```

Layout rules, each checked by `cargo xtask tidy`:

| Rule | Source |
| --- | --- |
| Every crate but `xtask` is named `teamwire-<x>` and sits in a folder of the same name; `version = "0.0.0"`, `publish = false` | repopract R21 |
| Every member has `[lints] workspace = true` | repopract R9 |
| Every dependency in a member manifest is `workspace = true` | repopract R25 |
| No `mod.rs` files. A source file over 400 non-test lines is reported; over 800 fails | repopract R27 |
| Every crate's `lib.rs` or `main.rs` opens with a `//!` comment of at most 15 lines: purpose, what it must never do, invariants. `ARCHITECTURE.md`'s Components table has one row per crate under `crates/`, and no row without a crate | repopract R28 |
| `CLAUDE.md`'s first line is `@AGENTS.md`; `AGENTS.md` is under 120 lines | repopract R1, R3 |
| The crate dependency graph follows the layers in section 5 | repopract R22 |
| With `--base <ref>`: any change to a protected file prints `PROTECTED-CHANGED <path>`; a removed `#[test]`; a new `#[ignore]`, `#[expect(`, `#[allow(` or `#[should_panic`; a removed or changed `#[cfg(test)]` or `mod tests;` line; or an added `#[cfg_attr(..., ignore | allow | expect | should_panic)]` prints `WEAKENED <path>:<line>`. The adapter check skips comments and string literals | repopract R17, R18 |

Unit tests go in a `#[cfg(test)] mod tests` at the bottom of the file, or,
when the file would pass 400 lines, in a sibling file declared with
`#[cfg(test)] mod tests;` (`src/<file>/tests.rs`), which keeps access to
private items. tidy counts `src/**/tests.rs` as test code.
Integration tests go in `crates/<crate>/tests/<topic>.rs` and use only the
crate's public API. CLI tests go in `crates/teamwire-cli/tests/`.

## 2. Gates, in order

`scripts/gate.sh` runs these in this order, prints one line per step with its
wall time, and stops at the first failure. `scripts/gate.sh fast` runs steps
1-3.

| # | Command | What it stops |
| --- | --- | --- |
| 1 | `cargo fmt --all -- --check` | Format changes |
| 2 | `cargo xtask tidy --base main` | The layout rules of section 1 and layering. `PROTECTED-CHANGED` and `WEAKENED` lines are printed without failing the step; the gate shows them even when the step passes, so the reviewer sees them |
| 3 | `cargo clippy --workspace --all-targets --locked -- -D warnings` | The lint table, disallowed methods, suppressions without a reason |
| 4 | `TEAMWIRE_FAKES_PREBUILT=1 cargo test --workspace --locked` | Behaviour (unit, property, contract, crash, end-to-end) |
| 5 | `RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps --locked` | Broken doc links |
| 6 | `scripts/check-docs.sh` | Required documents still present |

Doc examples are not compiled as tests. Each library crate sets `doctest =
false` under `[lib]` in its `Cargo.toml`, because no crate has a doctest and
compiling the empty sets cost an estimated 12 s in step 4. A crate that wants doctests
removes its `doctest = false` line. An explicit `cargo test --doc` still
starts the doctest runner for every crate.

`--locked` makes any build that would change `Cargo.lock` fail, so no ticket
changes dependencies by accident. `-D warnings` goes on the clippy call, not
in `RUSTFLAGS`, so the build cache is not thrown away.

A ticket runs its build-plan **Check** and `scripts/gate.sh fast`. The lead
runs the full `scripts/gate.sh` once after integrating each wave.

`scripts/test-changed.sh` is the test run for the inner loop. Given crate
names, or by default the crates with files that differ from `main` (committed,
staged, unstaged and untracked), it tests those crates and every workspace
crate that depends on them, dev-dependencies included. The end-to-end and
crash test crates count as dependents. It builds the tests, the fakes and the
CLI binary in one command, then runs `cargo test --locked -p ...` with
`TEAMWIRE_FAKES_PREBUILT=1`. `--dry-run` prints the package list and the
commands without building. The full gate still runs at the end of each wave.

The gate sets `TEAMWIRE_FAKES_PREBUILT=1` because the workspace build has
already made the fake binaries. Without the variable, each test process runs
`cargo build` for them again. That build makes a second copy of the fakes with
other dependency features, and cargo replaces the files every time, even when
nothing changed (`tests/fakes/src/bins.rs`). The end-to-end and crash tests also use
`target/<profile>/teamwire` without building it when the variable is set. So
whoever sets the variable must have built the whole workspace first, including
the CLI binary; `scripts/test-changed.sh` does that. A run that forgot fails
at once: each binary is checked against the dep-info file cargo wrote beside
it, and one older than any of its source files is refused, with the file
named (`teamwire_fakes::teamwire_cli` and `FakeBins::ensure`). A plain `cargo test -p <crate>`
leaves the variable unset and builds the fakes itself.

`cargo-nextest` 0.9.148 is installed (2026-10-10), and `.config/nextest.toml`
is ready for it, but the gate does not use it yet. nextest runs many tests at
once, each in its own process, and the end-to-end and crash tests start dozens
of processes each. Process starts on this Mac are slow: at times every start
costs about 0.25 s, even `/bin/echo`, and a newly built binary's first start
costs seconds. Under nextest those waits broke teamwire's own timeouts and the
tests' own 20 s waits, and the gate failed in a different test on each of four
runs. When it passed, step 4 took about 200 s against about 330 s with `cargo
test`. The cause of the slow starts is not proven. The candidates are the
system's program check (`syspolicyd`), a security agent, and other load on
the machine. `docs/research/2026-10-10-test-suite.md` has the measurements
and a plan. Try nextest again once a start costs milliseconds.

`cargo-deny` and `cargo-mutants` are not installed on this machine (checked
2026-09-29). repopract recommends `cargo-deny` for a licence audit;
installing it is the user's decision and is listed under the open questions
in `ARCHITECTURE.md`. If the user installs it, `cargo deny check` becomes
step 6, with the `deny.toml` of repopract section 5.4.

## 3. Toolchain and format

`rust-toolchain.toml`:

```toml
[toolchain]
channel = "1.94.1"
components = ["rustfmt", "clippy"]
profile = "minimal"
```

1.94.1 is the installed stable (rust report 1.1). `rmcp` 3.5 needs 1.88 or
newer. If rustup does not treat the installed `stable` as `1.94.1`, the first
build fetches the pinned toolchain once; T01 may do that. teamwire is a
binary, so its minimum supported Rust version is this pin, and there is no
separate MSRV job.

`rustfmt.toml`:

```toml
edition = "2024"
style_edition = "2024"
```

## 4. Workspace manifest, lints and disallowed methods

Root `Cargo.toml` (repopract m014 and section 5.4; uv `Cargo.toml` lines
413-450, ruff lines 262-317):

```toml
[workspace]
resolver = "2"
members = ["crates/*", "tests/fakes", "tests/crash", "tests/e2e", "xtask"]

[workspace.package]
edition = "2024"
rust-version = "1.94"
version = "0.0.0"
publish = false
license = "MIT OR Apache-2.0"

[workspace.lints.rust]
unsafe_code = "forbid"
unreachable_pub = "warn"
unused_must_use = "deny"
unexpected_cfgs = "warn"
missing_debug_implementations = "warn"

[workspace.lints.clippy]
# Base: pedantic, as in uv and ruff, with their noisy lints allowed.
pedantic = { level = "warn", priority = -2 }
missing_errors_doc = "allow"
missing_panics_doc = "allow"
module_name_repetitions = "allow"
must_use_candidate = "allow"
similar_names = "allow"
too_many_lines = "allow"
if_not_else = "allow"
map_unwrap_or = "allow"
match_same_arms = "allow"
implicit_hasher = "allow"
# Stability: no panics or silent drops in library code.
unwrap_used = "warn"
expect_used = "warn"
panic = "warn"
todo = "warn"
unimplemented = "warn"
indexing_slicing = "warn"
let_underscore_must_use = "warn"
await_holding_lock = "deny"
# Suppression discipline: #[expect(lint, reason = "...")] only.
allow_attributes = "deny"
allow_attributes_without_reason = "deny"
# A new enum variant forces a decision at every match.
wildcard_enum_match_arm = "warn"
# Hygiene.
dbg_macro = "warn"
print_stdout = "warn"
print_stderr = "warn"
exit = "warn"
get_unwrap = "warn"
use_self = "warn"

[profile.dev]
debug = "line-tables-only"
```

`unsafe_code = "forbid"`: teamwire needs no unsafe code. Reading another
process's environment (for the locator) uses the `sysinfo` crate's
`Process::environ`, which does the system calls inside that crate. T03
confirms it works for a same-user process on macOS and stops to ask the lead
if it does not.

`clippy.toml` (repopract R32, adapted to this design's crate names):

```toml
allow-unwrap-in-tests = true
allow-expect-in-tests = true
disallowed-methods = [
  { path = "std::fs::write",             reason = "files outside SQLite go through Platform::write_atomic" },
  { path = "std::fs::rename",            reason = "files outside SQLite go through Platform::write_atomic" },
  { path = "std::fs::File::create",      reason = "files outside SQLite go through Platform::write_atomic" },
  { path = "std::process::Command::new", reason = "processes start only through Platform::run, so tests cannot start real herdr or Claude" },
  { path = "std::time::SystemTime::now", reason = "use the Clock trait so the watch is testable" },
  { path = "std::time::Instant::now",    reason = "use the Clock trait so the watch is testable" },
  { path = "std::env::set_var",          reason = "process-wide state; pass configuration explicitly" },
]
```

Only `teamwire-platform` (the real `Platform` and `Clock`), `tests/fakes`,
`tests/crash`, `tests/e2e` and `xtask` may call these, each call site under
`#[expect(clippy::disallowed_methods, reason = "...")]`. T01 runs clippy once
and fixes any option name clippy rejects (repopract marked two names as
unverified; those two are left out above).

## 5. Crate layers

A crate may depend only on crates in lower layers. `cargo xtask tidy` reads
`cargo metadata` and fails on any other edge (repopract R22).

| Layer | Crates | May depend on |
| --- | --- | --- |
| 0 | `teamwire-core` | external crates only |
| 1 | `teamwire-store`, `teamwire-platform` | 0 |
| 2 | `teamwire-host-herdr`, `teamwire-host-tmux`, `teamwire-harness-claude` | 0, `teamwire-platform` |
| 3 | `teamwire-deliver`, `teamwire-verify` | 0, 1 |
| 4 | `teamwire-msg`, `teamwire-report`, `teamwire-launch`, `teamwire-watch`, `teamwire-otel`, `teamwire-close` | 0, 1, 3. No layer-4 crate depends on another; `teamwire-close` reads what launch recorded from the store |
| 5 | `teamwire-mcp` | 0, 1, 3, 4 |
| 6 | `teamwire-cli` | everything above, and the layer-2 adapters (only in `wiring.rs`) |
| test | `teamwire-fakes` | 0 |
| test | `teamwire-crash-tests`, `teamwire-e2e` | anything; they run the binary |

Service crates never depend on an adapter crate. They receive adapters as
`&dyn Host`, `&dyn Harness`, `&dyn Platform` in `Ctx`. `anyhow` appears only in
`teamwire-cli` and `xtask`; library crates use `thiserror` (repopract R30).
`cargo xtask tidy` enforces both rules: the adapter crates may be named in
`teamwire-cli` only in `src/wiring.rs` (`xtask/src/adapters.rs`), and only
`teamwire-cli` and `xtask` may depend on `anyhow` (`xtask/src/layers.rs`).

## 6. Rules every ticket brief restates

The standing rules are in `AGENTS.md`. Each brief restates these, filled in
for its ticket (repopract section 5.5):

1. **Owns**: the exact paths from the build-plan row. You own no shared file
   unless it is listed. Anything else goes to the lead as a question.
2. **Check**: the row's command, already run once by the lead on the base
   commit, plus `scripts/gate.sh fast`. Done means both pass on your last
   commit, with the real output in your report.
3. **Contract**: match `docs/api/core.md` exactly. Changing a trait or a type
   in `teamwire-core` is a question for the lead.
4. **Dependencies and `pub` items**: add no dependency and do not touch
   `Cargo.lock`. List every new `pub` item in your report.
5. **Protected files**: none, unless the row lists one.
6. **Tests**: every test named for your paths in `docs/failure-modes.md`,
   plus a test that fails without each behaviour you add. For store and
   delivery code, the failure-path tests come first.
7. **Fakes**: tests use `teamwire_fakes::TestHome`, the in-process fakes, the
   fake binaries and `FakeClock`. Never the real `~/.claude/`, never real
   herdr or Claude.
8. **Never weaken a check**: no `#[ignore]`, no deleted test, no
   `#[expect]` to get green, no edit to lint, format, toolchain or gate files.
9. **One coherent change**; commit after each unit; one progress line per
   commit in your report; no reformatting outside your paths.
10. **Stop rule**: two fix rounds on a failing check, then report `failed`.
11. **Report**: commits, checks run with their output, decisions you made,
    new `pub` items.

## 7. Proposed `AGENTS.md` and `CLAUDE.md`

Instruction files help only with rules an agent cannot read from the code, and
they cost inference on every turn; the architecture belongs in `docs/`
(repopract m015, F1; the ETH Zurich study it cites, arXiv 2602.11988, is
second-hand here). Claude Code ignores `AGENTS.md` when a `CLAUDE.md` exists,
so `AGENTS.md` is the one copy that counts and `CLAUDE.md` imports it
(repopract R3, F6). The lead should confirm the `@AGENTS.md` import form
against `https://code.claude.com/docs/en/memory` before applying it. Each
"never" line names the program that enforces it (repopract R5).

`CLAUDE.md`:

```markdown
@AGENTS.md
```

`AGENTS.md` (the separate-project block is today's `CLAUDE.md`, kept word
for word in meaning):

```markdown
# teamwire

Rust tool that runs teams of coding agents. Map: docs/ARCHITECTURE.md.
Your ticket: docs/build-plan.md. Interfaces: docs/api/. Conventions: docs/conventions.md.

## Separate project: never touch the running setup

The user's live Claude Code setup runs on this machine, and teamwire runs
the user's real agent teams. So:
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
- Your ticket's check: the Check column of your row in docs/build-plan.md
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
- Do not edit protected files unless your ticket owns them: root Cargo.toml,
  clippy.toml, rustfmt.toml, rust-toolchain.toml, scripts/, xtask/,
  tests/fakes/, docs/adr/. [tidy --base prints PROTECTED-CHANGED]
- Never delete, #[ignore] or loosen a test to get green. Fix the code.
  [tidy --base prints WEAKENED; teamwire verify flags it]

## Where things go

- One crate per concern under crates/. Each lib.rs starts with a short //!
  note: purpose, what it must never do, invariants. Read it first.
- No mod.rs. Split a source file past 400 lines; tidy fails past 800.
- Unit tests at the bottom of the file; integration tests in crates/<crate>/tests/.
- Extend the nearest existing helper instead of adding a parallel one.

## Comments

- Explain why, not what, for a reader who never saw this session.
- No conversation or ticket history in code. Change an ADR only by adding one
  that supersedes it.
```

## 8. What was adopted from repopract

| repopract item | Adopted? | Where, or why not |
| --- | --- | --- |
| R1-R3 short `AGENTS.md`, `CLAUDE.md` imports it | Yes | §7; tidy checks |
| R4 nested instruction files | Not now | No crate has a rule that needs one yet |
| R5 every rule has a program | Yes | §7 brackets |
| R7, R8 one gate script with a fast mode | Yes | §2 (`scripts/gate.sh`, `fast`) |
| R9 lints opt-in checked | Yes | tidy |
| R10, R14, R36 nextest and cargo-deny in the gate | No | nextest is installed but not in the gate: parallel tests overload process starts on this Mac (§2); cargo-deny is not installed, a user decision (`ARCHITECTURE.md` open questions) |
| R11 clippy with `-D warnings` on the call | Yes | §2 |
| R12 `#[expect]` with reason only | Yes | §4 |
| R13 `Cargo.lock` committed, `--locked` | Yes | §2 |
| R15 pre-commit hook | No | A git hook is outside the files a ticket owns; the fast gate covers it |
| R16 pinned toolchain | Yes | §3 |
| R17, R18 protected files and weakened tests | Yes | tidy `--base`; also teamwire's own verify (VER-3, VER-4) |
| R19 snapshot policy | Not needed | No snapshot tests planned |
| R20 cargo-mutants | Not now | Not installed; noted in `docs/test-strategy.md` as a later step |
| R21-R25 flat crates, layering, one crate per adapter, scaffold declares deps | Yes | §1, §5; T01 |
| R26 lockfile conflicts | Yes | The lead regenerates `Cargo.lock` from the base file at integration, never by hand |
| R27, R28 file sizes, `//!` notes, doc map | Yes | tidy |
| R29 `xtask` | Yes | T01 (minimal), T20 (full) |
| R30 thiserror, anyhow only in CLI | Yes | §5 |
| R31 `unreachable_pub`, `unsafe_code = forbid` | Yes | §4 |
| R32 disallowed methods | Yes, adapted | §4: writes and spawns go through `teamwire-platform`, not the store |
| R33 no Cargo features | Yes | No `[features]` tables in v1 |
| R34 `teamwire-testkit` crate for in-process fakes | No | `CLAUDE.md` names `tests/fakes/`; one crate there holds both the in-process fakes and the fake binaries |
| R35 failure-path tests first | Yes | §6 item 6 |
| R37 `debug = "line-tables-only"` | Yes | §4 |
| R38 briefs restate conventions | Yes | §6 |
