# Feature Specification: Amend a running member's check and owns

**Feature Branch**: `dfix2/spec`

**Created**: 2026-11-13

**Status**: Draft

**Input**: User description: "Fix three teamwire problems found in team deckh's run (findings F13, F14, F15), each proven by a test that fails without the fix, with the lead guidance changed in the same change. US1 (P1): a lead changes a running member's `check` or `owns` with `teamwire amend <team> --role <role> [--check CMD] [--owns PATHS] --reason TEXT`. verify uses the new values from then on, and earlier verify records keep the check they ran. The member gets a `note` with the new values and the reason, and that note, shown by `teamwire log`, is the record of the change. No new event type and no schema change. The command is shell only, not an MCP tool (F13). US2 (P2): the speckit preset says the commit that removes spec-kit goes on the pull request branch, cut from the implementer's last verified head, never on the implementer's own branch, and that a lead whose removal landed there uses `amend` to swap the check (F14). US3 (P3): the lead skill's role table lists every git command each seat's rules ban, and a test fails when the table misses a ban that the seat contracts state (F15)."

The source of every finding is
`/Users/apoorvagarwal/Projects/teamwire-agent-local/deckh-findings-2026-11-10.md`.
Finding numbers F13, F14 and F15 below refer to that file. Team deckh's
evidence is in `/Users/apoorvagarwal/.teamwire/archive/deckh-2026-11-13`.

## Clarifications

### Session 2026-11-13

- Q: Does the amend note wake a member that has already recorded a result? → A: Yes, as every note does (ADR 0043). The note tells the member to run its new check and record its result again, and the watch tracks that turn.
- Q: Does a verify record made before an amend still count for `close --role`? → A: No. `close --role` needs a verify that started after the newest amend note to that member. The amend note's text starts with the fixed marker `CHECK AMENDED:`, and `close` finds the note by that marker, the way `SPEC AMENDMENT:` notices are recognised today. No schema change. `amend` also prints that verify must run again. A pass made with the old check must never vouch for the new one.
- Q: Does `--owns` replace the member's owns list or add to it? → A: It replaces the whole list, in the same shape as launch's `owns`. The note shows the old list and the new one.

## User Scenarios & Testing *(mandatory)*

The users here are two people. The **lead** is the agent session that runs a
team of member agents. The **operator** is the person who watches the lead
and answers its questions. A member's **check** is the shell command that
`teamwire verify` runs for that member. A member's **owns** is the list of
path patterns that member may change; verify flags a change outside it.

### User Story 1 - The lead changes a running member's check or owns (Priority: P1)

The lead writes each member's check and owns at launch. Today nothing can
change them afterwards. In team deckh the lead wrote an implementer's check
that ran every test file instead of two, and one flaky file outside the
team's work failed verify. The closer found the right command, but the
wrong one stayed for the rest of the run (F13).

With this story, the lead runs
`teamwire amend <team> --role <role> [--check CMD] [--owns PATHS] --reason TEXT`.
Every verify after that runs the new check and judges the diff against the
new owns. The member receives a `note` that states the new values and the
reason, so the member knows its scope changed. That note, shown by
`teamwire log`, is the team's record of the change.

**Why this priority**: A wrong check cannot be fixed today without closing
the member unverified. US2 depends on this command.

**Independent Test**: Launch a fake member with check A, run
`teamwire amend` with check B and a reason, then run `teamwire verify` for
the member. Verify runs B, the member's inbox holds a note naming B and the
reason, and `teamwire log` shows that note.

**Acceptance Scenarios**:

1. **Given** an open team with member `impl` whose check is A, **When** the
   lead runs `teamwire amend <team> --role impl --check B --reason "A runs
   every test file"`, **Then** the command exits 0, the next
   `teamwire verify <team> --role impl` runs B and not A, and `impl`'s inbox
   holds a `note` from the lead that names B and the reason.
