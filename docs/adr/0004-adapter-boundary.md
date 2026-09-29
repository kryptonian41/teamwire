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

1. **The core crate depends on no adapter crate, and has no closed list of
   products.** Adapters are identified by string ids (`HarnessId`, `HostId`)
   and looked up in a `Registry` that the CLI's wiring fills. The wiring
   module is the only code that names a concrete adapter. Adding Codex CLI is
   a new crate plus one `add_harness` call; no core type changes.
2. **The harness and host belong to each party.** The lead's are recorded on
   the team, each member's on its own row. A Claude Code lead can lead a
   member on another harness; every lookup (proving the lead, locating the
   lead, delivering to a member) uses that party's adapters (ADP-12).
3. **A harness may deliver text itself.** `Harness::push` with the capability
   `can_push`, and the route `HarnessPush`, leave room for opencode's
   `prompt_async` and Claude Channels. v1 adapters do not use it.
4. **Sink, locator and liveness are roles, not traits.** The sink is
   `Host::submit`. The locator is a function in `teamwire-deliver` that
   combines `Harness::processes_for_session`, `Platform::env_of` and
   `Host::occupant` (ADR 0006). Liveness is `Harness::is_alive` plus
   `Host::state`.
5. **Each adapter declares a capability record** (ADP-6):

   | Field | Claude Code | herdr | tmux | Fake |
   | --- | --- | --- | --- | --- |
   | `can_type` | via host | yes | yes | configurable |
   | `reports_state` (idle, working, blocked) | via `claude agents --json` | yes | no | configurable |
   | `knows_session_of_place` | n/a | yes (`agent_session`) | no (pid tree only) | configurable |
   | `turn_end_hook` | yes (Stop) | n/a | n/a | configurable |
   | `in_turn_hook` | yes (PostToolUse) | n/a | n/a | configurable |
   | `mcp_client` | yes | n/a | n/a | yes |
   | `choose_session_id` | yes (`--session-id`) | n/a | n/a | yes |
   | `can_push` (harness puts text in its own session) | no in v1 | n/a | n/a | configurable |
   | `session_env_via_config` (identity through a config file) | yes (`settings.json` `env`) | n/a | n/a | configurable |
   | `sets_env_at_spawn` (host passes environment) | n/a | no | yes (`new-window -e`) | configurable; `fake-herdr` never |
   | `guards_dialogs` (host refuses to type into a dialog) | n/a | yes (`agent_blocked`) | no; teamwire checks the screen first (ADR 0006 point 3a) | configurable |
   | `submit_ack` | none | "submitted", `--wait` sees a state change | none | configurable |

   The core picks routes from these fields, never from product names.
6. **Per-member configuration is per run.** For Claude Code, launch writes
   `members/<role>/settings.json` (the Stop and PostToolUse hooks, pointing at
   `teamwire hook claude-stop` and `teamwire hook claude-post-tool`) and
   `members/<role>/mcp.json` (the `teamwire mcp` server with the member's team
   and role in its argv), and passes them with `--settings` and
   `--mcp-config`. Nothing is registered in `~/.claude/settings.json`
   (LCH-21, SEC-3). This also means the Python Stop hook and teamwire's hook
   never both act on one member: the Python hook finds no Python manifest for
   a teamwire member and exits at its first check.
7. **Host error handling uses codes.** Each host adapter maps its product's
   errors into `HostError { code: HostErrorCode, detail }`, where the code is
   one of `NotReady`, `Busy`, `Blocked`, `NotFound`, `Stalled`, `Timeout`,
   `BadArgument`, `Unsupported`, `Other`. The herdr adapter reads `error.code` from herdr's
   JSON (ADP-5). Retry rules in ADR 0003 are written against these codes.
8. **Every adapter call has a timeout** (DEL-14), set by the caller.
9. **One contract-test suite per trait**, in `teamwire-fakes`, runs against
   the fake and against each real adapter (the herdr adapter against the fake
   `herdr` binary, the tmux adapter against a real tmux on a private socket).
10. **No host is a valid configuration, built as a real adapter.**
   `teamwire-core` ships `NoHost` (id `none`): every capability is false,
   `list` and `live_agent_names` return empty, `occupant` and `state` return
   `NotFound`, and every writing call (`spawn`, `submit`, `send_keys`,
   `close`) returns `NotFound` without doing anything. Wiring always registers
   it and picks it for the lead when neither herdr nor tmux is detected.
   What still works with it (DEL-17):
   - A lead whose host is `none` gets route B for results (the watch prints
     them) and its questions are `undelivered` with `no_host`, so the watch
     prints `QUESTION-UNDELIVERED`.
   - A message to a member whose host is `none` is stored, marked
     `undelivered` with `no_host`, and still readable with `inbox`.
   - `status`, `log`, `verify`, `metrics` and `close` need no host.
   - `launch` refuses members whose host is `none` (`no_host`): v1 cannot
     start them, because a headless host that spawns `claude -p` is not built
     (ADR 0010).

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
