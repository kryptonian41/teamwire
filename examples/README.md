# Examples

Files for you to copy by hand, except the skill and the lead's MCP server,
which `teamwire install` writes for you (ADR 0012). No other command writes
under `~/.claude/`.

| Path | What it is | Where it goes |
| --- | --- | --- |
| `presets/*.json` | The six team shapes. `teamwire preset` prints them. | Copy one, fill in the `CHANGEME` and `/abs/path` values, then `teamwire launch --spec FILE`. |
| `skills/teamwire/SKILL.md` | The lead's procedures, written for `teamwire` commands and MCP tools. | Run `teamwire install`. It puts the skill, compiled into the binary from this file, into the teamwire plugin under `$TEAMWIRE_HOME/claude-plugin/`. Claude Code names it `teamwire:teamwire`. |
| `mcp/lead.json` | Registers `teamwire mcp --lead`, the MCP server the lead uses. | `teamwire install` carries this server in the teamwire plugin. Claude Code names its tools `mcp__plugin_teamwire_teamwire__<tool>`. A `teamwire` entry in a project's `.mcp.json` starts a second server beside the plugin's, so do not merge this file into one; `teamwire doctor` warns when it finds such an entry. |

The setup steps, and how to undo them, are in `docs/ARCHITECTURE.md`, section
"Installing teamwire".