2. **Given** member `impl` with owns `crates/a/`, **When** the lead amends
   it with `--owns crates/a/,crates/b/` and a reason, **Then** the next
   verify raises no `outside-owns:` flag for a changed file under
   `crates/b/`.
3. **Given** `impl` has a verify record made with check A, **When** the lead
   amends the check to B, **Then** that earlier record is unchanged: its
   head, verdict, exit code and log hash stay as stored.
4. **Given** the lead amended `impl`, **When** anyone runs
   `teamwire log <team>`, **Then** the amend note appears with its sender,
   recipient, the new values and the reason.
5. **Given** the lead runs `teamwire amend` with neither `--check` nor
   `--owns`, or without `--reason`, or with an empty reason, **Then** the
   command refuses, changes nothing, sends no note, and says what is
   missing.
6. **Given** the role is not a member of the team, or the team is closed,
   **Then** the command refuses, changes nothing and sends no note.
7. **Given** `impl`'s newest verify record at its head is `PASS`, made with
   check A, **When** the lead amends the check to B and runs
   `teamwire close <team> --role impl` without verifying again, **Then**
   close refuses and says verify must run again after the amend. **When**
   the lead then runs verify and it passes, **Then** close accepts `impl`.
8. **Given** `impl` has recorded `done`, **When** the lead amends it,
   **Then** the note wakes `impl` into a new generation, the note tells it
   to run its new check and record its result again, and `amend` prints
   that verify must run again.
9. **Given** a lead on Claude Code, Codex or pi reads its lead guidance,
   **Then** the guidance names `teamwire amend` and when to use it.

---

### User Story 2 - A team that removes spec-kit keeps a valid check (Priority: P2)

The `speckit` preset gives the implementer a check that starts with
`test -s specs/<feature>/tasks.md`. Team deckh removed every spec-kit file
before the pull request, as the user had decided, and made that removal
on the implementer's own branch. Every verify after that failed. A failed
verify cannot be waived, and `close --role` refuses an implementer without
a passing verify, so the lead had to close it with `--unverified` (F14).

With this story, the `speckit` preset says where the removal goes: on the
pull request branch, cut from the implementer's last verified head, never
on the implementer's own branch. The preset also says what a lead does when
the removal is already on the implementer's branch: run `teamwire amend`
to swap the check for one that no longer needs `tasks.md`, then verify
again.

**Why this priority**: The preset text prevents the dead end. `amend`
(US1) gives the way out when the text came too late.

**Independent Test**: Read the `speckit` preset as `teamwire preset speckit`
prints it. A test fails when the text loses the rule about where the
removal commit goes, or the rule that names `teamwire amend`.

**Acceptance Scenarios**:

1. **Given** a lead reads the `speckit` preset, **When** the team plans to
   remove spec-kit before the pull request, **Then** the preset says the
   removal commit goes on the pull request branch, cut from the
   implementer's last verified head, and never on the implementer's own
   branch.
2. **Given** the removal is already on the implementer's branch and verify
   fails on the missing `tasks.md`, **When** the lead follows the preset,
   **Then** the preset names `teamwire amend <team> --role <role> --check
   CMD --reason TEXT` and a check without the `tasks.md` test, and the next
   verify can pass.
3. **Given** the lead skill and its reference describe the spec-kit flow,
   **Then** they say the same as the preset about the removal commit.

---

### User Story 3 - The role table lists every git ban of each seat (Priority: P3)

Each member's seat rules are written into its system prompt at launch. The
lead skill has a role table that says what each kind of member may not do.
In team deckh the lead's brief told the implementer to build in a new
worktree. The implementer's seat rules forbid `git worktree add`, so it
switched branches in its own worktree instead. The role table listed that
ban only for the closer, so the lead did not know an implementer has it
(F15).

With this story, the role table's "May not" column lists every git command
that each seat's rules ban. A test reads the seat rules each kind receives
and the role table, and fails when the table misses a git ban that the
seat rules state.

