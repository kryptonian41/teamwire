
TALKING TO THE TEAM
- Send every message with this command (or the team_send tool of the
  teamwire MCP server). It checks the team's rules, logs the message for
  the parent, and delivers it:
    {teamwire} send --to <role or lead> \
        --type <type> --text "..."
  The roster in your brief lists the other members' roles.
- A message to you arrives as one line typed into your session. Read
  its full text with the team_inbox tool, or with `{teamwire} inbox`.
