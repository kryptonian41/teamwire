## You are leading from Codex

This skill was written for Claude Code. Read it with these changes, which
hold for every section below.

- **Start Codex as `codex --no-daemon`**, in a herdr or tmux pane. Without
  that flag teamwire cannot find your pane to deliver results, and a launch
  of a new team refuses with `lead_not_locatable`.
- **Your tools** are `mcp__teamwire__team_send`, `mcp__teamwire__team_resend`,
  `mcp__teamwire__team_status`, `mcp__teamwire__team_members`,
  `mcp__teamwire__team_log`, `mcp__teamwire__team_teams` and
  `mcp__teamwire__team_decisions`. They are already
  loaded. There is no ToolSearch step.
- **The watch.** Codex has no Monitor tool. Start the watch in the
  background:
  `nohup teamwire watch <team> --deliver >/dev/null 2>&1 &`
  Each watch line is typed into your pane as a message, as a member's result
  is. After `--add`, start the watch again only when it has printed
  `TEAM-COMPLETE`. Where this skill quotes a line that names Monitor, teamwire
  prints these to you instead:

  ```
  watch: nohup teamwire watch <team> --deliver >/dev/null 2>&1 &
  [teamwire:<team>] no watch is running; start it in the background with `nohup teamwire watch <team> --deliver >/dev/null 2>&1 &` before you stop.
  (no watch is running: nohup teamwire watch <team> --deliver >/dev/null 2>&1 &)
  no watch is running: start one with `nohup teamwire watch <team> --deliver >/dev/null 2>&1 &`
  ```

  The first comes from launch, the second from your Stop hook, the third
  ends a member's result line, and the fourth follows a reprompt.
- **Pull requests.** Watch them in the background too, with the team's name
  after `--deliver`:
  `nohup teamwire pr-watch --deliver <team> -R OWNER/REPO -i 45 3453 3454 >/dev/null 2>&1 &`
  Each line it prints is typed into your pane, under the same rules as the
  watch's lines.
- **A new session.** After `/new` or a resume into another session, the team
  still names your old one. Run `teamwire lead --adopt <team>`, then start
  the watch again.
- **This skill** is `teamwire:teamwire`, and you invoke it with `$`.
- **Members and models.** Under a Codex lead, a member with no `harness`
  runs on Codex. Its `model` is one of the ids `teamwire models` lists for
  Codex. "Choose each member's model" sets each member's harness from its
  model's family: `"harness": "claude-code"` for a Claude model and
  `"harness": "pi"` for a model of any other family. A member stays on Codex
  when pi's list does not hold its model and Codex's list does, or when the
  user names Codex.
- **Setup.** "Setting up a machine" in `reference.md` describes Claude
  Code. For Codex, `teamwire install --harness codex` writes the plugin to
  `$TEAMWIRE_HOME/codex-plugin/`, and Codex asks you to review its hook once
  after each change. `teamwire doctor` checks it. Run the install again
  after a new `cargo install`.

- **No AFK runs.** An unattended AFK run (`/teamwire:afk`,
  `teamwire afk start`) needs a Claude Code lead. Codex has no `afk` skill,
  so a request for one is a run you tell the user to start from Claude Code.
