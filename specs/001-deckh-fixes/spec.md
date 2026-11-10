# Feature Specification: Fixes from team deckh's live run

**Feature Branch**: `dfix/spec`

**Created**: 2026-11-10

**Status**: Draft

**Input**: User description: "Fix the teamwire problems found in team deckh's live run on 2026-11-10, as listed in the findings file, with each fix proven by a test that fails without it and the lead guidance changed in the same change. US1 (P1): a result or message that could not be delivered to a blocked lead pane is delivered or plainly reported once the pane is free; the lead can record a message as read without having it typed again; an answer with `--re` settles its question's delivery state; the watch never reports the team complete while a member's session is busy; a message that wakes a finished member either waits for the lead's reprompt or handoff, or starts a generation the watch tracks (F1, F2, F3). US2 (P2): a teamwire command reads a spec-kit `tasks.md`, prints which phases and stories share files, lists the tasks that change no file or touch files no other task touches, and drafts each member's `owns` from the task paths plus the repository files that mention those paths, CI workflow path filters and tests that pin exact lists included (F4, F5). US3 (P3): the speckit preset tells the lead to read `checklists/requirements.md` before launching the implementer and to waive or fix each unticked item (F6); the closer preset has a slot for the gate this machine may run and the gates left to CI, and the closer reports both (F8); `teamwire status --role` shows how long the member's current tool call has run, on Claude Code, Codex and pi (F10); `close --dry-run` says \"symlink\" for a symlink and names its target (F11). Out of scope: F7 (already fixed in v0.24.0 by commit b485037) and F9 (an operator member kind, a later feature)."

The source of every finding is
`/Users/apoorvagarwal/Projects/teamwire-agent-local/deckh-findings-2026-11-10.md`.
Finding numbers F1 to F11 below refer to that file.

## Clarifications

### Session 2026-11-10

- Q: When a blocked lead pane frees up, does the watch retry delivery of an undelivered result or message? → A: No process ever types an old message again (ADR 0003 point 7 and INV-8 stay). Instead the lead's own hooks list each undelivered result and message to the lead, with its file path, at the lead's next tool call and turn end, until the lead marks it read; the hooks add text to the lead's turn and type nothing; this ships for Claude Code, Codex and pi. `teamwire received` accepts message ids besides roles (and the lead's MCP tools get the same where `received` is reachable). An `answer` with `--re <id>` settles that question's delivery state. The watch's `UNDELIVERED` and `QUESTION-UNDELIVERED` lines name the file to read.
- Q: A message reaches a member whose last result is `done`. Does teamwire hold it, or deliver it and track the woken turn? → A: By default teamwire delivers it, and the turn it starts counts as a new generation that the watch tracks and that ends with `done` like any generation. Exception: when the member's newest result has a `handoff:` summary, teamwire holds the message (stored, not typed), and `teamwire handoff` passes the held messages to the fresh session.
- Q: What counts as a repository file that "mentions" a task path in the `owns` draft? → A: A file that holds the exact path text, a CI workflow `paths:` glob that matches the path, the path's dotted module name (`harness.tools` for `harness/tools.py`), or the path's full file name with its extension (`tools.py`). A bare stem such as `tools` never counts. The output is labelled a draft for the lead to check.
- Q: May the fixes supersede accepted ADRs? → A: Yes, narrowly, in one new ADR for US1 that names each superseded point. ADR 0003 point 7 and INV-8 stay. WAT-6 and ADR 0007 point 6 are superseded so that a member whose result is `UNDELIVERED` or `UNCONFIRMED` and whose session is busy counts as working, not settled; a member whose session is dead or idle is still settled. The new-generation rule for a woken member supersedes any ADR point or requirement that says only a `reprompt` starts a generation.
- Q: When the watch cannot tell whether a member's session is busy, idle or dead, and that member's result is `UNDELIVERED` or `UNCONFIRMED`, does the watch count the member as settled or as working? → A: Settled, as today; the watch's line says it could not see the session.
- Q: Which recorded results make a waking message start a new generation: only a `done` result, or any result (`done`, `blocked`, `failed`)? → A: Any recorded result in the current generation (`done`, `blocked`, `failed`); the `handoff:` hold is the only exception.
- Q: The lead reprompts, instead of handing off, a member whose newest result is `handoff:` and that has held messages. What happens to the held messages? → A: The reprompt delivers the held messages right after the reprompt text, oldest first, and the hold ends.
- Q: Does the `tasks.md` reader draft one `owns` list per user story, or per member of a split it proposes? → A: One `owns` draft per user story, plus one for the setup and polish phases, and a line naming which stories share files; the lead merges stories into members.

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

After this story, the lead's own hooks tell the lead about every result and
question that could not be typed, with the file to read, without typing
anything into the pane. The lead can record "I read this" without a second
copy. The watch tracks every member whose session is working, and a member
that asked to be handed off waits for its handoff.

**Why this priority**: These three faults cost the deckh lead about 90
minutes and left the team without a watch. They break the core loop of
teamwire: a member reports, the lead reads, the watch tracks.

