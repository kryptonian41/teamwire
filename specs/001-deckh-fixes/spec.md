# Feature Specification: Fixes from team deckh's live run

**Feature Branch**: `dfix/spec`

**Created**: 2026-11-10

**Status**: Draft

**Input**: User description: "Fix the teamwire problems found in team deckh's live run on 2026-11-10, as listed in the findings file, with each fix proven by a test that fails without it and the lead guidance changed in the same change. US1 (P1): a result or message that could not be delivered to a blocked lead pane is delivered or plainly reported once the pane is free; the lead can record a message as read without having it typed again; an answer with `--re` settles its question's delivery state; the watch never reports the team complete while a member's session is busy; a message that wakes a finished member either waits for the lead's reprompt or handoff, or starts a generation the watch tracks (F1, F2, F3). US2 (P2): a teamwire command reads a spec-kit `tasks.md`, prints which phases and stories share files, lists the tasks that change no file or touch files no other task touches, and drafts each member's `owns` from the task paths plus the repository files that mention those paths, CI workflow path filters and tests that pin exact lists included (F4, F5). US3 (P3): the speckit preset tells the lead to read `checklists/requirements.md` before launching the implementer and to waive or fix each unticked item (F6); the closer preset has a slot for the gate this machine may run and the gates left to CI, and the closer reports both (F8); `teamwire status --role` shows how long the member's current tool call has run, on Claude Code, Codex and pi (F10); `close --dry-run` says \"symlink\" for a symlink and names its target (F11). Out of scope: F7 (already fixed in v0.24.0 by commit b485037) and F9 (an operator member kind, a later feature)."

The source of every finding is
`/Users/apoorvagarwal/Projects/teamwire-agent-local/deckh-findings-2026-11-10.md`.
Finding numbers F1 to F11 below refer to that file.

## User Scenarios & Testing *(mandatory)*

The users here are two people. The **lead** is the agent session that runs a
team of member agents. The **operator** is the person who watches the lead
and answers its questions.

### User Story 1 - Results and questions reach the lead, and the watch stays on (Priority: P1)

A member's result or question can arrive while the lead's pane is blocked,
for example by an open question dialog. Today nothing retries once the
dialog closes, the message stays "undelivered" even after the lead has read
it from its file, and the only way to clear it types the old message into
the lead's pane again (F1). A member whose result was not delivered and
which is working again counts as stopped, so the watch prints
`TEAM-COMPLETE` and exits while that member works (F2). A message that
wakes a finished member starts work that teamwire does not track, and the
lead can then no longer hand that member off (F3).

After this story, the lead learns of every result and question once its
pane is free, can record "I read this" without a second copy, and keeps a
watch that tracks every member that is working.

**Why this priority**: These three faults cost the deckh lead about 90
minutes and left the team without a watch. They break the core loop of
teamwire: a member reports, the lead reads, the watch tracks.

**Independent Test**: Run a team on the test fakes. Block the lead pane
while a member sends a question and records a result. Free the pane. Check
that the lead is told about both, that the lead can mark the question read
without a second copy, and that the watch neither exits nor loses the
member while the member's session is busy.

**Acceptance Scenarios**:

1. **Given** the lead's pane is blocked by a dialog and a member records a
   result, **When** the pane becomes free while a watch runs, **Then** the
   result is delivered to the lead, or the watch prints one plain line that
   names the result and the file to read it from. [NEEDS CLARIFICATION:
   should the watch retry delivery by itself once the pane is free, or only
   report plainly that it will not retry and where the message is?]
2. **Given** a question to the lead shows as undelivered, **When** the lead
   records it as read with a teamwire command, **Then** `teamwire status`
   no longer lists it under deliveries that need attention, and nothing is
   typed into any pane.
3. **Given** a question to the lead shows as undelivered, **When** the lead
   sends an answer with `--re <that question's id>`, **Then** the question
   no longer shows as undelivered.
4. **Given** a member's result was not delivered and the member's session
   is busy again, **When** a watch starts, **Then** the watch counts the
   member as working and does not print `TEAM-COMPLETE`.
5. **Given** a member recorded `done` (including a `handoff:` result),
   **When** the lead sends that member an answer or a note, **Then** the
   member's next work is tracked by teamwire, and the lead can still hand
   off or reprompt the member. [NEEDS CLARIFICATION: should teamwire hold
   the message until the lead reprompts or hands off, or deliver it and
   count the woken turn as a new generation the watch tracks?]

---

### User Story 2 - The lead sees which tasks can run in parallel and which files each member needs (Priority: P2)

Before launch, the lead of a spec-kit team must decide how many implementers
to start and which files each one owns. Today the lead does both by hand.
In deckh no one computed which phases shared files, two tasks that could
have run in parallel were missed (F4), and three files the implementer had
to change were missing from its `owns` list (F5): an import-linter config,
a test that pins an exact list, and a CI workflow path filter.

After this story, one teamwire command reads a spec-kit `tasks.md` and
prints three things: which phases and which stories share files, the tasks
that change no file or touch only files no other task touches, and a draft
`owns` list per proposed member. The draft adds the repository files that
mention the task paths. The lead checks the draft; the draft is not a final
list.

