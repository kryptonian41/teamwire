# ADR 0030: pi can lead a team

Status: accepted, 2026-10-30. Supersedes ADR 0029 points 1 and 9 where they
say pi runs members only.

## Context

ADR 0029 made pi a harness for members and left a pi lead unbuilt. The user
now wants to lead teams from pi. A lead needs four things from its harness:

1. a way for teamwire's CLI and the lead's MCP server to name the caller's
   session, and to tell an interactive session from a background one;
2. a way for teamwire to find the lead's process;
3. a skill, an MCP server and a Stop hook, which the lead's setup carries;
4. a way to read `teamwire watch`, whose lines wake the lead.

pi 1.4.0 differs from Claude Code and Codex on each:

- pi gives `PI_SESSION_ID` and `PI_SESSION_FILE` only to commands of its
  bash tool. An MCP server gets no session variable, and pi sends no `_meta`
  with a tool call (`dist/extensions/mcp/tools.js`), so the Codex route of
  ADR 0023 point 8 is closed.
- pi sets `PI_CODING_AGENT=true` in its own environment, so every process it
  starts inherits it.
- pi has packages: `pi install <local dir>` lists a folder in
  `settings.json` and loads it from there, without copying it. A package
  holds extensions and skills.
- pi loads the user's packages in every session, including teamwire's
  members (ADR 0029 point 3), and no option turns off one package for one
  run.
- pi has no tool like Claude's Monitor. A command run through the bash tool
  holds the call until it ends or times out. An extension can start a
  process and hand text to the model with `pi.sendMessage(…, {triggerTurn:
  true, deliverAs: "followUp"})`, which starts a turn when pi is idle and
  waits for the end of the run when it is not.
- `ctx.mode` is `tui` only for the terminal UI.

The facts come from pi 1.4.0's docs and type declarations. A run of real pi
1.4.0 on 2026-10-30, with `PI_CODING_AGENT_DIR` set to a temporary folder,
showed `pi install` recording the package as a path relative to that folder,
`pi --help` loading it without an error, and `pi --mode rpc` starting a
session in which the lead extension wrote its state file with `mode: "rpc"`,
started `mcp --lead`, and on `new_session` ended the first state file and
wrote a second. In that run the MCP server's parent pid was the pid in the
state file, with pi's managed install, whose `pi` command is a wrapper
script. The watch tool, the bash refusal and the Stop hook were run
by loading the generated file with pi's own `jiti` against a mock of pi's
API and a fake `teamwire`. No model turn ran.

## Decision

1. **pi can lead.** Wiring picks `pi` from `TEAMWIRE_LEAD_HARNESS=pi`, else
   after the Claude and Codex session variables when `PI_CODING_AGENT` is
   `true`. pi's marker comes last because a Claude or Codex session started
   from a pi shell inherits it. A member with no `harness` runs on the
   lead's harness (ADR 0023 point 1), so on pi under a pi lead.

2. **The lead package.** `teamwire install --harness pi` writes
   `$TEAMWIRE_HOME/pi-package/`:

   ```
   package.json                 the manifest, name teamwire-lead
   extensions/teamwire-lead.ts  the lead extension (point 3)
   skills/teamwire/SKILL.md     the shared skill with a pi part
   skills/teamwire/reference.md
   skills/promptwire/SKILL.md
   ```

   It then runs `pi install <dir>`, with `PI_CODING_AGENT_DIR`, `PI_OFFLINE=1`
   and `PI_SKIP_VERSION_CHECK=1`. `uninstall` runs `pi remove <dir>` and
   deletes the folder. teamwire changes pi's setup only through these two
   commands, and reads `settings.json` to see whether the package is listed.
   pi loads the folder in place, so new files need only a restart of pi,
   never a second `pi install`. Doctor reports `pi lead package` when `pi` is
   on PATH: missing is `ok` and names the install command, files that are
   not this binary's are a warning.

