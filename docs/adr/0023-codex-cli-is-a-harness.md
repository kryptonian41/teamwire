# ADR 0023: The Codex CLI is a harness, for members and for the lead

Status: accepted, 2026-10-18.

## Context

ADR 0004 promised that a second harness is one new crate and one
`add_harness` call. Codex CLI 0.163.0 (`@openai/codex`, 2026-10-15) is that
second harness, and it breaks the promise in a few places:

- Codex chooses its own thread id; there is no `--session-id`.
- herdr 0.9.3 starts an agent from `--kind` and passes only its arguments, and
  it cannot set environment variables. A wrapper script cannot load the
  member's identity, and Codex has no config key like Claude's
  `settings.json` `env` that reaches shell tools, hooks and MCP servers at
  once.
- By default the Codex TUI runs its session inside a shared background
  daemon, outside the terminal pane. Hooks then see the daemon's
  environment, and the session's process is not in the pane's process tree,
  so the locator of ADR 0006 cannot find it.
- Codex has no tool like Claude's Monitor that wakes the model on each line
  of a background command. A Codex lead cannot read `teamwire watch`.
- The member hooks, the contract text and doctor name Claude Code.

The facts below come from Codex's source at commit `8c07694`, its docs, and
`--help` of the 0.163.0 binary. The user chose on 2026-10-18 not to install
Codex for a live check, so none of them has been seen in a live session.
The last section lists the ones most likely to be wrong.

## Decision

1. **A crate and an id.** `teamwire-harness-codex` holds `CodexCli` (id
   `codex`) and `CodexConfig { codex_home, codex_bin }`. `codex_home` is
   `TEAMWIRE_CODEX_HOME` (tests), else `CODEX_HOME`, else `~/.codex`.
   teamwire only reads it. A spec picks Codex with `"harness": "codex"` on
   the team or a member (`plan.rs` already reads the field).

2. **Capabilities.**

   | Field | Codex |
   | --- | --- |
   | `turn_end_hook` | yes (`Stop`) |
   | `in_turn_hook` | yes (`PostToolUse`) |
   | `mcp_client` | yes |
   | `choose_session_id` | no; the `SessionStart` hook records it (point 5) |
   | `can_push` | no |
   | `session_env_via_config` | yes, through `-c` values (point 4) |
   | `reads_watch_lines` (new) | no; Claude Code yes |

   `reads_watch_lines` is new on `HarnessCaps`. It says whether a lead on
   this harness can run `teamwire watch` and read each line as it prints.

3. **A member's start.** `launch_spec` returns this argv. Each `-c` value is
   TOML, and every string inside it is escaped as a TOML basic string:

   ```
   codex --no-daemon
     --dangerously-bypass-approvals-and-sandbox
     --dangerously-bypass-hook-trust
     -C <cwd>
     [-m <model>] [-c model_reasoning_effort="<effort>"]
     -c developer_instructions="<the contract file's text>"
     -c check_for_update_on_startup=false
     -c tui.disable_paste_burst=true
     [-c 'projects={"<trust root>"={trust_level="trusted"}}']
     -c 'shell_environment_policy.set={TEAMWIRE_TEAM="<t>",TEAMWIRE_ROLE="<r>",TEAMWIRE_HOME="<h>",TEAMWIRE_TOKEN_FILE="<member_dir>/token"}'
     -c 'mcp_servers.teamwire={command="<bin>",args=["--home","<h>","mcp","--team","<t>","--role","<r>"],env={TEAMWIRE_TOKEN_FILE="<member_dir>/token"}}'
     -c 'hooks.Stop=[{hooks=[{type="command",command="<hook turn-end>",timeout=60}]}]'
     -c 'hooks.PostToolUse=[{hooks=[{type="command",command="<hook post-tool>"}]}]'
     -c 'hooks.SessionStart=[{hooks=[{type="command",command="<hook session-start>"}]}]'
     -c 'plugins={"teamwire@teamwire"={enabled=false}, <the member's plugins map>}'
   ```

   - `--no-daemon` keeps the session, its hooks and its MCP server inside the
     pane's process tree, under the environment Codex started with.
   - Both bypass flags give a Codex member the trust a Claude member has with
     `bypassPermissions` (user decision, 2026-10-18). Under Codex's
     `workspace-write` sandbox, `.git` and a worktree's gitdir are read-only,
     so `git commit` would fail.
   - `tui.disable_paste_burst=true`: without it, an Enter that arrives within
     120 ms of fast typing inserts a newline instead of submitting.
   - The trust root is the main repository's root for a worktree member (Codex
     keys trust there), else the cwd. The value is passed only when
     `MemberLaunch::accept_trust` (new, copied from the spec) is on. A
     resume sets it, because the folder was accepted at the first start.
     It writes nothing to `config.toml`.
   - `<hook X>` is one shell command line, which Codex runs with `$SHELL -lc`:

     ```
     TEAMWIRE_TEAM=<t> TEAMWIRE_ROLE=<r> TEAMWIRE_HOME=<h> TEAMWIRE_TOKEN_FILE=<f> <bin> --home <h> hook <X> --team <t> --role <r>
     ```

     Every value is quoted for `sh`. The hook process gets Codex's own
     environment, which on herdr carries no teamwire variable, so the command
     sets them.
   - The `plugins` map of ADR 0017 applies to Codex members too, with the same
     `name@marketplace` ids. `teamwire@teamwire` is always off, as for Claude
     members (ADR 0018 point 4).

   `resume_spec` is `codex resume <id>` with the same options. `-C <cwd>`
   keeps Codex from asking which folder to resume in.