**Independent Test**: Run a team on the test fakes. Block the lead pane
while a member sends a question and records a result. Run the lead's hook.
Check that the hook lists both with their files, that the lead can mark
the question read without a second copy, that the watch neither exits nor
loses the member while the member's session is busy, and that a message to
a member with a `handoff:` result is held for the handoff.

**Acceptance Scenarios**:

1. **Given** the lead's pane is blocked by a dialog and a member records a
   result and sends a question to the lead, **When** the lead makes its next
   tool call or ends its turn, **Then** the lead's hook adds a list to the
   lead's turn that names the result and the question, each with its file
   path, and nothing is typed into the lead's pane.
2. **Given** the hook has listed an undelivered message, **When** the lead
   makes further tool calls without marking it read, **Then** the hook keeps
   listing it at each tool call and turn end.
3. **Given** a question to the lead shows as undelivered, **When** the lead
   runs `teamwire received <team> <message id>`, **Then** `teamwire status`
   no longer lists it under deliveries that need attention, the hook stops
   listing it, and nothing is typed into any pane.
4. **Given** a question to the lead shows as undelivered, **When** the lead
   sends an answer with `--re <that question's id>`, **Then** the question
   no longer shows as undelivered and the hook stops listing it.
5. **Given** a result or a question to the lead could not be typed, **When**
   the watch prints its `UNDELIVERED` or `QUESTION-UNDELIVERED` line,
   **Then** the line names the file to read.
6. **Given** a member's result was not delivered (`UNDELIVERED` or
   `UNCONFIRMED`) and the member's session is busy again, **When** a watch
   starts, **Then** the watch counts the member as working and does not
   print `TEAM-COMPLETE`.
7. **Given** a member's result was not delivered and the member's session
   is dead or idle, **When** a watch starts, **Then** the watch counts the
   member as settled, so the team can complete. The same holds when the
  watch cannot see the session, and the watch's line says so.
8. **Given** a member recorded a result (`done`, `blocked` or `failed`)
   whose summary does not start with `handoff:`, **When** the lead or another member sends it a message that
   wakes it, **Then** the woken turn counts as a new generation that the
   watch tracks, and that generation ends when the member records `done`.
9. **Given** a member's newest result has a `handoff:` summary, **When** the
   lead or another member sends it a message, **Then** teamwire stores the
   message and types nothing into the member's pane, and
   `teamwire handoff <team> --role <role>` passes the held messages to the
   fresh session.
10. **Given** a member with a `handoff:` result has held messages, **When**
    the lead reprompts it instead of handing it off, **Then** the held
    messages are typed right after the reprompt text, oldest first, and the
    hold ends.

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
`owns` list per user story, plus one for the setup and polish phases. Each
draft adds the repository files that mention the task paths. The lead merges
stories into members. The lead checks the draft; the draft is not a final
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
   each with the reason it was added.
5. **Given** a task path `harness/tools.py`, a config file that names the
   module `harness.tools`, and a file that names only the stem `tools`,
   **When** the lead runs the command, **Then** the config file is in the
   draft `owns` list and the file that names only `tools` is not.
6. **Given** a `tasks.md` file that does not exist or has no task lines,
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

- The lead's pane stays blocked for longer than the watch runs. The lead's
  hook still lists the message at the lead's next tool call, with or
  without a watch.
- The lead's harness is Codex or pi. The hook that lists undelivered
  messages works there the same way as on Claude Code.
- A member with a `handoff:` result gets several messages before the lead
  hands it off. The handoff passes all of them, oldest first.
- The lead reprompts, instead of handing off, a member with a `handoff:`
  result that has held messages. The held messages are typed right after
  the reprompt text, oldest first, and the hold ends.
- A woken member's new generation ends with no `done`, because its session
  dies. The watch reports it the way it reports any generation whose
  session died.
- The lead records a message as read that was already delivered. The command
  succeeds and changes nothing.
- The lead records as read a message id that does not exist, or a message
  sent to another member. The command refuses and names the reason.
- An answer with `--re` names a message that is not a question to the
  sender. The delivery state of that message does not change.
- The host cannot report whether a member's session with an undelivered
  result is busy, idle or dead. The watch counts the member as settled, as
  today, and its line says it could not see the session.
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

- **FR-001**: The lead's hooks MUST list each result and each message to
  the lead that was never typed into the lead's pane, with its file path, at
  the lead's next tool call and at the lead's turn end, and MUST keep
  listing it until the lead marks it read. The hooks add text to the lead's
  turn and MUST type nothing into the pane. This MUST ship for every lead
  harness: Claude Code, Codex and pi.
- **FR-002**: teamwire MUST NOT type a message after the command that
  sent it has exited. ADR 0003 point 7 and INV-8 stay in force.
- **FR-003**: `teamwire received` MUST accept message ids (such as `m008`)
  besides roles, and record each named message as read by the lead without
  typing it again. The lead's MCP tools MUST accept the same where they
  reach `received`.
