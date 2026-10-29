# ADR 0029: pi is a harness, for members only

Status: accepted, 2026-10-25.

## Context

pi (`@earendil-works/pi-coding-agent`, 1.4.0) is a terminal coding agent
that can run many providers' models. The user wants teamwire members to run
on it. It differs from Claude Code and Codex in the ways that shape an
adapter:

- pi has no hook commands. Its hooks are TypeScript extensions that run
  inside the pi process, loaded from `~/.pi/agent/extensions/` or with
  `-e <file>`.
- pi takes the session id with `--session-id`, so teamwire can choose it.
- pi has no tool like Claude's Monitor, so a pi lead could not read
  `teamwire watch`.
- pi renames its process to `pi` and clears its argv, so a process listing
  cannot find a session by its id.
- pi holds no lock on a live session's file, and writes the file only at
  the session's first message.

The facts below come from pi's docs and `dist/` source in the 1.4.0 package,
and from a pi 1.4.0 run in a private tmux server on 2026-10-25 with no
prompt sent. That run captured the screens in
`crates/teamwire-harness-pi/tests/screens/`. It also loaded the member
extension that launch writes: pi showed no load error, ran the MCP server
with the expected arguments and `TEAMWIRE_*` variables, and listed the
contract as context. The extension's hook paths were checked by loading
the same file with jiti, pi's own loader, against a mock of pi's API. No
model turn ran, so pi never called them itself.

## Decision

1. **Members only.** `teamwire-harness-pi` holds `Pi` (id `pi`) and
   `PiConfig { agent_dir, pi_bin }`. `agent_dir` is `PI_CODING_AGENT_DIR`,
   else `~/.pi/agent`. A spec picks pi
   with `"harness": "pi"` on a member or the team. Wiring never picks pi as
   the lead's harness, even when `TEAMWIRE_LEAD_HARNESS=pi`, and
   `current_session` returns `None`, so a pi session can never prove a lead.
   `teamwire install` adds nothing for pi.

2. **Capabilities.**

   | Field | pi |
   | --- | --- |
   | `turn_end_hook` | yes, through the extension (point 4) |
   | `in_turn_hook` | yes, through the extension |
   | `mcp_client` | yes |
   | `choose_session_id` | yes (`--session-id`) |
   | `can_push` | no |
   | `session_env_via_config` | yes, through the extension |
   | `reads_watch_lines` | no |

3. **A member's start.** `launch_spec` returns:

   ```
   pi --session-id <id> --session-dir <agent_dir>/sessions/teamwire
      --no-approve
      --extension <member_dir>/pi-extension.ts
      --append-system-prompt <contract file>
      [--model <model>] [--thinking <effort>]
   ```

   - `--append-system-prompt` reads the file when the path exists, and
     otherwise appends the text itself. So launch refuses a missing
     contract rather than send its path as the prompt.
   - `--no-approve` skips the folder's own `.pi` settings, MCP servers,
     extensions and prompt files, which would run as the member with no
     review (the case INV-25 refuses for Claude). With it, pi opens no
     project-trust dialog, so the spec's `accept_trust` has nothing to
     answer. `AGENTS.md` and `CLAUDE.md` still load. The user's own
     `~/.pi/agent` packages and extensions still load, because a member's
     model provider can be one of them.
   - All members' sessions go to one folder, so a session is found from its
     id alone, and members stay out of the user's per-folder session lists.
     pi writes that folder; teamwire only reads it.
   - The spec's `plugins` map does not apply: pi has packages, not plugins.
   - `resume_spec` is the same command with the session's id: pi's
     `--session-id` opens a session that exists.