**Why this priority**: The mismatch cost one confused turn, not a failed
verify. The fix is text plus one test.

**Independent Test**: Remove `git worktree add` from the implementer's row
of the role table and run the test. The test fails and names the kind and
the missing command.

**Acceptance Scenarios**:

1. **Given** the seat rules every member receives forbid `git worktree add`,
   `remove`, `move` and `prune`, **When** the lead reads the role table,
   **Then** each kind's row lists those commands.
2. **Given** the implementer's seat rules forbid `git stash`,
   `git reset --hard` and `git checkout -- .`, **Then** the implementer's
   row lists them, as it does today.
3. **Given** someone adds a git ban to one kind's seat rules and not to the
   role table, **When** the test runs, **Then** the test fails and names the
   kind and the command.
4. **Given** someone lists a git command for a kind whose seat rules do not
   ban it, **When** the test runs, **Then** the test fails and names the
   kind and the command.

---

### Edge Cases

- `amend` names the same check the member already has. The command still
  sends the note with the reason; verify behaves as before.
- `amend` targets a member that has already recorded a result, such as an
  implementer that said `done` before the spec-kit removal. The amend note
  wakes the member as every note does (ADR 0043): a new generation starts,
  the note tells the member to run its new check and record its result
  again, and the watch tracks that turn until the new result.
- `amend` targets a member whose newest record at its current head is a
  `PASS` made with the old check. That record no longer counts for
  `close --role`: close needs a verify that started after the newest amend
  note, even at the same head.
- `amend --owns` targets an implementer and the new list is empty. Verify
  then raises `no-owns`, as it does at launch for an empty list.
- `amend --owns` names a pattern that is not a valid glob. Verify raises
  `bad-owns:` for it, as it does for a launch value.
- `amend` targets a reviewer, closer or plan reviewer, which takes no
  messages. The check and owns still change; the note is stored and shown
  by `teamwire log`, following the rules for a note to that kind today.
- A waiver recorded before the amend. The amend changes no waiver; each
  waiver ends by its own rule (ADR 0045).
- The lead runs `amend` while a verify for the same member is running.
  The running verify keeps the values it read at its start; the next one
  uses the new values.

## Requirements *(mandatory)*

### Functional Requirements

**US1, amend (F13)**

- **FR-001**: teamwire MUST provide
  `teamwire amend <team> --role <role> [--check CMD] [--owns PATHS] --reason TEXT`,
  a lifecycle command that takes its arguments, refuses, exits and prints
  the way `launch`, `verify` and `close` do.
- **FR-002**: `amend` MUST refuse, change nothing and send no note when
  neither `--check` nor `--owns` is given, when `--reason` is missing or
  empty, when the team is not open, or when the role is not a member of the
  team. Each refusal names what is wrong.
- **FR-003**: `--owns PATHS` MUST replace the member's whole owns list,
  in the same shape as launch's `owns`. It never adds to the old list.
- **FR-004**: After `amend` exits 0, every `teamwire verify` that starts
  later MUST run the new check and judge the diff against the new owns.
- **FR-005**: `amend` MUST leave every earlier verify record, waiver,
  completion and message unchanged.
- **FR-006**: `amend` MUST send the member a `note` from the lead whose
  text starts with the fixed marker `CHECK AMENDED:` and states each
  changed field's old and new value and the reason. The note is the record
  of the change; `teamwire log` shows it like any note.
- **FR-006a**: The amend note MUST tell the member to run its new check and
  record its result again. A member that has already recorded a result is
  woken by the note as by any note (ADR 0043), and the watch tracks that
  turn.
- **FR-006b**: `close --role` MUST refuse a member whose newest amend note
  is newer than the start of the verify record it would accept, even at
  the same head, and the refusal MUST say that verify must run again after
  the amend. `close` MUST find the amend note by its marker, sender `lead`
  and type `note`, with no schema change. The plan decides whether the
  whole-team `close` applies the same rule.
