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
  `TEAM-COMPLETE`.
- **This skill** is `teamwire:teamwire`, and you invoke it with `$`.
- **Members and models.** Under a Codex lead, a member with no `harness`
  runs on Codex. A member that keeps a Claude model (`sonnet`, `opus`, as the
  presets and the model table below name them) needs `"harness":
  "claude-code"`. A Codex member takes a Codex model name in `model`, or none
  to run on Codex's default. The `sonnet` and `opus` advice below is for
  Claude members.

- **No AFK runs.** An unattended AFK run (`/teamwire:afk`,
  `teamwire afk start`) needs a Claude Code lead. Codex has no `afk` skill,
  so a request for one is a run you tell the user to start from Claude Code.
