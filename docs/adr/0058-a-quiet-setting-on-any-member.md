# ADR 0058: A `quiet` setting makes a member of any kind quiet

Status: accepted, 2026-11-20. Extends MSG-3 from a list of kinds to a
property of each member. ADR 0025 stands: a planner is an `investigator`,
and teamwire has no `planner` kind. ADR 0057 adds the column in the same
schema step.

## Context

A quiet member sends no message, and only its own lead may message it
(MSG-3). Until now the kind alone decided this: a reviewer, a plan reviewer
and a closer were quiet, an implementer and an investigator talked. Four
places in code asked the kind: the sender check and the recipient check in
`crates/teamwire-msg/src/rules.rs`, the check for another team's member in
`rules/foreign.rs`, and the roster in `roster.rs`.

Step 2 of the work on sets (ADR 0057) runs a set of planners. Each planner
writes a plan for the same task on a model of another family, and a
reconciler then joins the plans. A planner that reads another planner's
messages stops being independent, so the user decided on 2026-11-19 that
planners in a set must be quiet by code, not only by their brief.

A planner is an `investigator` (ADR 0025, decision 7). ADR 0025 rejected a
planner kind because the investigator seat already forbids editing
repository files. That point still holds: the investigator seat fits a
planner in every way except that it talks.

## Decision

The user chose the setting over a kind on 2026-11-19.

1. **A spec member may carry `"quiet": true`.** Code then treats that member
   as quiet, whatever its kind. It may send no message of any type, and
   only its own lead may message it. A message to it from another member,
   from another team or from the user is refused with
   `recipient_not_allowed`.
2. **A planner is an `investigator` with `"quiet": true`.** No `planner`
   kind is added. The investigator seat already gives a planner every rule
   it needs, and ADR 0025's reason for having no planner kind stands. The
   `reconciler` is a kind (ADR 0057), not a quiet investigator, because its
   rules are the part a whole set depends on and belong in a seat file.
3. **`"quiet": false` on a kind that is quiet by nature is refused** with
   `quiet_cannot_be_unset`. A reviewer with `"quiet": false` would read as
   a reviewer that talks, and the rules keep no such promise. A member with
   no `quiet` key is accepted, and so is `"quiet": true` on a quiet kind.
4. **One place answers whether a member is quiet.** `Member::is_quiet`
   answers true when the member's kind is quiet or its stored `quiet` is
   true. `Member::may_send(type)` answers what the kind may send, and
   nothing when the member is quiet. The four places above ask the member
   through these two functions. `MemberKind::is_quiet` stays and answers for
   the kind alone.
5. **Every JSON output gives `quiet` in effect.** `teamwire teams --json`,
   `teamwire members --json`, the `team_members` tool and `teamwire export`
   write the answer of `Member::is_quiet`. A reviewer shows `"quiet": true`,
   although its stored value is false. A reader of the JSON never needs to
   know which kinds are quiet. The dashboard offers no message control for
   a member whose roster entry says `quiet`. A reader of an older build's
   JSON reads a missing key as false and falls back on the kind.
6. **The seat text follows the setting.** `ContractParts` gains `quiet`. For
   a quiet member of any kind, `contract.rs` writes the quiet talking part
   in place of the four parts on sending messages, and the part that says
   to put a question in the report file under 'Questions for the parent' in
   place of the part that says to send one. The seat rewrite by
   `teamwire amend` keeps those parts.
7. **The events and the dry run name the setting.** Launch's
   `member.creating`, `member.started`, `member.failed` and `member.refused`
   events carry `"quiet": true` in their data when the spec sets it, next to
   `task_class`, and leave the key out otherwise. `teamwire launch
   --dry-run` prints `, quiet` on the member's line when the spec sets it,
   and prints nothing extra for a kind that is quiet by nature.

## The store

Schema version 13 stores the setting in
`members.quiet INTEGER NOT NULL DEFAULT 0 CHECK (quiet IN (0, 1))`. The
column holds what the spec said, not the answer in effect, so a reviewer's
row holds 0. ADR 0057 describes the step and how launch writes the value
with `Tx::set_member_marks`, on the insert and on the relaunch of a role
that failed before it started.

## Rejected

- **A `planner` kind.** It would repeat the investigator seat with one
  rule changed, and ADR 0025 already rejected it for that reason.
- **Quiet by the brief alone.** A brief that says "send nothing" works only
  while the model obeys it, and the messages it sends still reach the
  other planners. The user asked for code to stop them.
- **Store the answer in effect.** The column would then hold two facts in
  one value, and a later change to which kinds are quiet would leave old
  rows wrong. The stored value is the spec's, and `Member::is_quiet` works
  out the rest.
- **The name `silent`.** `teamwire watch` already prints `SILENT <role>`
  for a member that stopped responding. The setting is named `quiet` so
  the two never read as one thing.

## Consequences

- A lead can make any member quiet without a new kind, so step 2 needs no
  code change for its planners.
- Launch accepts `"quiet": true` on an implementer. Such a member can send
  no notice after a commit another member depends on, no finding, no
  question and no answer, and only its own lead can message it. Its seat
  text tells it to write its questions in its report file. Whether a quiet
  implementer is ever useful is still open.
- Code that decides silence must call `Member::is_quiet` or
  `Member::may_send`, never `MemberKind::is_quiet`. A check on the kind
  alone lets a quiet investigator talk.
