# Examples

Files for you to copy by hand. teamwire never installs, registers or edits any
of them, and never writes under `~/.claude/`.

| Path | What it is | Where it goes |
| --- | --- | --- |
| `presets/*.json` | The six team shapes. `teamwire preset` prints them. | Copy one, fill in the `CHANGEME` and `/abs/path` values, then `teamwire launch --spec FILE`. |
| `skills/teamwire/SKILL.md` | The lead's procedures, written for `teamwire` commands and MCP tools. | Copy the `teamwire` directory to `~/.claude/skills/teamwire/`. |
| `mcp/lead.json` | Registers `teamwire mcp --lead`, the MCP server the lead uses. | Merge its `mcpServers` entry into the MCP settings of the lead's Claude Code session, for example your project's `.mcp.json`. |

The cutover steps, and the way back, are in `docs/ARCHITECTURE.md`, section
"Migration and cutover".
