## You are leading from pi

This skill was written for Claude Code. Read it with these changes, which
hold for every section below.

- **Start pi in a herdr or tmux pane**, with teamwire's lead package loaded.
  `teamwire install --harness pi` adds the package, and pi loads it at start.
  Without it, a launch of a new team refuses with `lead_setup_missing`.
- **Your tools** are `mcp__teamwire__team_send`, `mcp__teamwire__team_resend`,
  `mcp__teamwire__team_status`, `mcp__teamwire__team_members`,
  `mcp__teamwire__team_log`, `mcp__teamwire__team_teams` and
  `mcp__teamwire__team_decisions`. They are already
  loaded. There is no ToolSearch step.
- **The watch.** pi has no Monitor tool. Start the watch with the
  `teamwire_watch` tool and the team's name, right after launch. Each line
  the watch prints reaches you as a message from teamwire. A line arrives
  once the tool calls of your current turn have finished, and starts a turn
  when you are idle, so you never poll. Wherever this skill says to start
  `teamwire watch <team>` with Monitor, call `teamwire_watch` instead. Never
  run `teamwire watch` through bash: the call would hold until it timed out,
  and the lead package refuses it. Where this skill quotes a line that names
  Monitor, teamwire prints these to you instead:

  ```
  watch: call the teamwire_watch tool with team <team>
  [teamwire:<team>] no watch is running; start it with the teamwire_watch tool (team "<team>") before you stop.
  (no watch is running: call the teamwire_watch tool with team <team>)
  no watch is running: start one with the teamwire_watch tool (team <team>)
  ```

  The first comes from launch, the second from your Stop hook, the third
  ends a member's result line, and the fourth follows a reprompt.
- **Pull requests.** Watch them with the `teamwire_pr_watch` tool, wherever
  this skill says to run `teamwire pr-watch` with Monitor. It takes `repo`
  (`OWNER/REPO`, required), `prs` (the pull request numbers, required) and
  `interval` (seconds, optional; the command's default when left out). Each
  line it prints reaches you as a message from teamwire, as a watch line
  does. The lead package refuses `teamwire pr-watch` through bash, as it
  refuses `teamwire watch`.
- **When the watch ends.** A watch that printed `TEAM-COMPLETE` has exited,
  so call `teamwire_watch` again after `--add`. Quitting pi, `/reload`,
  `/new` and `/resume` stop the watches this session runs. After `/new` or
  `/resume` the team still names your old session: run
  `teamwire lead --adopt <team>`, then call `teamwire_watch` again.
- **This skill** is `teamwire`, and you load it with `/skill:teamwire`.
  The promptwire skill loads with `/skill:promptwire`.
- **Setup.** "Setting up a machine" in `reference.md` describes Claude
  Code. `teamwire install --harness pi` writes the package to
  `$TEAMWIRE_HOME/pi-package/`. Open pi sessions see a change after a
  restart. `teamwire doctor` checks it as `pi lead package`.
- **Members and models.** Under a pi lead, a member with no `harness` runs
  on pi. Its `model` is one of the ids or patterns `teamwire models` lists
  for pi, and it names its provider, as `provider/id`. pi lists a provider
  whenever it finds any credential for it, such as an `AWS_PROFILE` for
  Amazon Bedrock, even when the account cannot call that provider. Leave
  such a provider out with `TEAMWIRE_PI_EXCLUDE_PROVIDERS` in the
  environment, or with `TEAMWIRE_PI_EXCLUDE_PROVIDERS = "amazon-bedrock"` in
  `$TEAMWIRE_HOME/config.toml`.
- **No AFK runs.** An unattended AFK run (`/teamwire:afk`,
  `teamwire afk start`) needs a Claude Code lead. pi has no `afk` skill, so
  a request for one is a run you tell the user to start from Claude Code.
