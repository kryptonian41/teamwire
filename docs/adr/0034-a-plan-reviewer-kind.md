# ADR 0034: A plan reviewer reads the plan and the briefs before implementers start

Status: accepted, 2026-11-04. Adds a fifth member kind to the four of the
first build. Changes no decision of an earlier ADR.

## Context

A teamwire member starts with its brief and nothing else. When a brief
leaves a design choice open, the member makes the choice itself, and two
members can make it differently. teamwire's own records show the cost:

- In `docs/research/2026-10-26-gains-complex-task.md`, a solo run with a
  brief that left out two interfaces passed 147 of 191 hidden tests. With
  the complete brief, every setup passed between 177 and 182.
- In the same study, one lead started a plan reviewer of its own accord. The
  plan reviewer found 9 gaps in the plan before any implementer ran.
- In `docs/research/2026-10-18-lead-field-reports.md`, 18 to 22 of 33 real
  member questions asked to edit a file outside the member's `owns`, and in
  five runs members answered one open question differently.

Today the lead runs the planning check on its own plan. Google's
Elephant-Goldfish Model (`docs/research/2026-11-03-elephant-goldfish-model.md`)
names the failure: the session that wrote a plan shares the plan's blind
spots. A reader that never saw the planning conversation finds what the
document leaves unsaid.

The `reviewer` kind cannot do this job. Its rules
(`crates/teamwire-launch/contracts/kind_reviewer.md`) tell it to read a code
diff and to give each finding a concrete failure in code. A plan has no code
diff yet. Bending the reviewer's rules to cover both jobs would also put plan
reviews and code reviews in one row of the outcome report
(`crates/teamwire-otel/src/outcomes.rs`), which groups members by kind.

## Decision

The user made these decisions on 2026-11-04:

1. **A fifth `MemberKind`, `plan_reviewer`.** One kind covers the plan and
   the briefs, because the briefs are part of the plan.
2. **The plan reviewer is quiet.** It sends no messages, and only its own
   lead may message it, exactly like a reviewer and a closer (MSG-3). A
   reader that hears from the team is no longer fresh.
3. **It reads the plan, the team spec, every brief and the repository.** The
   spec matters because it holds each member's `owns` and `check`.
4. **It reports a paraphrase first, then findings.** For each brief it
   writes, in two to five sentences, what the brief asks for. A wrong
   paraphrase shows the lead that the brief says something other than what
   the lead meant. Each finding names a brief's file and line. A finding is
   one of: a design choice the brief leaves to the member, a file the work
   must change that no member owns, two briefs or a brief and the plan that
   contradict each other, a claim about the code that the code does not
   bear out, a "Done when" item that no command or file can check, a
   member's check that misses a gate the repository's CI runs, or a team
   shape that does not fit the work. The last two come from the plan
   reviewer row of the AFK skill, which the user approved earlier. It
   proposes no fixes, as a reviewer does not.
5. **It runs in `pane` mode only.** It writes nothing, so launch refuses it
   in `worktree` mode with the refusal code `plan_reviewer_needs_pane`.
6. **Its task class is `judgement`** (ADR 0033), as for a reviewer and a
   closer. Its kind keeps its outcomes apart from theirs.
7. **The lead starts it first.** The lead launches a team with the plan
   reviewer alone, fixes the briefs from its report, then adds the
   implementers with `launch --add`. Launch needs no new ordering rule.
8. **AFK runs may use it.** The AFK model table gives it the same row as a
   reviewer. Which findings may stop an AFK launch with no human present
   stays open until trials show what the plan reviewer finds.

## The store

`members.kind` carries a CHECK list of the four kinds (schema v1). SQLite
cannot change a CHECK in place, so schema version 9 rebuilds `members` with
the longer list.

The rebuild differs from the `events` rebuilds of versions 4, 6, 7 and 8.
Three tables, `completions`, `verify_records` and `waivers`, refer to
`members (role)` by foreign key. With `foreign_keys` on, `ALTER TABLE members
RENAME TO members_old` rewrites those references to point at `members_old`,
and dropping `members_old` afterwards fails or leaves them dangling. A test
on 2026-11-04 with SQLite 3.54 confirmed both, with `legacy_alter_table` on
as well. So version 9 follows SQLite's documented twelve-step procedure for
a table that other tables refer to:

1. Turn `foreign_keys` off on the connection, before the migration
   transaction begins. SQLite ignores the pragma inside a transaction.
2. Inside the one migration transaction: create `members_new` with the
   longer CHECK list, copy every row column by column, drop `members`, and
   rename `members_new` to `members`. The child tables' references name
   `members` and never change.
3. Run `PRAGMA foreign_key_check` in the same transaction. Any row it
   returns fails the migration, and the transaction rolls back.
4. Commit, then turn `foreign_keys` back on, also when the migration failed.

The migration code turns the pragma off for every migration, not only for
version 9. Every step runs in one transaction with the foreign key check at
its end, so the check covers all of them.

A build at schema 8 refuses a database at schema 9 with `SchemaNewer`, as
for every earlier step. Version 9 adds no column, so a read-only open of a
version 8 database by a newer build needs no temporary view.

## Rejected

- **Widen the reviewer's rules to "a diff or a plan".** This needs no schema
  change. It mixes two jobs in one set of rules and one outcome row, and the
  reviewer's rules are narrow on purpose.
- **Run a reviewer on a branch that holds the plan and the briefs.** This
  needs no code change, and the research note suggested it for a first
  trial. Each finding's concrete failure would still have to be stretched to
  fit the reviewer's rules, and the outcomes would still mix.
- **Check the kind only in Rust and drop the CHECK.** Every enum column in
  the store has a CHECK that lists exactly the core enum's values, and a test
  holds it to that.
- **Edit the CHECK text through `PRAGMA writable_schema`.** SQLite allows
  this for loosening a constraint, but a wrong edit corrupts the schema with
  no error until the next open. The rebuild is longer and safe.

## Consequences

- A lead can review its plan with a fresh member before it spends an
  implementer's budget.
- The outcome report shows plan reviews in their own rows, so trials can
  compare teams with and without one.
- Each member's **Decisions I made** list and each member question after a
  plan review show what the review missed.
- An older build cannot open a team that a newer build migrated, as after
  every schema step.
