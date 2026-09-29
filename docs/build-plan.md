# Build plan

The build as 20 tickets in five waves. Each ticket is a vertical slice with
its own tests, sized to fit one fresh context window. No two tickets in one
wave own the same path. Every wave has at most five implementer tickets.

How the waves run:

1. **Wave 1 is the contract.** T01 alone. The lead reviews it and commits it
   to `main` before anything else starts. It fixes the workspace, every crate's
   name and dependencies, the core types and traits of `docs/api/core.md`, the
   schema DDL, the CLI's subcommand list, and the lint, format and gate setup
   of `docs/conventions.md`, with a minimal structure checker.
2. Each later wave branches from `main` after the previous wave is merged and
   `scripts/gate.sh` passed on the merge.
3. A ticket's **Check** is the exact command that proves it. The ticket must
   also contain every test that `docs/failure-modes.md` names inside the paths
   it owns.
4. Because T01 declares every dependency, no later ticket changes
   `Cargo.toml` at the root or `Cargo.lock`. Builds use `--locked`.

## Ticket table

`cli/` below means `crates/teamwire-cli/`.

| Ticket | Wave | Blocked by | Owns | Check | Size |
| --- | --- | --- | --- | --- | --- |
| T01 Workspace skeleton and contract | 1 | none | `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `rustfmt.toml`, `clippy.toml`, `.cargo/config.toml`, `xtask/` (minimal tidy), `AGENTS.md`, `CLAUDE.md` (content from `docs/conventions.md` §6, with the lead's approval), `scripts/gate.sh`, `crates/teamwire-core/`, `crates/*/Cargo.toml` and `crates/*/src/lib.rs` skeletons of every other crate, `crates/teamwire-store/src/schema/v1.sql`, `cli/src/main.rs`, `cli/src/wiring.rs`, `cli/src/cmd/*.rs` stubs, `tests/fakes/Cargo.toml`, `tests/crash/Cargo.toml`, `tests/e2e/Cargo.toml` and their empty `src/lib.rs` | `cargo build --workspace --locked && cargo test -p teamwire-core --locked && cargo xtask tidy && cargo clippy --workspace --all-targets --locked -- -D warnings && cargo run -q -p teamwire-cli --locked -- --help` | L |
| T02 Store | 2 | T01 | `crates/teamwire-store/` | `cargo test -p teamwire-store --locked` | L |
| T03 Platform | 2 | T01 | `crates/teamwire-platform/` | `cargo test -p teamwire-platform --locked` | M |
| T04 Fakes and contract suites | 2 | T01 | `tests/fakes/` | `cargo test -p teamwire-fakes --locked` | L |
| T20 Structure checker (`xtask tidy`) | 2 | T01 | `xtask/` | `cargo test -p xtask --locked && cargo xtask tidy` | M |
| T05 herdr host adapter | 3 | T04 | `crates/teamwire-host-herdr/` | `cargo test -p teamwire-host-herdr --locked` | M |
| T06 Claude Code harness adapter | 3 | T03, T04 | `crates/teamwire-harness-claude/` | `cargo test -p teamwire-harness-claude --locked` | L |
| T07 Locator and delivery | 3 | T02, T03, T04 | `crates/teamwire-deliver/` | `cargo test -p teamwire-deliver --locked` | M |
| T08 Launch | 3 | T02, T03, T04 | `crates/teamwire-launch/`, `cli/src/cmd/launch.rs`, `cli/tests/launch.rs` | `cargo test -p teamwire-launch --locked && cargo test -p teamwire-cli --test launch --locked` | L |
| T09 Verify and the report check | 3 | T02, T03, T04 | `crates/teamwire-verify/`, `cli/src/cmd/verify.rs`, `cli/tests/verify.rs` | `cargo test -p teamwire-verify --locked && cargo test -p teamwire-cli --test verify --locked` | L |
| T10 Watch and status | 4 | T02, T04 | `crates/teamwire-watch/`, `cli/src/cmd/watch.rs`, `cli/src/cmd/status.rs`, `cli/tests/watch.rs`, `cli/tests/status.rs` | `cargo test -p teamwire-watch --locked && cargo test -p teamwire-cli --test watch --test status --locked` | L |
| T11 Metrics and OpenTelemetry export | 4 | T02, T04 | `crates/teamwire-otel/`, `cli/src/cmd/metrics.rs`, `cli/src/cmd/export.rs`, `cli/tests/metrics.rs` | `cargo test -p teamwire-otel --locked && cargo test -p teamwire-cli --test metrics --locked` | M |
| T12 Messages | 4 | T07 | `crates/teamwire-msg/`, `cli/src/cmd/send.rs`, `cli/src/cmd/log.rs`, `cli/src/cmd/inbox.rs`, `cli/src/cmd/ack.rs`, `cli/src/cmd/members.rs`, `cli/src/cmd/lead.rs`, `cli/tests/send.rs`, `cli/tests/inbox.rs` | `cargo test -p teamwire-msg --locked && cargo test -p teamwire-cli --test send --test inbox --locked` | M |
| T13 Completions, results and hooks | 4 | T06, T07, T09 | `crates/teamwire-report/`, `cli/src/cmd/done.rs`, `cli/src/cmd/hook.rs`, `cli/tests/done.rs`, `cli/tests/hook.rs` | `cargo test -p teamwire-report --locked && cargo test -p teamwire-cli --test done --test hook --locked` | L |
| T14 Close, removal and archive | 4 | T03, T08, T09 | `crates/teamwire-close/`, `cli/src/cmd/close.rs`, `cli/tests/close.rs` | `cargo test -p teamwire-close --locked && cargo test -p teamwire-cli --test close --locked` | L |
| T15 MCP server | 5 | T10, T12, T13 | `crates/teamwire-mcp/`, `cli/src/cmd/mcp.rs` | `cargo test -p teamwire-mcp --locked` | M |
| T16 tmux host adapter | 5 | T04 | `crates/teamwire-host-tmux/` | `cargo test -p teamwire-host-tmux --locked` | M |
| T17 End-to-end tests | 5 | T05, T06, T08, T10, T12, T13, T14 | `tests/e2e/` | `cargo test -p teamwire-e2e --locked` | L |
| T18 Crash tests | 5 | T12, T13, T14 | `tests/crash/` | `cargo test -p teamwire-crash-tests --locked` | M |
| T19 Doctor, PR watch, presets, skill, examples | 5 | T02, T03, T05, T06 | `cli/src/cmd/doctor.rs`, `cli/src/cmd/pr_watch.rs`, `cli/src/cmd/preset.rs`, `cli/src/cmd/version.rs`, `cli/tests/doctor.rs`, `cli/tests/pr_watch.rs`, `cli/tests/preset.rs`, `examples/` | `cargo test -p teamwire-cli --test doctor --test pr_watch --test preset --locked` | M |

Waves at a glance:

| Wave | Tickets | What exists when it is merged |
| --- | --- | --- |
| 1 | T01 | The contract: workspace, core types and traits, schema DDL, CLI skeleton, gates |
| 2 | T02, T03, T04, T20 | Store, platform, fakes and contract suites, the full structure checker |
| 3 | T05, T06, T07, T08, T09 | Real adapters, delivery, launch, verify |
| 4 | T10, T11, T12, T13, T14 | Watch, metrics, messages, completions, close: the full lifecycle |
| 5 | T15, T16, T17, T18, T19 | MCP, tmux, end-to-end and crash proof, extras |

## Tickets

### T01 Workspace skeleton and contract (wave 1)

Builds:

- Root `Cargo.toml`, `rust-toolchain.toml`, `rustfmt.toml` exactly as in
  `docs/conventions.md` §3 and §4. `[workspace.dependencies]` declares every
  crate any ticket will use, at the newest version on crates.io at build
  time except these exact pins: `rmcp = "=3.5.0"` (features `server`,
  `transport-io`, `macros`, `schemars`); `opentelemetry`, `opentelemetry_sdk`,
  `opentelemetry-otlp` pinned to one `=0.33.x` (OTLP with `http-proto` and
  `reqwest-blocking-client`). The others: `rusqlite` (feature `bundled`),
  `serde`, `serde_json`, `thiserror`, `ulid`, `uuid` (v4), `time`
  (formatting, parsing), `sha2`, `hex`, `getrandom`, `clap` (derive),
  `tokio` (only for `teamwire-mcp`), `schemars`, `tracing`,
  `tracing-subscriber`, `globset`, `toml`, `sysinfo`, `nix` (process, signal),
  `tempfile`, `proptest`.
- `crates/teamwire-core`: every type and trait in `docs/api/core.md` §1-6a,
  including the `Registry` (adapter ids are strings; no enum of products),
  `sanitize::one_line`, `OneLine`, the event type enum with its string forms,
  the capability records. Unit tests for name parsing, sanitising
  (`crates/teamwire-core/tests/sanitize.rs::forged_event_line_is_flattened`
  and the property of `docs/test-strategy.md`), and enum string round trips.
- Skeletons of every other crate: `Cargo.toml` with `[lints] workspace =
  true` and its dependencies, and a `src/lib.rs` holding the crate's doc
  comment and the public function signatures of `docs/api/core.md` §8 with
  bodies that return a "not built" error. Adapter crates get a struct whose
  trait methods return `HostError { code: Other, .. }` or the matching
  harness or platform error.
- `crates/teamwire-store/src/schema/v1.sql`: the full DDL for the tables in
  `ARCHITECTURE.md`, with the constraints of `docs/api/core.md` §7.
- `cli/src/main.rs`: clap definitions for every subcommand and flag in
  `docs/api/cli.md`; each dispatches to `cmd::<name>::run`, whose stub prints
  "not built yet" and exits 3. `cli/src/wiring.rs` fills the `Registry` with every
  adapter the binary has and picks the lead's harness and host from the
  environment.
- `scripts/gate.sh`: the six gates of `docs/conventions.md` §2 in order, one
  line per step with its time, and `fast` mode for steps 1-3.
- `clippy.toml`, `.cargo/config.toml` (the `xtask` alias), the lint table and
  `[profile.dev]` of `docs/conventions.md` §4. Run clippy once and drop any
  option name it rejects.
- `xtask/` with a minimal `tidy`: every member has `[lints] workspace = true`,
  every member dependency is `workspace = true`, no `mod.rs`, `CLAUDE.md`'s
  first line is `@AGENTS.md`. T20 adds the rest.
- `AGENTS.md` and `CLAUDE.md` from `docs/conventions.md` §6, applied only
  after the lead confirms the text.
- First build fetches the pinned toolchain through rustup once; this ticket
  may do that.

### T20 Structure checker (wave 2)

Completes `cargo xtask tidy` with every rule of `docs/conventions.md` §1:
crate naming and manifest fields, file-size report and limit, the `//!`
note at the top of each crate and the one-row-per-crate check against
`ARCHITECTURE.md`'s Components table, `AGENTS.md` length, the layer graph of
§5 from `cargo metadata`, and `--base <ref>` reporting `PROTECTED-CHANGED`
and `WEAKENED` lines. Tests use small fixture workspaces in temporary
directories, one per rule, each proving the rule fails when broken.

### T02 Store (wave 2)

`Store::open`, `open_existing`, `write` (always `BEGIN IMMEDIATE`), `read`,
`backup_to`, `quick_check`, `integrity_check`; pragmas on every connection;
`application_id` and `user_version`; forward migration in one transaction;
refusal of a newer schema; one typed `Tx` method per operation named in
`docs/api/core.md` §7, each writing its event row; counters for display ids
and per-sender `seq`. Required tests: every `crates/teamwire-store/tests/...`
row in `docs/failure-modes.md` (concurrency, corrupt, migrate, full), a test
that reads back each pragma, and the store properties of
`docs/test-strategy.md`.

### T03 Platform (wave 2)

`MacOs` and `Linux` implementations of `Platform`, and `SystemClock`, the real `Clock` (the only code allowed to read the system clock): pid liveness, `env_of`
(with `sysinfo`; confirm it reads a same-user process's environment on
macOS, else stop and ask the lead), `run` in a new process group with a
timeout that kills the group and reports 124, `write_atomic` (temp file,
fsync, rename, fsync directory), `free_bytes`, `on_network_fs`, and the
`caffeinate` sleep hold with the ownership rules of SLP-1 (reuse an untimed
hold, `owned` flag, re-check before kill, never kill a hold a user started).
Port the flag parser of `team_sleephold.py` and its test cases. Required
tests: `crates/teamwire-platform/tests/run.rs::timeout_kills_process_group`,
`crates/teamwire-platform/tests/sleephold.rs::release_only_last_owned`, and
the platform contract suite from T04 once merged (this ticket writes its own
tests first; T05-T09 run in the next wave).

### T04 Fakes and contract suites (wave 2)

`FakeHost`, `FakeHarness`, `FakePlatform`, `FakeClock`, `TestHome`, the
`fake-herdr` and `fake-claude` binaries, and the contract suites
`host_contract`, `harness_contract`, `platform_contract`, all as described in
`docs/test-strategy.md`. `fake-herdr` starts every agent with a cleared
environment, and `fake-claude` applies the `env` key of its `--settings` file
to its shell steps and hooks. The fake binaries' fault files and scripts are
documented in the crate's README. The fakes pass their own contract suites.

### T05 herdr host adapter (wave 3)

`Herdr` implements `Host` over herdr's JSON CLI, running every command through `Platform::run` with a timeout;
error envelope mapped to `HostErrorCode` by `error.code`; `agent start` with
the bounded retry on `agent_pane_busy` and `agent_not_ready`; `occupant` from
`agent get` (`agent_session.value`); one workspace per team and one tab per
member, labels set once. Capabilities: `sets_env_at_spawn = false`.
Passes `host_contract` against `fake-herdr`.
Required test: `crates/teamwire-host-herdr/tests/errors.rs::pane_busy_retried_by_code`.
Adds `examples/live_check.rs` (read-only calls only).

### T06 Claude Code harness adapter (wave 3)

`ClaudeCode` implements `Harness`: `launch_spec` (argv of
`ARCHITECTURE.md`, contract and brief as files, `--session-id` chosen by
teamwire, per-run `settings.json` with the Stop and PostToolUse hooks (their
commands carry `--team` and `--role`) and an `env` key holding the member's
`TEAMWIRE_*` identity and token, `mcp.json` with the token in the server's
argv, both mode 0600 under `$TEAMWIRE_HOME`, `OTEL_RESOURCE_ATTRIBUTES`
appended; capability `session_env_via_config = true`);
`current_session` from `CLAUDE_CODE_SESSION_ID` and `CLAUDE_CODE_SESSION_KIND`;
liveness from session files, then `claude agents --json` with both row shapes
and a 5 s bound; `parse_turn_end` and `hold_reply`; the trust step (read
`hasTrustDialogAccepted` read-only; answer only the plain dialog; refuse the
escalated one and a home directory). All Claude paths come from config so
tests use a temporary directory. Required tests: every
`crates/teamwire-harness-claude/tests/...` row in `docs/failure-modes.md`,
and a fixture test that the argv matches `team-launch.py:1026-1039` apart from
the teamwire additions. Adds `examples/live_check.rs` for the user to run by
hand (`docs/test-strategy.md` §6).

### T07 Locator and delivery (wave 3)

`locate` (ADR 0006 decision table), the second host check before submit,
`deliver_line` with the retry rules of ADR 0003 and the `HandedOff`,
`Unconfirmed`, `Undelivered` outcomes, and the typed-line formats for
messages and results. Required tests: every
`crates/teamwire-deliver/tests/...` row in `docs/failure-modes.md` and the
locator property.

### T08 Launch (wave 3)

Spec parsing and whole-spec validation (LCH-2, LCH-5, LCH-6, LCH-7,
LCH-9, LCH-10, LCH-18, LCH-20, LCH-23), including the choice of each
member's identity channel from its adapters' capabilities and the refusal
`no_identity_channel`; member records written before each
irreversible step (LCH-3); `git worktree add` with `base_sha`; setup with the
`TEAMWIRE_*` variables, timeout, log and undo; contract text as data files in
`crates/teamwire-launch/contracts/` (one per kind, plus the shared "talking to
the team", "your scope" and finishing parts), adapted from
`~/.claude/team/bin/team-launch.py:319-569` (read-only) with teamwire's own
commands, and a test that each rule listed in speclife §1.2 is present; spawn
through the host; trust through the harness; dispatch sent once; member
tokens; the sleep hold; `--add`; exit codes 1 and 2. Required tests: every
`crates/teamwire-launch/tests/...` row in `docs/failure-modes.md`.

### T09 Verify and the report check (wave 3)

The check runner (process group, timeout 124), the diff base rule, `owns`
matching with `globset` (ADR 0009 semantics), the per-language rule files in
`crates/teamwire-verify/rules/` (Rust, Python, JS/TS, Go, Java), the five
verdicts, waivers bound to a HEAD, `--deep` and `--repair-files`, and a public
`report_check(member) -> ReportGaps` used by T13 (progress lines for own
first-parent non-merge commits since start minus 120 s; decisions list).
Required tests: every `crates/teamwire-verify/tests/...` row in
`docs/failure-modes.md`, a Rust fixture branch that adds `#[ignore]`, and the
`owns` property.

### T10 Watch and status (wave 4)

`poll` with the rules of WAT-1 to WAT-16: one line per poll, announcements
persisted, startup grace, listing failure, sleep gap, reprompt re-arm, watch
route `COMPLETED`, `UNDELIVERED`, `QUESTION-UNDELIVERED`, `TEAM-STALLED`,
`TEAM-COMPLETE`; the watch lease; the watch never calls a writing host
method. `status` as in `docs/api/cli.md`, read-only. Required tests: every
`crates/teamwire-watch/tests/...` row in `docs/failure-modes.md` and the watch
property. Port the scripted poll sequences of
`~/.claude/team/bin/tests/test_team_watch.py` (read-only) as test cases.

### T11 Metrics and OpenTelemetry export (wave 4)

`teamwire metrics` queries for every metric in `docs/metrics.md`, per team and
across teams and archives; `export_once` with the export mark, deterministic
trace and span ids, spans from event pairs; `teamwire export` as JSON lines.
Tests use an in-process OTLP receiver stub over HTTP on localhost. Required
tests: every `crates/teamwire-otel/tests/...` row in `docs/failure-modes.md`.

### T12 Messages (wave 4)

`prove_actor` (ADR 0008), the six types and their rules (MSG-1 to MSG-3,
MSG-9, MSG-14, MSG-15), `send` with the outbox order, idempotent keys and
`comms/` copies, `inbox` with hash checks, `ack`, `log`, reprompt generation
changes with undo on a known failure and keep on unconfirmed, and `teamwire
lead --adopt`. Port every refusal case of
`~/.claude/team/bin/tests/test_team_msg.py:138-232` (read-only). Required tests:
every `crates/teamwire-msg/tests/...` row in `docs/failure-modes.md` and the
delivery-state property.

### T13 Completions, results and hooks (wave 4)

`done` (validation, route B gap check), the result file with its marker and
rescue, `on_turn_end` (route A of ADR 0007, including `delivering` recovery
and the combined hold note), `on_tool_use` reminders, and the CLI entry
points `teamwire done`, `teamwire hook claude-stop`, `teamwire hook
claude-post-tool`, which always exit 0 for hooks. Port the cases of
`~/.claude/team/bin/tests/test_team_report.py:314-449` (read-only). Required
tests: every `crates/teamwire-report/tests/...` and `cli/tests/hook.rs` row in
`docs/failure-modes.md`.

### T14 Close, removal and archive (wave 4)

The close plan and `--dry-run`, the fixed order of CLS-1, the close lock with
re-checks under it, archive with the SQLite backup API and `SHA256SUMS`,
worktree removal without force, member removal with CLS-5, `--stop`,
`--unverified`, `--keep-state`, `--clear-stale-lock`, sleep hold release
last, branches kept and unpushed commits printed. Port
`~/.claude/team/bin/tests/test_team_msg.py:236-262` (read-only). Required
tests: every `crates/teamwire-close/tests/...` row in `docs/failure-modes.md`.

### T15 MCP server (wave 5)

`teamwire mcp` over stdio with `rmcp`: the seven tools of `docs/api/mcp.md`,
member and lead identity, structured results and error codes. Tests drive the
server with an in-process `rmcp` client: tool list snapshot, one success and
one refusal per tool, and a check that a refused call stores nothing.

### T16 tmux host adapter (wave 5)

`Tmux` implements `Host` with `new-window`, `load-buffer` plus
`paste-buffer -p` plus `send-keys Enter`, `capture-pane`, `display-message`
for the pane's pid, and the pid tree for `occupant`. `reports_state` is
false. Passes `host_contract` against a real tmux on a private socket
(`tmux -L teamwire-test-<pid>`), killed at the end of each test.

### T17 End-to-end tests (wave 5)

The scenarios of `docs/test-strategy.md` §5 with the real binary, real git,
`fake-herdr` and `fake-claude`.

### T18 Crash tests (wave 5)

The scenarios of `docs/test-strategy.md` §3 with `kill -9` at random points,
including every `tests/crash/...` row of `docs/failure-modes.md`.

### T19 Doctor, PR watch, presets, skill, examples (wave 5)

`teamwire doctor` (read-only checks of `docs/api/cli.md`), `teamwire
pr-watch` (EXT-1; tests with a fake `gh` on `PATH`), `teamwire preset` over
`examples/presets/*.json` (the six shapes, with no machine-specific paths),
`teamwire version`, `examples/skills/teamwire/SKILL.md` (the lead's
procedures from the `agent-team` skill, rewritten for teamwire commands), and
`examples/mcp/lead.json`. Required tests:
`cli/tests/doctor.rs::network_fs_warned` and one test per preset that it
parses as a valid spec.

## Open questions that affect the plan

None of the user's open questions (`ARCHITECTURE.md`) blocks a ticket. Two
engineering facts are confirmed inside tickets, and each ticket stops and asks
the lead if the fact is false:

- T03: `sysinfo` reads another same-user process's environment on macOS.
- T06: hooks in a `--settings` file are added to the user's own hooks. The
  ticket cannot test this (no real Claude session in tests); its
  `live_check` example is how the user confirms it before cutover.
