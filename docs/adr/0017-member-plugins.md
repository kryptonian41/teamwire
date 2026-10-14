# ADR 0017: Per-member Claude plugins

Status: accepted, 2026-10-14.

## Context

Teamwire starts each member's `claude` with no flag that narrows which
plugins load. So every member session loads every plugin the user has
installed. A "write less code" plugin (about 1,225 tokens at session start
and 715 more per subagent) and a 25-skill plugin now run in reviewers,
investigators and closers too, where they only cost tokens and steer the
work.

Claude Code reads `enabledPlugins` from the file given to `--settings`. A
per-run settings file with `"enabledPlugins": {"<id>": false}` turns that
plugin off for that one session and changes nothing else. The plugin's own
shared state under `~/.claude/` stays as it was.

One plugin offers an environment switch that turns it off by deleting a flag
file shared by every session in the repository. Teamwire never sets or
recommends that switch, because it changes sessions it did not start.

## Decision

1. **A `plugins` field.** The spec takes `plugins` on the team and on a
   member. It is an object from a Claude plugin id `name@marketplace` to
   true or false. The team's map applies to every member of the same spec
   file. For each key, the member's value wins. The store keeps no team map,
   so a spec passed to `teamwire launch --add` must repeat it.
2. **Validation.** A key must be `name@marketplace`. Both parts must be
   non-empty and use only ASCII letters, digits, `.`, `_` and `-`. A bad key
   refuses the launch with code `bad_plugin_id` and names the key.
3. **Where it goes.** Launch merges the two maps into `MemberLaunch.plugins`.
   The Claude harness writes the result as `enabledPlugins` in the member's
   `settings.json`. An empty map writes no key, so an existing team gets a
   byte-identical file. The argv does not change. A resumed member reuses the
   file written at launch.
4. **A generic map, not defaults in code.** No plugin id appears in Rust.
   Which kind of member gets which plugins is a choice about prompts, so it
   lives in the presets and the lead skill, where it can change without a
   release.
5. **Claude only.** The field is Claude-specific. Another harness ignores
   `MemberLaunch.plugins`.

## Consequences

- A lead can switch a plugin off for one role or one kind of role without
  touching the user's installation.
- A skill installed as a plain user skill under `~/.claude/skills/` is not a
  plugin. This field does not cover it.
- Unknown keys in a spec are still dropped without a warning, as before.
- The `--json` output of the CLI does not change shape, so
  `ui/src/shared/teamwire.ts` is unchanged.
