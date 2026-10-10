# Examples

Files for you to copy by hand, except the skill and the lead's MCP server,
which `teamwire install` writes for you (ADR 0012). No other command writes
under `~/.claude/`.

| Path | What it is | Where it goes |
| --- | --- | --- |
| `presets/*.json` | The six team shapes. `teamwire preset` prints them. | Copy one, fill in the `CHANGEME` and `/abs/path` values, then `teamwire launch --spec FILE`. |
| `skills/teamwire/SKILL.md` | The lead's procedures, written for `teamwire` commands and MCP tools. | Run `teamwire install`. It writes the skill, compiled into the binary from this file, to `~/.claude/skills/teamwire/`. |
| `mcp/lead.json` | Registers `teamwire mcp --lead`, the MCP server the lead uses. | `teamwire install` registers it for your user. A `teamwire` entry in a project's `.mcp.json` runs in place of the user one in that project, so do not merge this file into one; `teamwire doctor` warns when it finds such an entry. |

The cutover steps, and the way back, are in `docs/ARCHITECTURE.md`, section
"Migration and cutover".
