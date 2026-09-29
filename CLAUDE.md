# teamwire

A Rust tool that runs teams of coding agents: team discovery, messages between
members, team events, metrics, and the team lifecycle (launch, watch, status,
verify, close). Its API does not depend on any one agent harness; Claude Code
is the first harness it supports.

## Separate project: never touch the running setup

A Python version of this tooling runs the user's real agent teams today. It
must keep working while teamwire is built. So:

- Write only inside this repository and inside temporary directories you
  create for tests. Never create, edit, move or delete anything under
  `~/.claude/` (including `~/.claude/team/`, `~/.claude/hooks/`,
  `~/.claude/settings.json`, `~/.claude/skills/`) or in `~/.claude.json`.
- Never register a hook, MCP server, skill or launch agent anywhere outside
  this repository. Example configs live in this repo as files.
- teamwire's own state directory is `$TEAMWIRE_HOME`, default
  `~/.teamwire`. Tests always set `TEAMWIRE_HOME` to a temporary directory.
- Tests never type into a real herdr pane or start a real Claude session.
  Use the fake `herdr` and fake harness binaries under `tests/fakes/`.
- Reading the Python tooling to learn its behaviour is fine:
  `~/.claude/team/bin/`, `~/.claude/hooks/team-report.sh`,
  `~/.claude/skills/agent-team/SKILL.md`.

## Commands

- Docs check (architecture phase): `scripts/check-docs.sh`
- Build: `cargo build --workspace` (once the workspace exists)
- Targeted tests: `cargo test -p <crate>`
- Lint: `cargo clippy --workspace --all-targets -- -D warnings`
- Format check: `cargo fmt --all -- --check`

On this Mac, GNU tools carry a `g` prefix: `gsed -i`, `gdate`, `gtimeout`.
Plain `sed -i` is BSD sed and does something different.