**Why this priority**: The split and the `owns` list are set once, before
launch. A mistake costs a question and a lead turn per missing file, and
lost parallel work. The fix is a new command, so it can ship after US1.

**Independent Test**: Run the command on a fixture `tasks.md` and a fixture
repository. Check the overlap table, the list of independent tasks, and that
the `owns` draft includes a CI workflow that filters on a task path and a
test that pins a list naming a task path.

**Acceptance Scenarios**:

1. **Given** a `tasks.md` whose phases 3 and 4 both name one file, **When**
   the lead runs the command, **Then** the output lists phases 3 and 4 with
   that file under shared files.
2. **Given** a task line that names no file path, **When** the lead runs the
   command, **Then** the output lists that task as changing no file.
3. **Given** a task whose files no other task names, **When** the lead runs
   the command, **Then** the output lists that task as independent.
4. **Given** a CI workflow whose path filter matches a task's path, and a
   test file that names a task's path in a fixed list, **When** the lead
   runs the command, **Then** both files appear in the draft `owns` list,
   each with the reason it was added. [NEEDS CLARIFICATION: what counts as
   a file that "mentions" a task path: the exact path text only, or also a
   glob in a CI path filter that matches the path and the file's module or
   base name?]
5. **Given** a `tasks.md` file that does not exist or has no task lines,
   **When** the lead runs the command, **Then** the command exits with a
   non-zero status and names the problem.

---

### User Story 3 - Small fixes to guidance and status output (Priority: P3)

Four smaller problems each cost the deckh lead a turn or a worry:

- The speckit preset says `specify` ticks every item of its
  `checklists/requirements.md`. In deckh `specify` left three items
  unticked on purpose, so `implement` stopped and asked (F6).
- The closer preset requires a full gate on each head that ships. Some
  operators forbid the full suite on their machine, and the preset has no
  way to say which gate runs locally and which runs in CI (F8).
- `teamwire status` says `working` for a member that has run one shell
  command for 15 minutes. The operator thought the member was stuck (F10).
- `close --dry-run` lists symlinked folders such as `.venv` as ignored files
  that removal deletes. The lead had to check that the main checkout's
  folders were still there (F11).

**Why this priority**: Each costs one turn or one check. None loses work.

**Independent Test**: Each fix is tested on its own: the preset texts by the
tests that check preset content, the status line on the fakes for each of
the three harnesses, and the dry run on a worktree fixture with a symlink.

**Acceptance Scenarios**:

1. **Given** the speckit preset, **When** the lead reads the step before the
   implementer launches, **Then** the preset says that `specify` can leave
   items in `checklists/requirements.md` unticked, and tells the lead to
   read the checklist and either waive each unticked item in the brief or
   have the spec member fix it.
2. **Given** the closer preset, **When** the lead fills it, **Then** the
   preset has one slot for the gate this machine may run and one for the
   gates left to CI, and the closer's report states which gate it ran and
   which it left to CI.
3. **Given** a member on Claude Code, Codex or pi whose current tool call
   started 15 minutes ago, **When** the operator runs
   `teamwire status <team> --role <role>`, **Then** the output names the
   tool and shows that it has run for about 15 minutes.
4. **Given** a member worktree where an ignored path is a symlink, **When**
   the lead runs `teamwire close <team> --role <role> --dry-run`, **Then**
   the output marks that path as a symlink and names its target, and lists
   it apart from files that removal deletes.

---

### Edge Cases

- The lead's pane stays blocked for longer than the watch runs. The message
  must still be reported when the next watch starts.
- The lead records a message as read that was already delivered. The command
  succeeds and changes nothing.
- The lead records as read a message id that does not exist, or a message
  sent to another member. The command refuses and names the reason.
- An answer with `--re` names a message that is not a question to the
  sender. The delivery state of that message does not change.
- A member's session is busy but the host cannot report whether it is busy.
  The watch keeps the member as not complete and says it could not tell.
- A member's harness does not report when the current tool call started.
  `teamwire status --role` says the duration is unknown, not zero.
- A task line names a path that does not exist in the repository yet (a new
  file). The path still counts for overlap and still goes in the `owns`
  draft.
- A symlink's target is outside the repository, or the link is broken. The
  dry run still names the target and marks a broken link.

## Requirements *(mandatory)*

### Functional Requirements

**US1: delivery and member state (F1, F2, F3)**

- **FR-001**: When a result or a message to the lead could not be delivered
  because the lead's pane was blocked, teamwire MUST either deliver it once
  the pane is free or print one line that names the message and the file to
  read it from (see the clarification in US1 scenario 1).
- **FR-002**: The lead MUST be able to record a message as read with a
  teamwire command that types nothing into any pane.
- **FR-003**: A message the lead recorded as read MUST no longer appear in
  `teamwire status` under deliveries that need attention.
- **FR-004**: An answer sent with `--re <id>` MUST settle the delivery state
  of question `<id>`, so the question no longer shows as undelivered.
