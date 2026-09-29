# ADR 0004: Harness, host and platform adapters

Status: accepted, 2026-09-29.

## Context

The brief fixes that the API must not depend on one harness or one terminal
host. Claude Code and herdr come first. Codex CLI, Gemini CLI and opencode as
harnesses, and tmux or no multiplexer as hosts, must fit without changing the
core.

The Python tooling mixes the two everywhere. speclife 1.5 lists the Claude
facts (`claude agents --json`, the session files, the trust dialog, the Stop
hook's input) and the herdr facts (JSON envelopes, `agent prompt`, pane ids,
workspaces) side by side in the same scripts.

harness R1-R3 propose two traits. specmsg R44 proposes three roles: a sink
(put text in a session), a locator (where is this session now) and a liveness
source. speclife LC-52 needs a third kind of fact that is neither harness nor
host: the operating system (sleep hold, process environment, process groups).

## Decision

Three traits in `teamwire-core`, each with one implementation crate per
product, and a fake for each under `tests/fakes/`:

| Trait | Knows | v1 implementations |
| --- | --- | --- |
| `Harness` | One agent program: how to start it, its session ids, whether a session is alive, whether the caller is a background session, how to give a member per-run hooks and an MCP server, its trust dialog, its OpenTelemetry variables, how to parse its turn-end hook input | `ClaudeCode`, `FakeHarness` |
| `Host` | One place programs run: spawn in a new place, submit text, report state, read the screen tail, send keys, close, list, and say who occupies a place | `Herdr`, `Tmux` (wave 5), `FakeHost` |
| `Platform` | The operating system: pid liveness, a process's environment, process-group kill, the sleep hold, atomic file writes | `MacOs`, `Linux` (liveness and env only; no sleep hold), `FakePlatform` |

The full signatures are in `docs/api/core.md`. Rules that go with them:

1. **The core crate depends on no adapter crate.** The CLI's wiring module is
   the only code that names a concrete adapter.
2. **Sink, locator and liveness are roles, not traits.** The sink is
   `Host::submit`. The locator is a function in `teamwire-deliver` that
   combines `Harness::processes_for_session`, `Platform::env_of` and
   `Host::occupant` (ADR 0006). Liveness is `Harness::is_alive` plus
   `Host::state`.
3. **Each adapter declares a capability record** (ADP-6):

   | Field | Claude Code | herdr | tmux | Fake |
   | --- | --- | --- | --- | --- |
   | `can_type` | via host | yes | yes | configurable |
   | `reports_state` (idle, working, blocked) | via `claude agents --json` | yes | no | configurable |
   | `knows_session_of_place` | n/a | yes (`agent_session`) | no (pid tree only) | configurable |
   | `turn_end_hook` | yes (Stop) | n/a | n/a | configurable |
   | `in_turn_hook` | yes (PostToolUse) | n/a | n/a | configurable |
   | `mcp_client` | yes | n/a | n/a | yes |
   | `choose_session_id` | yes (`--session-id`) | n/a | n/a | yes |
   | `submit_ack` | none | "submitted", `--wait` sees a state change | none | configurable |

   The core picks routes from these fields, never from product names.
4. **Per-member configuration is per run.** For Claude Code, launch writes
   `members/<role>/settings.json` (the Stop and PostToolUse hooks, pointing at
   `teamwire hook claude-stop` and `teamwire hook claude-post-tool`) and
   `members/<role>/mcp.json` (the `teamwire mcp` server with the member's team
   and role in its argv), and passes them with `--settings` and
   `--mcp-config`. Nothing is registered in `~/.claude/settings.json`
   (LCH-21, SEC-3). This also means the Python Stop hook and teamwire's hook
   never both act on one member: the Python hook finds no Python manifest for
   a teamwire member and exits at its first check.
5. **Host error handling uses codes.** Each host adapter maps its product's
   errors into `HostError { code: HostErrorCode, detail }`, where the code is
   one of `NotReady`, `Busy`, `Blocked`, `NotFound`, `Stalled`, `Timeout`,
   `BadArgument`, `Other`. The herdr adapter reads `error.code` from herdr's
   JSON (ADP-5). Retry rules in ADR 0003 are written against these codes.
6. **Every adapter call has a timeout** (DEL-14), set by the caller.
7. **One contract-test suite per trait**, in `teamwire-fakes`, runs against
   the fake and against each real adapter (the herdr adapter against the fake
   `herdr` binary, the tmux adapter against a real tmux on a private socket).
8. **No host is a valid configuration.** A member may have no place (ADP-9).
   It then has no typed wake-up. It reads through `inbox`, its completions
   reach the lead through the watch (ADR 0007), and undelivered items are
   still shown (DEL-17). In v1 the only way to start such a member is outside
   teamwire; a headless host that spawns `claude -p` is not built (ADR 0010).

## Consequences

- Adding Codex, Gemini CLI or opencode means one new `Harness` crate and a row
  in the capability table. Adding a host means one new `Host` crate that
  passes the contract suite.
- The trust dialog stays a screen-scraping step inside the Claude adapter
  (LCH-15). It reads `~/.claude.json` read-only first and only reads the
  screen when trust is missing.
- tmux cannot say which session sits in a pane. Its locator evidence is the
  pane's process tree, matched against the harness's process for that
  session. That is still two independent views (ADR 0006).
- The per-run `--settings` route rests on one fact not yet tested: that hooks
  in a `--settings` file are added to the user's own hooks. harness R7 read it
  from `claude --help`. The build plan puts a manual live check in the Claude
  adapter ticket before anything depends on it.
