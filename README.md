# teamwire

teamwire runs a team of coding agents for one lead agent. It launches each
member with a role, a brief and its own git worktree; carries messages
between members under fixed rules, storing every message before it tries to
deliver it; tells the lead, with one short line, when a member finishes, and
says so loudly when it cannot; watches the team for members that stall, die or
run late; checks each member's work; and closes the team down in a safe
order. All state lives in one SQLite database per team under
`$TEAMWIRE_HOME` (default `~/.teamwire`), metrics are kept locally and
exported over OpenTelemetry, and agents use it through a CLI or MCP tools.
Claude Code in herdr is the first supported setup; the core names no agent
harness and no terminal host. It is written in Rust and is being built to
replace a Python version of the same tooling, which keeps running until the
user decides to switch.

Status: architecture phase. There is no code yet.

## Documents

| Document | What it holds |
| --- | --- |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | The design: components, data model, message lifecycle, failure handling, adapters, security, cutover, open questions |
| [docs/requirements.md](docs/requirements.md) | Every requirement (MUST or SHOULD) with its source, and the invariants that come from real incidents |
| [docs/adr/](docs/adr/) | Decision records: the options, the choice, the consequences |
| [docs/api/core.md](docs/api/core.md) | The Rust types, traits and service functions |
| [docs/api/mcp.md](docs/api/mcp.md) | Every MCP tool: input, output, errors |
| [docs/api/cli.md](docs/api/cli.md) | Every subcommand, and which Python script it replaces |
| [docs/failure-modes.md](docs/failure-modes.md) | Each failure, how it is detected, what teamwire does, the test that proves it |
| [docs/metrics.md](docs/metrics.md) | Events, metrics and trace spans |
| [docs/test-strategy.md](docs/test-strategy.md) | Fakes, property, crash, fault-injection and end-to-end tests |
| [docs/conventions.md](docs/conventions.md) | Repository layout, gates, lints, rules for every ticket |
| [docs/build-plan.md](docs/build-plan.md) | The build as tickets in dependency waves |

## Checks

- Architecture phase: `scripts/check-docs.sh`
- Once the workspace exists: `scripts/gate.sh` (see `docs/conventions.md`)