3. **The lead extension.** The extension shares its state-file and hook code
   with the member extension of ADR 0029. At each `session_start` it checks
   for `TEAMWIRE_ROLE` or `TEAMWIRE_TOKEN_FILE` in `process.env`, which a
   member's own extension sets when it loads. When either is set, the
   extension does nothing in that session. Otherwise:
   - it registers the MCP server `teamwire` as `<bin> [--home H] mcp --lead`
     with `exposure: "direct"`, so the skill's `mcp__teamwire__team_*` tools
     are declared to the model;
   - it writes the state file of ADR 0029 point 4,
     `<agent_dir>/sessions/teamwire/<id>.teamwire.json`, adding `"lead":
     true`, `mode`, `session_file` and `cwd`;
   - when a run completes it runs `<bin> [--home H] hook lead-stop --harness
     pi` with the member hook's input, and a `block` reply holds the lead
     once, as for a member;
   - it registers the tool `teamwire_watch` (point 5);
   - it refuses a bash call that runs `teamwire watch <team>` without
     `--once`, with a reason that names the tool.

4. **How a pi lead proves itself.** `caller_session` first asks the
   platform for its own parent process. When a lead state file names that
   pid, is not `ended`, and the process runs with the recorded start time,
   that file's session is the caller's. This is how the lead's MCP server,
   a child of pi, finds the session, and a `/new` that ends one file and
   starts another moves it with no restart. Otherwise `PI_SESSION_ID` names
   the session, which counts only when its state file is a lead's and its
   process runs. The kind is `Interactive` only for `mode: "tui"`.
   `current_session` reads the process environment the same way.

5. **The watch runs inside the extension.** The tool `teamwire_watch`
   `{team}` starts `<bin> [--home H] watch <team>` as a child of pi. Each
   line it prints goes to the model through `pi.sendMessage` with
   `customType: "teamwire.watch"`, `triggerTurn: true` and `deliverAs:
   "followUp"`. The tool waits up to 2 s, so a watch that is refused at
   once, such as a second watch of the team, fails the call with its
   message. A watch that ends with a non-zero code later sends one line
   saying so. `session_shutdown` stops every watch of the process: a quit,
   `/reload`, `/new` and `/resume` all end them, and the lead starts them
   again from the session it then runs. pi's `reads_watch_lines` is
   therefore true, so launch prints `teamwire watch <team>` and does not run
   the locator check of ADR 0023 point 9 on a pi lead.

6. **Launch refuses a lead without its setup.** `Harness` gains
   `lead_setup_missing() -> Option<String>`, `None` by default. pi returns a
   reason when `current_session` finds no lead session. Launch of a new team
   and `new_team_lead` refuse with `lead_setup_missing` and that reason. A
   team that already runs is not checked again.

7. **Resume.** `lead_resume_argv` is `pi --session-id <id>`, which reopen
   runs from the lead's recorded folder. `session_cwd` and `context_fill`
   read the session file that the lead's state file names.

8. **Not built.** AFK runs from a pi lead (AFK-2 still refuses a lead that
   is not Claude Code). OpenTelemetry settings for pi.

## Consequences

- A test suite run from inside a pi session now wires pi as the lead of
  every `teamwire` the tests start, unless the test sets a Claude session
  variable. The suite passed that way on 2026-10-30, and also with pi's
  variables removed.
- The lead package loads in every pi member. It registers nothing there,
  but its code runs at each session start.
- A lead's watch lines no longer depend on typing into its pane. Results
  still reach the lead by typing when the host can type (ADR 0007).
- A live run on 2026-10-30 (pi 1.4.0 in the terminal UI on a private tmux
  server, the `claude-bridge` provider, one pi member) passed end to end.
  The lead read the package's skill, launched through bash, and started
  the watch with `teamwire_watch`. The member's result reached the lead by
  typing (`result.delivered`, route `host_typing`), so the locator found the
  pi lead's pane from its state file. `verify` passed and `close`
  finished. `TEAM-CLOSED` reached the idle lead as a watch message and
  started a turn. A second team, whose member had no task, showed
  `mcp__teamwire__team_status` answering the lead through the MCP server,
  which proved the lead by its parent process.
- Facts still unchecked:
  1. A `sendMessage` with `triggerTurn` starts a turn in the terminal UI
     while the person has text in the input box, without taking that text.
  2. A watch line queued with `deliverAs: "followUp"` during a run reaches
     the model when that run ends.
  3. A lead-stop `block` reply holds a pi lead for one more turn. No live
     run had working members with no watch at the end of a lead's run.
  4. The parent of the lead's MCP server is the pi process itself in an
     npm install of pi, as it is in the managed install.