- **FR-004**: A message the lead recorded as read MUST no longer appear in
  `teamwire status` under deliveries that need attention, nor in the hook's
  list.
- **FR-005**: An answer sent with `--re <id>` MUST settle the delivery state
  of question `<id>` for the lead, so the question no longer shows as
  undelivered.
- **FR-006**: The watch's `UNDELIVERED` and `QUESTION-UNDELIVERED` lines
  MUST name the file to read.
- **FR-007**: A member whose result is `UNDELIVERED` or `UNCONFIRMED` and
  whose session is busy MUST count as working, not settled, in the watch and
  in `teamwire status`. The watch MUST NOT print `TEAM-COMPLETE` while such a
  member works. A member whose session is dead or idle MUST still count as
  settled. A member whose session the watch cannot see MUST count as
  settled, and the watch's line MUST say it could not see the session.
- **FR-008**: A message that wakes a member whose newest result in its
  current generation is `done`, `blocked` or `failed`, without a `handoff:`
  summary, MUST start a new generation that the watch
  tracks and that ends when the member records `done`.
- **FR-009**: A message to a member whose newest result has a `handoff:`
  summary MUST be stored and not typed. `teamwire handoff` MUST pass the
  held messages to the fresh session. A `reprompt` to that member MUST type
  the held messages right after the reprompt text, oldest first, and end
  the hold.
- **FR-010**: One new ADR MUST record the US1 decisions and name each point
  it supersedes: WAT-6, ADR 0007 point 6, and any ADR point or requirement
  that says only a `reprompt` starts a generation.

**US2: the `tasks.md` reader (F4, F5)**

- **FR-011**: teamwire MUST provide a command that reads a spec-kit
  `tasks.md` and the repository it belongs to, and writes nothing.
- **FR-012**: The command MUST print, for each pair of phases and each pair
  of user stories, the files both name.
- **FR-013**: The command MUST list the tasks that name no file, and the
  tasks whose files no other task names.
- **FR-014**: The command MUST print one draft `owns` list per user story,
  plus one for the setup and polish phases, and a line naming which stories
  share files. Each draft is built from the task paths plus the repository files that mention
  those paths, each added file with the reason it was added. A file
  mentions a task path when it holds the exact path text, a CI workflow
  `paths:` glob that matches the path, the path's dotted module name (such
  as `harness.tools` for `harness/tools.py`), or the path's full file name
  with its extension (such as `tools.py`). A bare stem such as `tools` MUST
  NOT count.
- **FR-015**: The command's output MUST label the `owns` list as a draft for
  the lead to check.
- **FR-016**: The decision to add this command MUST be recorded in a new
  ADR that supersedes ADR 0042's decision 1 in part.

**US3: small fixes (F6, F8, F10, F11)**

- **FR-017**: The speckit preset MUST say that `specify` can leave items in
  `checklists/requirements.md` unticked, and MUST tell the lead to read the
  checklist before launching the implementer and to waive or fix each
  unticked item.
- **FR-018**: The closer preset MUST have a slot for the gate this machine
  may run and a slot for the gates left to CI. The closer's report MUST
  state both.
- **FR-019**: `teamwire status --role <role>` MUST show the name of the
  member's current tool call and how long it has run, for members on Claude
  Code, Codex and pi.
- **FR-020**: `teamwire close --dry-run` MUST mark each symlink among the
  paths that removal touches, name its target, and keep it apart from the
  files that removal deletes.

**All stories**

- **FR-021**: Every fix MUST come with a test that fails without the fix.
- **FR-022**: The lead skill, preset, reference or doc text that describes a
  changed behaviour MUST change in the same change as the code.
- **FR-023**: Each fix MUST name its root cause with file and line evidence
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
- **Generation**: One run of a member that the watch tracks, from launch,
  reprompt or a waking message to the member's `done`.
- **Held message**: A message to a member whose newest result has a
  `handoff:` summary. It is stored, not typed, until a handoff or a
  reprompt passes it on.
- **Task line**: One line of a spec-kit `tasks.md`, with a task id, an
  optional story label, a phase, and the file paths it names.
- **Owns draft**: A proposed list of files a member may change, with the
  reason each file is on it.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: In a replay of the deckh sequence on the test fakes, the lead
  is told about every blocked result and question, with its file, at its
  next tool call, against about 90 minutes in the live run.
- **SC-002**: The lead clears an undelivered message it has read in one
  command, with zero messages typed again into its pane.
- **SC-003**: In that replay, the watch prints `TEAM-COMPLETE` zero times
  while a member is busy, against three times in the live run.
- **SC-004**: The lead needs zero dummy reprompts to keep a woken member
  tracked, and a member with a `handoff:` result does zero work in its old
  session after a message reaches it.
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
- The command proposes no split. It drafts per user story, and the lead
  merges stories into members.
- The fixes reuse the existing delivery, message, report, watch and close
  code, and the existing presets. No new crate unless the layering rules in
  `docs/conventions.md` section 5 call for one.
- Tests use the fakes in `tests/fakes/`. No test types into a real pane or
  starts a real agent session.
