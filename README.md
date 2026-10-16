# teamwire

Run a team of coding agents from one lead agent, and get their results back
with no human watching.

teamwire is a single Rust binary. A lead agent, such as a Claude Code session,
uses it to start members, pass messages between them, collect each member's
result, check the work, and close the team down. Every message and every
result is stored in SQLite before teamwire tries to deliver it. When a
delivery fails, teamwire reports the failure instead of dropping it.

## What it does

| Step | Command | What happens |
| --- | --- | --- |
| Launch | `teamwire launch` | Starts each member in its own terminal tab, with a role, a brief, and its own git worktree when it writes code. |
| Message | `teamwire send` | Checks the sender and the message type, stores the message, then types a one-line pointer into each recipient's pane. |
| Report | `teamwire done` | A member records that it is done, blocked or failed. A Stop hook writes the result file and wakes the lead with one line. |
| Watch | `teamwire watch` | Prints one line when a member dies, stalls or runs past its deadline, and when the whole team completes. |
| Verify | `teamwire verify` | Runs each member's check itself, and flags weakened tests and edits outside the files the member owns. |
| Close | `teamwire close` | Refuses while work is uncommitted, archives the team, checks the archive, then removes worktrees and tabs. |

The lead calls teamwire through the CLI or through an MCP server. Members get
their own MCP tools, which cannot run the lifecycle commands.

## Status

teamwire runs the author's real agent teams. The version is `0.0.0`, and the
crates are not published to crates.io.

| Part | Supported |
| --- | --- |
| Agent harness | Claude Code |
| Terminal host | herdr, and tmux. In tmux, teamwire finds the lead through the process tree and asks Claude Code whether a member is working. |
| Platform | macOS. Linux works without the sleep hold that keeps the machine awake during a run. |

## Requirements

- Rust 1.94.1. The `rust-toolchain.toml` file pins it, so `rustup` installs it
  for you.
- git
- Claude Code, with `claude` on `PATH`
- herdr or tmux. With herdr: `herdr` on `PATH` and its session hook in your
  Claude Code settings. With tmux: `tmux` on `PATH`. The lead session must run
  in a herdr pane or a tmux window of its own.

## Install

```bash
cargo install --locked --path crates/teamwire-cli
teamwire doctor
teamwire install
```

`teamwire doctor` checks the state directory, the disk, the herdr session
hook, the `claude` binary and its flags, the host binary in use, the teamwire
plugin, and that the repository ignores `.worktrees/`. It changes nothing.

`teamwire install` writes the teamwire plugin to `$TEAMWIRE_HOME/claude-plugin/`
and registers it with `claude plugin`. The plugin holds the lead's skill
(`/teamwire:teamwire`), its MCP server and its Stop hook (ADR 0018). Install
also removes the skill and MCP entry an older teamwire installed. Run it again
after every `cargo install`, so the plugin matches the new binary, then start
a new Claude Code session. `teamwire uninstall` removes the plugin.

Before you rely on teamwire, run the live checks in
[docs/test-strategy.md](docs/test-strategy.md), section 6. The full setup
steps are in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), section "Installing
teamwire".

## Quick start

Run these from the lead's Claude Code session, inside a herdr pane or a tmux window.

```bash
teamwire preset                    # list the six team shapes
teamwire preset survey             # print one shape's rules and a spec skeleton
# copy the spec into team.json, then fill in every CHANGEME and /abs/path value
teamwire launch --dry-run --spec team.json
teamwire launch --spec team.json
teamwire watch <team>              # leave it running in the background
```

teamwire types each member's result into the lead's pane when the member
finishes. Then:

```bash
teamwire status <team>             # members, messages and results
teamwire verify <team>             # run each member's check
teamwire close <team> --dry-run
teamwire close <team>
```

The `teamwire` skill holds the full procedure for a lead: picking a team
shape, writing briefs, the message rules, and when to remove a member. Its
source is [examples/skills/teamwire/SKILL.md](examples/skills/teamwire/SKILL.md).

## Configuration

| Setting | Default | What it does |
| --- | --- | --- |
| `TEAMWIRE_HOME` | `~/.teamwire` | Where every team's state lives. `--home <DIR>` overrides it on any command. |
| `host` in the spec | detected | The terminal host. teamwire picks herdr when `HERDR_ENV=1`. |
| `harness` in the spec | `claude-code` | The agent program each member runs. |

Each team has one SQLite database under `$TEAMWIRE_HOME`. teamwire also writes
each message body and each result as a Markdown file beside the database, so a
person can read them. teamwire writes nothing under `~/.claude/` except what
`teamwire install` writes.

Metrics come from the event rows in each team's database. `teamwire metrics`
shows them locally. `teamwire metrics export` sends them to an OpenTelemetry
endpoint. See [docs/metrics.md](docs/metrics.md).

## Documentation

| Document | What it holds |
| --- | --- |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | The design: components, data model, message lifecycle, failure handling, adapters, security, installation, open questions |
| [docs/api/cli.md](docs/api/cli.md) | Every subcommand: its flags, output and exit codes |
| [docs/api/mcp.md](docs/api/mcp.md) | Every MCP tool: input, output, errors |
| [docs/api/core.md](docs/api/core.md) | The Rust types, traits and service functions |
| [docs/requirements.md](docs/requirements.md) | Every requirement (MUST or SHOULD) with its source, and the invariants that come from real incidents |
| [docs/failure-modes.md](docs/failure-modes.md) | Each failure, how it is detected, what teamwire does, and the test that proves it |
| [docs/metrics.md](docs/metrics.md) | Events, metrics and trace spans |
| [docs/adr/](docs/adr/) | Decision records: the options, the choice, the consequences |
| [docs/test-strategy.md](docs/test-strategy.md) | Fakes, property, crash, fault-injection and end-to-end tests |
| [docs/conventions.md](docs/conventions.md) | Repository layout, gates, lints, and the rules for every change |
| [docs/build-plan.md](docs/build-plan.md) | The build as tickets in dependency waves |
| [CONTEXT.md](CONTEXT.md) | The glossary of terms that the docs, errors and commands share |
| [examples/](examples/) | Team presets, the lead's skill, and the lead's MCP config |
| [ui/](ui/) | A read-only web dashboard for teams, built with Bun and React. Start it with `cd ui && bun install && bun run start` |

## Development

The workspace has one crate per concern under `crates/`. A crate depends only
on crates in lower layers, listed in [docs/conventions.md](docs/conventions.md),
section 5. Read [AGENTS.md](AGENTS.md) before you change anything. It lists
the hard rules and the program that enforces each one.

```bash
cargo test -p <crate> --locked     # while you work on one crate
scripts/gate.sh fast               # format, tidy and clippy
scripts/test-changed.sh            # test the changed crates and every crate that depends on them
scripts/gate.sh                    # the full gate: adds every test, rustdoc and the docs check
```

Tests never start a real Claude session or type into a real herdr pane. They
use the fake binaries in `tests/fakes/`, and set `TEAMWIRE_HOME` to a
temporary directory.

## License

`Cargo.toml` declares the workspace license as `MIT OR Apache-2.0`.