4. **The extension.** Launch writes `<member_dir>/pi-extension.ts` (mode
   0600), generated from a template with one JSON literal holding the
   member's values. The extension:
   - sets `TEAMWIRE_TEAM`, `TEAMWIRE_ROLE`, `TEAMWIRE_HOME` and
     `TEAMWIRE_TOKEN_FILE` in `process.env` when it loads. pi's bash tool
     copies `process.env` for each command, so every shell command the
     member runs has them.
   - registers the MCP server `teamwire` (`<bin> [--home H] mcp --team T
     --role R`, env `TEAMWIRE_TOKEN_FILE`) with `pi.registerMcpServer`.
   - on `agent_before_settle` with outcome `completed`, runs
     `<bin> [--home H] hook turn-end --team T --role R` with
     `{session_id, hold_active, final_message, session_file}` on stdin.
     `final_message` is the text of the newest assistant message from
     `agent_end`. A reply `{"decision":"block","reason":<note>}` becomes a
     displayed `custom_message` entry with `continue: true`, which pi sends
     to the model as one more turn. `hold_active` is true for the settle
     that follows such a hold. An aborted or failed run runs no hook.
   - on `tool_result` of a call the model made (no `parentToolCallId`),
     runs `hook post-tool` and appends its `additionalContext` to the
     result's content.
   - writes a state file, `<agent_dir>/sessions/teamwire/<id>.teamwire.json`,
     holding `{pid, started_at_ms, state}`. The extension replaces the whole
     file through a rename at each change, so a reader never sees half of
     one. `state` is `idle` at `session_start` and `agent_settled`,
     `working` at `agent_start`, `blocked` while an extension's dialog waits
     (`ui_prompt_start` to `ui_prompt_end`), and `ended` at
     `session_shutdown`, which fires on a quit and on a switch to another
     session. pi writes its own session file only at the first message, so
     this file is how teamwire finds the process before then.
   - at each `turn_end`, adds a `custom` entry `teamwire.fill`
     `{tokens, window}` from `ctx.getContextUsage()` to the session. pi keeps
     `custom` entries out of the model's context.
   - gives each hook 60 s (`hook_timeout_s`) and kills it after. A hook that
     fails, prints nothing or prints junk lets the turn go on.

   The token travels as for Codex (ADR 0023 point 4): launch writes it alone
   to `<member_dir>/token` (0600), and the extension names only that path.

5. **Liveness and state.** The session file is
   `<agent_dir>/sessions/teamwire/<time>_<id>.jsonl`, and its first line's
   `cwd` answers `session_cwd`. `processes_for_session` reads the state file
   and returns its pid when the state is not `ended` and
   `Platform::process_alive` confirms the pid and the start time. `is_alive`
   is then `Alive` with the recorded state (`Idle`, `Working`, `Blocked`).
   It is `Dead` when the state is `ended` or the process no longer runs,
   and `Unknown` with no state file. So a member started with
   `"dispatch": false`, or whose first message did not arrive, can be
   reached at once. And under tmux, whose host reports no state, close reads
   an idle pi member as idle instead of refusing it as unreadable.

6. **The screen.** pi draws its input box as plain text between two rules
   of `─`, with a footer whose stats line reads `0.0%/1.0M` (or `?/1.0M`).
   `InputPrompt` needs the box, a top rule with no status in it, and that
   footer line. A working run puts its status in the top rule
   (`── ⠋ Working ───`). A dialog takes the box's place and prints a key
   hint (`↑↓ navigate`, or `enter …` with `… cancel`), with `→ ` on the
   chosen option. Text in the box is typed text, since pi draws none of its
   own there. A long line can wrap inside a word, so `line_pending` compares
   text with whitespace removed. A paste over 1000 characters shows as
   `[paste #N <count> chars]`, which counts only when the count is the
   line's own UTF-16 length. `reads_input_box` is true.

7. **Startup.** `answer_startup` waits for the input box and answers no
   dialog. Any dialog is refused.

8. **herdr.** herdr 0.9.3 starts pi with `--kind pi`; the herdr adapter now
   passes `pi` through. herdr learns a pi pane's session from its own pi
   extension, `~/.pi/agent/extensions/herdr-agent-state.ts`, which
   `herdr integration install pi` adds. Doctor warns when `pi` is on PATH
   under herdr and that file is missing. teamwire does not install it.

9. **Not built.** A pi lead. OpenTelemetry settings for pi members
   (`otel_env` is empty). A compaction backstop like LCH-29's: pi compacts
   by its own settings.

## Consequences

- The turn-end event of a pi member is named `pi-stop`, from the existing
  `<harness id>-stop` rule.
- The style-code reader that the Claude adapter had moved to
  `teamwire_core::screen::visible`, so both adapters read styled screens the
  same way.
- A person who runs `/new` or `/resume` inside a member's pane moves the
  pane to another session. The old session's state file then says `ended`,
  so teamwire reports the recorded session dead, and `teamwire reopen`
  resumes it.
- A pi that is killed (`kill -9`) leaves its last state in the file. The
  pid check then reports the session dead.
- Facts most likely to be wrong, to check first in a live team:
  1. herdr's pi integration reports the pi session id as `agent_session`.
     Without that, every delivery to a pi member under herdr fails
     `host_no_session`.
  2. A `custom_message` entry with `continue: true` from
     `agent_before_settle` reaches the model as the next turn's input.
  3. pi's context reading (`ContextUsage.tokens`) counts what Claude's
     figure counts closely enough for the handoff point.
  4. `agent_settled` fires after every run, including an aborted one, so a
     member never stays `working` after its run ends.
