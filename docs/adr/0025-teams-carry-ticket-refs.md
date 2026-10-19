# ADR 0025: A team carries ticket refs

Status: accepted, 2026-10-19.

## Context

A team has a title and a name, and nothing that says which ticket it works
on. Work that spans teams, such as one Linear ticket `GKA-580` worked on by
three teams over a week, has no link between the teams. To find every team
that touched a ticket, a person reads briefs and reports by hand. A survey of
the 72 archived teams on 2026-10-19 found this need, and found that no
archive holds a ref, because nothing could record one.

## Decision

1. **The team spec has a `refs` list.** Example: `"refs": ["GKA-580",
   "#3772"]`. The field is optional and defaults to an empty list.
   teamwire treats a ref as free text and never looks it up.
2. **A ref is 1 to 64 characters with no white space and no control
   character.** `GKA-580`, `#3772` and `knit-insights/teamwire#12` are valid.
   `GKA 580` and the empty string are refused. A team holds at most 20 refs
   after any `--add`.
3. **An exact repeat is recorded once, and order is first-seen order.** Two
   refs that differ in case are two refs.
4. **An invalid ref, or a total over 20, fails validation of the whole
   spec.** Launch exits 1 and starts nobody, as for an invalid role.
   `--dry-run` prints the same refusal, and otherwise prints the refs it would
   record or append.
5. **The first launch records the refs.** Schema version 5 adds the column
   `team.refs`, a JSON array of strings that is never NULL and defaults to
   `[]`. The `team.created` event's data holds the same list as `refs`.
6. **`launch --add` appends the refs not yet recorded.** The append and a new
   event, `team.refs_added` with data `{"added": [...]}`, happen in one
   `Store::write` transaction. An `--add` that names no new ref writes no
   event. A ref is never removed, and a ref on a member is out of scope.
7. **Schema version 5 also rebuilds `events`** with the longer type list, the
   way version 4 did for `delivery.reentered`, because SQLite cannot change a
   CHECK in place. Every row and the `AUTOINCREMENT` counter carry over. A
   read-only open of a version 4 database reads `refs` as empty.
8. **`teamwire status` and `teamwire teams` show the refs.** Both print them
   on the team's line when there are any. Both `--json` outputs carry
   `"refs": [...]` for each team, always present, and so do the MCP tools
   `team_status` and `team_teams`, which return the same JSON.
9. **The dashboard's schemas accept the field.** `ui/src/shared/teamwire.ts`
   reads `refs` with a default of `[]`, so output from an older teamwire still
   parses.

## Consequences

- A command that finds teams by ref, such as `teamwire decisions`, reads the
  list from the team row or from the archive's `team.refs` and needs no brief.
- The 72 existing archives hold no refs. A search by text still has to cover
  them, and a search by ref cannot.
- A database migrated to version 5 cannot be opened by an older binary. The
  same held for every earlier version.
- `teamwire metrics` and `teamwire export` do not label metrics with refs.
  The metrics keep their labels, and `team.refs_added` records no metric.
- A wrong ref cannot be removed. The user fixes it by closing the team and
  launching a new one, or by leaving it, because a later reader treats a ref
  as a hint and not as proof.
