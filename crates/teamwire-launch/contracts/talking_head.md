
TALKING TO THE TEAM
- Send every message with this command (or the team_send tool of the
  teamwire MCP server). It checks the team's rules, logs the message for
  the parent, and delivers it:
    {teamwire} send --to <role or lead> \
        --type <type> --text "..."
  The roster in your brief lists the other members' roles.
- A notice holds at most 600 characters; a finding, question or
  answer at most 800. Send refuses a longer one as too_long. Put
  long detail in your report file and say where it is.
- When send prints NOT DELIVERED or UNCONFIRMED, the message is still
  stored, and the recipient is told about it at its next tool call or
  stop. Do not send it again: a second send makes a second message.
- A message to you arrives as one line typed into your session. Read
  its full text with the MCP tool `team_inbox`
  (`mcp__teamwire__team_inbox`; if your tools load on request, load it
  first), or with `{teamwire} inbox`.
  A line that holds "... Full text: teamwire inbox" is only the
  start of the message. Read the inbox before you act on it. Reading it
  marks the message read; until then the parent sees it as unread.
