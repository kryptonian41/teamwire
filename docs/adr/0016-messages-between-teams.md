# ADR 0016: Messages between teams, and between any two agents that talk

Status: accepted, 2026-10-13. Changes MSG-1, MSG-3 and DEL-12, and adds
MSG-20 and MSG-21. Builds on ADR 0003 (delivery) and ADR 0008 (identity),
which stand.

## Context

Several teams can run on one machine, each with its own lead and its own
database at `$TEAMWIRE_HOME/teams/<team>/team.db`. Teams working on
neighbouring code need to share facts: a changed interface, a moved table, a
port. Before this change, a recipient was `lead` or a role of the sender's
own team, and a member's MCP server refused any other team, so no agent
could reach another team at all.

Inside a team, each kind could send only some types. An implementer sent no
`finding`, an investigator no `notice`, and a `question` could go only to the
lead or an investigator. Those limits stopped useful messages and protected
nothing. Reviewers and closers send nothing and hear only from the lead
(MSG-3). That rule protects something real: a review that peers cannot
steer.

## Decision

1. **Who talks.** Leads, implementers and investigators talk. Each of them
   sends `finding`, `question` and `answer` to any agent that talks, in any
   team. A member that talks also sends `notice`. The lead sends no notice,
   because a notice names a commit in the sender's checkout and the lead has
   none recorded. Reviewers and closers stay quiet: they send nothing, and
   only their own team's lead may message them. `reprompt` and `note` come
   only from a member's own lead.
2. **Address.** An agent of another team is `<role>@<team>`, and its lead is
   `lead@<team>`. Neither a role nor a team name can hold `@`, so every name
   has one reading. A name that points back at the sender's own team is read
   as the local name before any rule runs.
3. **Another team informs, never instructs.** `reprompt` and `note` never
   cross teams. A line from another team, even from its lead, carries the
   peer reminder of DEL-12 and names the sending team. Only a member's own
   lead is "the lead" in its pane.
4. **Two copies under one ULID.** The sender's team stores its copy as for
   any send. Its delivery row names `role@team`. The recipient's team stores
   its own copy under the same ULID, with the sender as `role@team`, its own
   display id, its own `seq` counter, and one `pending` row per local
   recipient. Because the question exists in both stores under one ULID, an
   answer's `re` holds in both.
5. **Order, and no two locks.** The sender's copy is committed first. Then
   the recipient team's copy is written, in a transaction that also checks
   that team is open. Then the line is typed, with no transaction open. Then
   the outcome is recorded in the recipient's team. Last, it is mirrored onto
   the sender's row. Each write is its own transaction in one store. No
   command ever holds two stores' locks at once, so two teams messaging each
   other cannot wait on each other.
6. **The recipient team's copy counts.** Its row moves to `acked` when the
   recipient reads it. The sender's row shows the last outcome the sender
   saw (`handed_off`, `unconfirmed`, `undelivered`) and never becomes
   `acked`.
7. **Repair by resend.** A resend from the sending team runs steps 5 and 6
   again. It writes a missing copy, tries a row still pending there, and
   only records a row already handed off there, without typing it again. A
   retry is counted in the recipient's team as a resend, so its stale clock
   restarts too. The recipient team's lead may resend its own copy. Its
   members may not, as for any message they did not send.
8. **Rules run in the sender's team.** The other team's store is opened
   read-only to check it exists and is open, and that the recipient exists,
   was not removed, and talks. The burst and repeat limits count the
   sender's own rows. The receiving store runs no rules on a copy it is
   given.
9. **Discovery.** `teamwire teams` and the `team_teams` tool list every team
   with its state and roster. `team_members` with `of`, and `teamwire
   members <team>`, read one other team. All of these open stores read-only.
   A caller is still proven only in its own team.
10. **Schema version 3.** No table changes. The version moves so that a
    binary that reads only `lead` and roles stops at "schema newer" instead
    of reading a team-qualified row as damaged.
11. **`to` on delivery events.** Every `delivery.*` event records its
    recipient as `to`. The events' role column is empty for the lead and for
    another team's agent alike, so a reader that keyed on the role alone
    would take one for the other.

## Consequences

- Teams can share facts without a human relaying them. The lead of each team
  still owns its work. A change to what another team builds goes through that
  team's lead.
- A sender's log shows a message as sent to `role@team`. The recipient
  team's log shows it from `role@team`. Neither log shows whether the other
  side read it. That lives only in the recipient team's copy.
- A message to an unknown or closed team is refused before anything is
  stored. A team that closes after the sender's copy is stored leaves that
  row `undelivered` with `team_closed`.
- An older binary refuses a version 3 database. Its running watches and MCP
  servers stop with "schema newer" after the first write by this binary.
