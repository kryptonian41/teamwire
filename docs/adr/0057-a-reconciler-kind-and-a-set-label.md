# ADR 0057: A reconciler kind joins the reports of a set, and a set label names the set

Status: accepted, 2026-11-20. Adds a sixth member kind to the five of
ADR 0034, and a new column to `members`. Changes no decision of an earlier
ADR. ADR 0058 adds the `quiet` setting in the same schema step.

## Context

A teamwire review has one reader. One model misses defects that a model of
another family finds, and a lead cannot tell which defects its one reader
missed. The user decided on 2026-11-18 and 2026-11-19 to let a lead run a
**set**: two or three members that do the same judging job on the same
brief, each on a model of a different family (ADR 0056), and that never
talk to each other. Step 1 of this work covers sets of reviewers and sets
of plan reviewers.

A set leaves the next step with two or three reports that overlap and
disagree. An owner that gets three reports fixes the same defect three
times, or fixes a finding that only one weak reader raised. So the user
also decided that every set ends with one member that reads the set's
reports, checks each finding, and writes the one report the next step acts
on. That member is the **reconciler**.

The reconciler's rules are the part the whole set depends on. A reviewer's
rules (`crates/teamwire-launch/contracts/kind_reviewer.md`) tell it to find
defects in a diff. A reconciler finds nothing new. It merges duplicates,
checks each finding in the code, and gives each a verdict. Two of its rules
go against what a model does by default: agreement between readers is not
evidence, and a judgement call goes to the parent undecided. A brief can
forget rules like these. A seat file cannot.

A later `launch --add` must also know which members form a set. The lead
adds the readers first and the reconciler after they have reported, so the
members of one set start in different launches.

## Decision

The user made decisions 1 and 2 on 2026-11-19. The rest follow from them.

1. **A sixth `MemberKind`, `reconciler`.** Its seat text is
   `crates/teamwire-launch/contracts/kind_reconciler.md`. It reads every
   report its brief names, by path and under role names only. It merges
   findings about one defect, checks each finding in the code (or in the
   plan, the briefs and the code they point at), and gives each one verdict:
   CONFIRMED, NOT CONFIRMED or JUDGEMENT. It writes one section per owner
   and ends with one table of every finding, its owner, its `file:line`, the
   readers that raised it and its verdict. A later study reads that table to
   see what each family added. When the readers wrote plans, it settles
   each disagreement about a fact from the code and sends each disagreement
   about judgement to the parent.
2. **The reconciler is quiet by its kind** (MSG-3). `MemberKind::is_quiet`
   answers true for it, as for a reviewer, a plan reviewer and a closer. It
   sends no message, and only its own lead may message it. A reconciler that
   hears from a reader learns which reader holds which view, and the merge
   then follows the loudest reader, not the evidence.
3. **It runs in `pane` mode only.** It edits nothing and commits nothing.
   Launch refuses it in `worktree` mode with `reconciler_needs_pane`, as it
   refuses a plan reviewer with `plan_reviewer_needs_pane`.
4. **It may run a test in a clone of its own.** The clone lives at
   `$TEAMWIRE_HOME/cache/closer/<team>/review-<role>`, the folder a reviewer
   uses, so `teamwire close` deletes it with no new code.
5. **The away-from-keyboard model table gives it the reviewer's row**
   (`crates/teamwire-launch/src/afk.rs`). A preset that classes it should
   use the task class `judgement` (ADR 0033), as for a reviewer.
6. **Every spec member may carry a set label.** `"set": "rev1"` puts the
   member in the set `rev1` of its team. Launch refuses a label that does
   not match `^[a-z][a-z0-9_]{0,15}$`, the pattern of a role name, with
   `set_label_invalid`. The word `lead` is a valid label, because a label is
   never an address. Launch stores the label with the member, so a later
   `launch --add` reads the earlier members of the set from the store.
   Launch only stores and prints the label in this step. The warnings about
   a set read it later.
7. **The events name the label.** The `member.creating`, `member.started`,
   `member.failed` and `member.refused` events of launch carry `set` in
   their data when the spec sets one, next to `task_class`. The events of a
   member with no label are unchanged. `teamwire launch --dry-run` prints
   `, set <label>` on the member's line.
8. **The JSON carries the label.** `teamwire teams --json`, `teamwire
   members --json`, the `team_members` tool and `teamwire export` write a
   member's `set` as a string or `null`. A reader of an older build's JSON
   reads a missing key as `null`.

## The store

Schema version 13 rebuilds `members`, as version 9 did (ADR 0034). The
CHECK list of `members.kind` gains `reconciler`, and SQLite cannot change a
CHECK in place. The step follows the same procedure: `migrate` turns
`foreign_keys` off before the transaction, the step creates `members_new`,
copies every row column by column, drops `members` and renames
`members_new`, and `PRAGMA foreign_key_check` runs before the commit. The
references from `completions`, `verify_records` and `waivers` name
`members` throughout and never change.

The same step adds two columns at the end of the table, so every older
column keeps its place:

- `quiet INTEGER NOT NULL DEFAULT 0 CHECK (quiet IN (0, 1))`, for ADR 0058.
- `set_label TEXT`. The column is named `set_label` because `SET` is an SQL
  keyword. A copied row takes NULL.

`NewMember` gains no field, because about 40 test files in ten crates build
it by a struct literal. Launch writes both values with
`Tx::set_member_marks(role, quiet, set)` in the same write transaction,
right after the insert. A role that failed before it started is launched
again through `relaunch_failed_member_with`, which updates the old row.
Launch writes the marks on that path too, from the new spec, so a
relaunched member never keeps the old row's values.

A read-only open of a version 12 database by this build reads `members`
through a temporary view that supplies the two new columns, and the reader
takes a NULL `quiet` as false. A view has no rowid, so `ReadTx::members`
orders by the rowid of `main.members` instead. A build at schema 12 refuses
a database at schema 13 with `SchemaNewer`, as after every earlier step.
The tests are in `crates/teamwire-store/tests/migrate_v13.rs`.

## Rejected

- **An `investigator` with a brief that says "send nothing" as the
  reconciler.** This needs no code, and this plan's own briefs were joined
  this way, because the installed build had no reconciler kind. The brief
  alone keeps such a member silent, and the reconciler's rules would live in
  every brief that a lead writes, so a lead could leave one out.
- **Widen the closer's rules to cover a set.** A closer verifies every claim
  of a whole team after the work. A reconciler joins the reports of one set,
  sometimes before any code exists. One seat for both jobs would also put
  both in one row of the outcome report, which groups members by kind.
- **Find a set from the brief file alone.** Members with the same brief file
  could form a set with no new field. A lead may give one brief to two
  members that are not a set, and a later warning about a set could not
  tell the two cases apart.
- **A table of sets.** A set has no data of its own beyond its label. One
  column on the member holds it.
- **Check the label with `Role::parse`.** That parse also refuses `lead`,
  which is a reserved address and harmless as a label.

## Consequences

- A lead can run a reviewer set or a plan-reviewer set and get one report
  back, with a verdict on every finding.
- The outcome report shows reconcilers in their own rows.
- An older build cannot open a team that this build migrated, as after
  every schema step. A team that an older build is still running must be
  closed before this build touches it.
- Every exhaustive match on `MemberKind` names the new kind. A new kind
  after this one touches the same places: the seat text in `contract.rs`,
  the table in `afk.rs`, the role table of the teamwire skill, the
  dashboard's kind list and the CHECK list of a new schema step.