4. **Identity without a token in argv.** Launch writes the token, and
   nothing else, to `<member_dir>/token` with mode 0600, as one of the
   harness's `LaunchFile`s. Only its path appears in argv, as
   `TEAMWIRE_TOKEN_FILE`. Every reader of `TEAMWIRE_TOKEN` (send, done, inbox,
   ack, `mcp --team`, the hooks) uses `teamwire_msg::caller_token`, which
   returns `TEAMWIRE_TOKEN`. When that is unset, it returns the contents of
   the file that `TEAMWIRE_TOKEN_FILE` names, with trailing whitespace
   removed. The channel is `IdentityChannel::HarnessConfig`. The file stays
   until close, so a resumed member keeps its identity, as a Claude member
   keeps its `settings.json`. The file is exposed exactly as `settings.json`
   is: readable by the user's own processes, never written into the
   worktree.

5. **The session id comes from the session-start hook.**
   `teamwire hook session-start --team T --role R` reads the caller's token
   through `caller_token`. It accepts the call only when the token's hash
   matches the member, and the member is not removed. It parses stdin with
   the member's harness (`parse_turn_end`; only `session` is used). It then
   records that session on the member row, replacing any other value, and
   prints nothing. Launch's step 7 records the host's
   `occupant().session` only when the row still has none, so the hook's
   value wins whichever comes first. Until a session is recorded, watch and
   status treat the member's liveness as unknown, never as dead.

6. **Member hooks follow the member's harness.** `hook turn-end`,
   `hook post-tool` and `hook session-start` resolve the harness from the
   member row (`ctx.adapters(&Party::Member(role))`). The old names
   `claude-stop` and `claude-post-tool` do the same, because settings written
   by an older launch still spell them. `hook lead-stop --harness <id>`
   parses with the named harness, `claude-code` when absent.
   `claude-lead-stop` stays. Codex's Stop input maps to `TurnEnd` as
   follows:

   | `TurnEnd` field | Codex Stop input |
   | --- | --- |
   | `session` | `session_id` |
   | `subagent` | `false` (Codex sends `SubagentStop` instead) |
   | `hold_already_active` | `stop_hook_active` |
   | `final_message` | `last_assistant_message` |
   | `transcript` | `transcript_path` |

   The hold reply is `{"decision":"block","reason":<note>}`. The PostToolUse
   reply has Claude's shape, `hookSpecificOutput.additionalContext`. The
   turn-end event's `hook` value is `claude-stop` for Claude Code and
   `codex-stop` for Codex.

7. **Liveness and the session's folder.** A live Codex writer holds an OS
   lock on `<codex_home>/thread-writer-locks/<id>.lock`.
   `processes_for_session` runs `lsof -t <lock file>` through
   `Platform::run`, under the caller's timeout, and keeps the live pids.
   `is_alive` reports `Alive` when a holder exists. It reports `Dead` when the
   session's rollout file exists and has no holder. Otherwise it reports
   `Unknown`. The rollout file is
   `<codex_home>/sessions/YYYY/MM/DD/rollout-*-<id>.jsonl`. Its first line is
   `session_meta`, whose `cwd` answers `session_cwd`.