- **FR-006c**: `amend` MUST print that verify must run again before the
  member can be closed.
- **FR-007**: `amend` MUST NOT add an event type or change the store's
  schema version.
- **FR-008**: `amend` MUST be a shell command only. The MCP server MUST NOT
  offer it as a tool.
- **FR-009**: `teamwire status` and `--json` outputs that show a member's
  check or owns MUST show the amended values after `amend`.
- **FR-010**: The lead skill, its reference, the AFK skill where it lists
  lead commands, `docs/api/cli.md`, the README and the lead guidance that
  teamwire writes for Codex and pi leads MUST name `teamwire amend`, in the
  same change as the command.

**US2, spec-kit removal (F14)**

- **FR-011**: The `speckit` preset MUST say that the commit that removes
  spec-kit goes on the pull request branch, cut from the implementer's last
  verified head, and never on the implementer's own branch.
- **FR-012**: The `speckit` preset MUST say that a lead whose removal is
  already on the implementer's branch runs `teamwire amend` to swap the
  check for one that does not test `tasks.md`, then verifies again.
- **FR-013**: The lead skill and its reference MUST say the same as the
  preset where they describe removing spec-kit.
- **FR-014**: A test MUST fail when the preset text loses either rule of
  FR-011 or FR-012.

**US3, git bans in the role table (F15)**

- **FR-015**: The lead skill's role table MUST list, in each kind's
  "May not" column, every git command that the seat rules of that kind
  ban.
- **FR-016**: A test MUST read the seat rules each kind receives at launch
  and the role table, and MUST fail, naming the kind and the command, when
  the table misses a ban the seat rules state or lists a ban they do not.

**All stories**

- **FR-017**: Every fix MUST have a test that fails without the fix.
- **FR-018**: A new ADR MUST record the `amend` decision. No existing ADR
  is edited.

### Key Entities

- **Member scope**: a member's check (one shell command, or none) and owns
  (a list of path patterns). Launch sets it. `amend` changes it. Verify
  reads it at the start of each run.
- **Amend note**: a `note` message from the lead to the member. Its text
  starts with `CHECK AMENDED:` and holds the old and new values of each
  changed field and the reason. It is the only record of the change, and
  `close` finds it by that marker.
- **Verify record**: one row per verify run of a member: head, verdict,
  exit code, flags, log hash. `amend` never changes one.
- **Seat rules**: the text launch writes into a member's system prompt for
  its kind. Some lines ban git commands.
- **Role table**: the table in the lead skill that lists, per kind, what a
  member does and may not do.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A lead can replace a wrong check with one command and no
  member restart; the next verify runs the new check, in 100% of runs of
  the acceptance test.
- **SC-002**: Replaying deckh's F14 sequence on the fakes (removal on the
  implementer's branch, verify fails) ends with a passing verify and a
  `close --role` without `--unverified`, after one `amend` and one verify.
- **SC-003**: Every git command banned by any seat's rules appears in that
  kind's row of the role table: 0 misses, checked by the test of FR-016.
- **SC-004**: Each of the three fixes has at least one test that fails
  when the fix is reverted.
- **SC-005**: The lead guidance for all three lead harnesses names
  `teamwire amend`.

## Assumptions

- The note of FR-006 states the old value too (confirmed by the answer on
  `--owns`), so the record of the change
  stands on its own: the per-member verify log keeps only the newest run,
  so an earlier verify record does not show its check.
- `--check` replaces the whole check command. Removing a check entirely is
  out of scope: `amend` takes a non-empty command.
- `amend` is a lead command. Like `launch`, `verify` and `close`, it does
  not check who runs it; no seat rule names it, so members are not told of
  it.
- Only F13, F14 and F15 are in scope. The other findings of the file are
  handled elsewhere or later.
- No new dependency and no change to `Cargo.lock`.