- **FR-005**: The watch MUST NOT print `TEAM-COMPLETE` while any member's
  session is busy, whatever that member's recorded delivery state.
- **FR-006**: A member whose result was not delivered and whose session is
  busy again MUST count as working in the watch and in `teamwire status`.
- **FR-007**: A message that reaches a member whose last result is `done`
  MUST NOT start work that the watch does not track. The lead MUST still be
  able to hand off or reprompt that member afterwards.

**US2: the `tasks.md` reader (F4, F5)**

- **FR-008**: teamwire MUST provide a command that reads a spec-kit
  `tasks.md` and the repository it belongs to, and writes nothing.
- **FR-009**: The command MUST print, for each pair of phases and each pair
  of user stories, the files both name.
- **FR-010**: The command MUST list the tasks that name no file, and the
  tasks whose files no other task names.
- **FR-011**: The command MUST print a draft `owns` list for each proposed
  member, built from the task paths plus the repository files that mention
  those paths, including CI workflow path filters and tests that pin exact
  lists, each added file with the reason it was added.
- **FR-012**: The command's output MUST label the `owns` list as a draft for
  the lead to check.
- **FR-013**: The decision to add this command MUST be recorded in a new
  ADR that supersedes ADR 0042's decision 1 in part.

**US3: small fixes (F6, F8, F10, F11)**

- **FR-014**: The speckit preset MUST say that `specify` can leave items in
  `checklists/requirements.md` unticked, and MUST tell the lead to read the
  checklist before launching the implementer and to waive or fix each
  unticked item.
- **FR-015**: The closer preset MUST have a slot for the gate this machine
  may run and a slot for the gates left to CI. The closer's report MUST
  state both.
- **FR-016**: `teamwire status --role <role>` MUST show the name of the
  member's current tool call and how long it has run, for members on Claude
  Code, Codex and pi.
- **FR-017**: `teamwire close --dry-run` MUST mark each symlink among the
  paths that removal touches, name its target, and keep it apart from the
  files that removal deletes.

**All stories**

- **FR-018**: Every fix MUST come with a test that fails without the fix.
- **FR-019**: The lead skill, preset, reference or doc text that describes a
  changed behaviour MUST change in the same change as the code.
- **FR-020**: Each fix MUST name its root cause with file and line evidence
  on `develop` before code changes.

### Key Entities

- **Message**: A question, answer, finding, notice or note between team
  members and the lead. Has an id, a sender, a recipient, an optional `re`
  id, and a delivery state.
- **Delivery state**: Whether a message or result reached its recipient's
  pane: delivered, undelivered with a reason (such as a blocked pane), or
  read by the recipient from its file.
- **Result**: A member's completion record for one generation, with a
  status and a summary.
- **Generation**: One prompted run of a member that the watch tracks, from
  launch or reprompt to the member's `done`.
- **Task line**: One line of a spec-kit `tasks.md`, with a task id, an
  optional story label, a phase, and the file paths it names.
- **Owns draft**: A proposed list of files a member may change, with the
  reason each file is on it.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: In a replay of the deckh sequence on the test fakes, the lead
  is told about every blocked result and question within one watch cycle of
  the pane becoming free, against about 90 minutes in the live run.
- **SC-002**: The lead clears an undelivered message it has read in one
  command, with zero messages typed again into its pane.
- **SC-003**: In that replay, the watch prints `TEAM-COMPLETE` zero times
  while a member is busy, against three times in the live run.
- **SC-004**: The lead needs zero dummy reprompts to keep a woken member
  tracked.
- **SC-005**: Run on deckh's `tasks.md` and the knit-3.0 repository at that
  run's base, the command's `owns` draft includes all three files the live
  run missed, and its overlap table matches the four phase pairs measured
  from `impl`'s commits.
- **SC-006**: Every fix in scope has a test that fails when the fix is
  reverted.
- **SC-007**: An operator can tell from one `teamwire status --role` call
  how long a member's current tool call has run, on each of the three
  harnesses.

## Assumptions

- F7 and F9 are out of scope. F7 is fixed in v0.24.0 by commit `b485037`.
  F9 needs a new member kind and is a later feature.
- deckh's members ran build `74b1ca7`, not v0.24.0 (`eda2e0b`). Each root
  cause must hold on `develop`; a finding already fixed there is reported,
  not fixed again.
- The `tasks.md` reader reads the task line format that spec-kit's `tasks`
  command writes: `- [ ] T001 [P] [US1] Description with file paths`.
- The command reads `tasks.md` only. Reading a free-form brief is out of
  scope.
- The `owns` draft is a starting point for the lead, not a final list. The
  lead still runs the planning check in the skill.
- The command proposes one member per user story when stories share no
  files, and one member for stories that share files. The lead decides the
  split.
- The fixes reuse the existing delivery, message, report, watch and close
  code, and the existing presets. No new crate unless the layering rules in
  `docs/conventions.md` section 5 call for one.
- Tests use the fakes in `tests/fakes/`. No test types into a real pane or
  starts a real agent session.