8. **The caller's session.** `caller_session(get)` (new, with a default that
   reads the process environment) reads `CODEX_SESSION_ID`, then
   `CODEX_THREAD_ID`, then `MCP_META_sessionId`, then `MCP_META_threadId`. The
   kind is `Interactive` only when the rollout's `session_meta.source` is
   `cli`, the terminal TUI. Anything else, or no rollout, is `Background`, so
   it cannot prove a lead. For the root thread every one of these ids is the
   same UUID. A subagent has its own thread id but the root's session id,
   which is why the session id is read first. The lead's MCP server
   (`mcp --lead`) passes each string field of the request's `_meta` to the
   lead check as `MCP_META_<key>`, because Codex gives the session to an MCP
   server only there. `CallerEnv` carries those values, and `prove_actor`
   calls `caller_session` with a lookup that checks them before the process
   environment. A member's server never does this.

9. **A Codex lead.**
   - It runs as `codex --no-daemon` in a herdr or tmux pane. No config key
     makes that the default.
   - Wiring picks the lead harness from `TEAMWIRE_LEAD_HARNESS` when that is
     set. Otherwise it picks `claude-code` when `CLAUDE_CODE_SESSION_ID` is
     set, else `codex` when `CODEX_THREAD_ID` is set, else `claude-code`.
   - `lead_resume_argv` is `codex --no-daemon resume <id>`.
   - Launch of a new team whose lead harness lacks `reads_watch_lines` runs
     the locator on the lead once. When the locator fails, launch refuses
     with code `lead_not_locatable`, and the message names
     `codex --no-daemon`. Route B would print results to a watch nobody
     reads.
   - `teamwire watch <team> --deliver` also types each line it prints into
     the lead's pane, through `teamwire-deliver`, with the
     `[teamwire:<team>]` prefix. A line that cannot be delivered is printed
     only. The lead Stop hook's note, and the note that ends a result line,
     name this command for a lead without `reads_watch_lines`. The lead
     starts it in the background.
   - The lead plugin is a local marketplace `teamwire` at
     `$TEAMWIRE_HOME/codex-plugin/` that holds one plugin, `teamwire`, with
     manifest `.codex-plugin/plugin.json`. It carries:
     - the skill `skills/teamwire/SKILL.md`: a short Codex part (tool names
       `mcp__teamwire__team_*`, no ToolSearch, `watch --deliver` in place of
       Monitor) followed by the shared skill text;
     - `.mcp.json` with the server `teamwire` running
       `<bin> [--home H] mcp --lead`;
     - `hooks/hooks.json` with a Stop hook running
       `<bin> [--home H] hook lead-stop --harness codex`.

     The CLI passes the shared skill text in, because one adapter crate may
     not depend on another.
   - teamwire changes Codex's setup only by running
     `codex plugin marketplace add|remove` and `codex plugin add|remove`. A
     changed plugin is removed and added again. Codex asks the user to review
     the plugin's hook once after each change, and install says so.
   - `teamwire install` finds which of `claude` and `codex` are on PATH. In a
     terminal it shows a numbered selector of those harnesses. Without a
     terminal it installs the harnesses named by `--harness`, or
     `claude-code` when the flag is absent. `uninstall` removes every
     harness's setup it finds. Doctor reports the `codex` binary when it is
     on PATH. It reports a missing Codex plugin as `ok` with the command that
     adds it, and a stale one as `warn`.

10. **The contract text names no harness.** It says "the MCP tool
   `team_done` (`mcp__teamwire__team_done`; if your tools load on request,
   load it first)" and "the repository's agent instruction file (CLAUDE.md or
   AGENTS.md)". Codex's name for a config server's tool is
   `mcp__teamwire__<tool>`, the same as Claude's.

11. **Not built.**
   - OpenTelemetry settings for Codex members: `otel_env` is empty.
     Metrics still come from the event rows.
   - A live check example.
   - Reading a Codex transcript, since `final_message` comes from the hook
     input.

## Consequences

- One test-only fake changes: `FakeHarness::caps()` sets
  `reads_watch_lines: true`. Every `MemberLaunch` literal gains
  `accept_trust`.
- A Codex member's `-c hooks.*` values replace that member's own `Stop`,
  `PostToolUse` and `SessionStart` hooks from `~/.codex` for that run.
- A long contract travels in argv. herdr types the command into a shell, so
  a very long line is a risk that only a live run can measure.
- Facts most likely to be wrong, to check first when Codex is installed:
  1. `-c hooks.<Event>=[...]` fires per run, with
     `--dangerously-bypass-hook-trust` suppressing the review dialog.
  2. `-c projects={...}` suppresses the folder-trust dialog.
  3. `-c plugins={...}` merges with the user's plugin table rather than
     replacing it.
  4. The thread lock is held for the whole session, and `lsof` sees it.
  5. herdr's `agent start --kind codex` passes every argument through
     unchanged.
