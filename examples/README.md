# Examples

Files for you to copy by hand, except the skills and the lead's MCP server,
which `teamwire install` puts into the teamwire plugin (ADR 0018, ADR 0023).
No command writes under `~/.claude/`. `teamwire install` only deletes the
skill file that an old build (ADR 0012) put in `~/.claude/skills/teamwire/`,
and its MCP entry in `~/.claude.json`.

| Path | What it is | Where it goes |
| --- | --- | --- |
| `presets/*.json` | The six team shapes. `teamwire preset` prints them. | Copy one, fill in the `CHANGEME` and `/abs/path` values, leave out each member that has a `_launch` or `_launch_after` note and add it later with `--add`, then `teamwire launch --spec FILE`. |
| `skills/teamwire/SKILL.md` | The lead's procedures, written for `teamwire` commands and MCP tools. | Run `teamwire install`. It puts the skill, compiled into the binary from this file, into the teamwire plugin under `$TEAMWIRE_HOME/claude-plugin/`. Claude Code names it `teamwire:teamwire`. |
| `skills/teamwire/reference.md` | The lead's lookup tables: watch lines, flags, model advice. `SKILL.md` links to it. | `teamwire install` puts it beside `SKILL.md` in the plugin. |
| `skills/afk/SKILL.md` | The procedure for an unattended AFK run. | `teamwire install` puts it in the Claude Code plugin. Claude Code names it `teamwire:afk`. Codex has no AFK skill. |
| `skills/codex-lead.md` | The changes a Codex lead makes when it reads `SKILL.md`. | `teamwire install --harness codex` appends it to the Codex plugin's `SKILL.md`. |
| `mcp/lead.json` | Registers `teamwire mcp --lead`, the MCP server the lead uses. | `teamwire install` carries this server in the teamwire plugin. Claude Code names its tools `mcp__plugin_teamwire_teamwire__<tool>`. A `teamwire` entry in a project's `.mcp.json` starts a second server beside the plugin's, so do not merge this file into one; `teamwire doctor` warns when it finds such an entry. |

The setup steps, and how to undo them, are in `docs/ARCHITECTURE.md`, section
"Installing teamwire".
