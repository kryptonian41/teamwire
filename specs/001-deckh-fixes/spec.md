# Feature Specification: Fixes from team deckh's live run

**Feature Branch**: `dfix/spec`

**Created**: 2026-11-10

**Status**: Draft

**Input**: User description: "Fix the teamwire problems found in team deckh's live run on 2026-11-10, as listed in the findings file, with each fix proven by a test that fails without it and the lead guidance changed in the same change. US1 (P1): a result or message that could not be delivered to a blocked lead pane is delivered or plainly reported once the pane is free; the lead can record a message as read without having it typed again; an answer with `--re` settles its question's delivery state; the watch never reports the team complete while a member's session is busy; a message that wakes a finished member either waits for the lead's reprompt or handoff, or starts a generation the watch tracks (F1, F2, F3). US2 (P2): a teamwire command reads a spec-kit `tasks.md`, prints which phases and stories share files, lists the tasks that change no file or touch files no other task touches, and drafts each member's `owns` from the task paths plus the repository files that mention those paths, CI workflow path filters and tests that pin exact lists included (F4, F5). US3 (P3): the speckit preset tells the lead to read `checklists/requirements.md` before launching the implementer and to waive or fix each unticked item (F6); the closer preset has a slot for the gate this machine may run and the gates left to CI, and the closer reports both (F8); `teamwire status --role` shows how long the member's current tool call has run, on Claude Code, Codex and pi (F10); `close --dry-run` says \"symlink\" for a symlink and names its target (F11). Out of scope: F7 (already fixed in v0.24.0 by commit b485037) and F9 (an operator member kind, a later feature)."

The source of every finding is
`/Users/apoorvagarwal/Projects/teamwire-agent-local/deckh-findings-2026-11-10.md`.
Finding numbers F1 to F12 below refer to that file. F12 was added to the
findings file after the first draft, and the user put it in scope as US4
(P4) on 2026-11-10.

## Clarifications

### Session 2026-11-10

- Q: When a blocked lead pane frees up, does the watch retry delivery of an undelivered result or message? → A: No process ever types an old message again (ADR 0003 point 7 and INV-8 stay). Instead the lead's own hooks list each undelivered result and message to the lead, with its file path, at the lead's next tool call and turn end, until the lead marks it read; the hooks add text to the lead's turn and type nothing; this ships for Claude Code, Codex and pi. `teamwire received` accepts message ids besides roles (and the lead's MCP tools get the same where `received` is reachable). An `answer` with `--re <id>` settles that question's delivery state. The watch's `UNDELIVERED` and `QUESTION-UNDELIVERED` lines name the file to read.
- Q: A message reaches a member whose last result is `done`. Does teamwire hold it, or deliver it and track the woken turn? → A: By default teamwire delivers it, and the turn it starts counts as a new generation that the watch tracks and that ends with `done` like any generation. Exception: when the member's newest result has a `handoff:` summary, teamwire holds the message (stored, not typed), and `teamwire handoff` passes the held messages to the fresh session.
- Q: What counts as a repository file that "mentions" a task path in the `owns` draft? → A: A file that holds the exact path text, a CI workflow `paths:` glob that matches the path, the path's dotted module name (`harness.tools` for `harness/tools.py`), or the path's full file name with its extension (`tools.py`). A bare stem such as `tools` never counts. The output is labelled a draft for the lead to check.
- Q: May the fixes supersede accepted ADRs? → A: Yes, narrowly, in one new ADR for US1 that names each superseded point. ADR 0003 point 7 and INV-8 stay. WAT-6 and ADR 0007 point 6 are superseded so that a member whose result is `UNDELIVERED` or `UNCONFIRMED` and whose session is busy counts as working, not settled; a member whose session is dead or idle is still settled. The new-generation rule for a woken member supersedes any ADR point or requirement that says only a `reprompt` starts a generation.
- Q: When the watch cannot tell whether a member's session is busy, idle or dead, and that member's result is `UNDELIVERED` or `UNCONFIRMED`, does the watch count the member as settled or as working? → A: Settled, as today; the watch's line says it could not see the session.
- Q: Which recorded results make a waking message start a new generation: only a `done` result, or any result (`done`, `blocked`, `failed`)? → A: Any recorded result in the current generation (`done`, `blocked`, `failed`); the `handoff:` hold is the only exception.
- Q: The lead reprompts, instead of handing off, a member whose newest result is `handoff:` and that has held messages. What happens to the held messages? → A: The reprompt delivers the held messages right after the reprompt text, oldest first, and the hold ends. (Fix round 1, 2026-11-13: the reprompt names the held messages by id and points to `teamwire inbox`; it does not type their text.)
- Q: Does the `tasks.md` reader draft one `owns` list per user story, or per member of a split it proposes? → A: One `owns` draft per user story, plus one for the setup and polish phases, and a line naming which stories share files; the lead merges stories into members.
- Q: May US4 supersede ADR 0009 point 2 ("A waiver does not carry over to a new HEAD"), and what ends a waiver instead? → A: Yes, narrowly, in a new ADR. A waiver for a flag about one file holds while that file's content is the same as at the HEAD where the lead waived it. Any change to that file's content ends the waiver, also one that a rebase or merge brings in. A flag that names no file, such as `no-commits`, still ends at the next commit, as today.

### Session 2026-11-13

These answers came from the plan review. The first four are the user's. The
rest are the lead's.

- Q: Which event records a generation that an `answer` or `note` started? → A: A new event type, `member.woken`, not `member.reprompted`. The metrics do not count it as a reprompt.
- Q: Where do held messages show? → A: `teamwire status` lists each on its own line, such as `held for handoff: m004 -> impl`, and not under deliveries that need attention. The dashboard does not show them as needing a look.
- Q: Does a member still resend a message to the lead that was not delivered? → A: No. The lead's hooks list every undelivered message, so a resend would type a second copy. The member's seat text no longer tells it to resend to the lead.
- Q: May an existing test change its expected value? → A: Yes, when it pins a rule that this feature's ADRs or a user answer in this spec changes. No assert may be removed, and the implementer names each changed test.
- Q: May `member.woken` move the store to a new schema version, and how does the upgrade go out? → A: Yes, schema version 11. The upgrade only goes forward: a store the new build has opened cannot be opened by an older build, as ADR 0038 says of version 10. The user installs the new build only after team `deckh` closes.
- Q (lead): What does `send` print when the lead's copy of a user's message was not delivered? → A: It still starts `lead copy not delivered: <id>`, then says the lead's hook lists the message. It names no `resend`, for the same reason as the seat text.
- Q (lead): What does `teamwire status` say for a member with an undelivered result whose session waits at a dialog mid-turn? → A: `working (at a dialog)`, as the watch counts it as working.
- Q (lead): How does `received` tell a message id from a role? → A: An argument is a message id only when it matches `m` and digits and no role of the team has that name. When a role has that name too, the command refuses and names both. A row already delivered or already read changes nothing, and the command says so.
- Q (lead): Which file does the lead's hook name for an undelivered result? → A: The same file as the watch's `Result:` path: the result file, or the member's report when the result file is missing.
- Q (lead): Which exit code does `speckit draft` use for a missing `tasks.md`? → A: 1, with a named error. Exit 3 stays for usage errors.
- Q (lead): How does `speckit draft` read paths? → A: It strips a `:N` or `:N-M` line suffix. An indented line under a task belongs to that task, until the next task line, heading or blank line. A token that holds `*` matches the repository's tracked files as a glob.
- Q (lead): Who measures SC-001, SC-003 and SC-005? → A: The closer, read-only. The closer replays deckh's sequence on the fakes and runs `speckit draft` on deckh's `tasks.md` in knit-3.0.

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
    messages are named right
    after the reprompt text, oldest first, with their ids and a pointer to `teamwire inbox`, and the hold ends.

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
   **When** the lead runs the command, **Then** the command exits with
   status 1 and names the problem. Exit status 3 stays for usage errors.

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

### User Story 4 - A waiver for a file holds until that file changes (Priority: P4)

In deckh the `spec` member changed `.specify/memory/constitution.md` with
the lead's approval, and the lead waived the two flags verify raised for
that file. Each later commit of `spec` touched only `specs/`. Even so,
verify raised both flags again, unwaived, at each new HEAD. The lead waived
the same two flags with the same reason at four HEADs (F12).

**Why this priority**: The cost is lead turns and a noisy verify, not lost
work. The fix is small and touches one crate.

**Independent Test**: In a test repository, a member changes a check config
file outside its `owns`. The lead waives both flags. The member then commits
a file inside its `owns`. Verify says `PASS` with both flags waived. A
further commit that changes the waived file makes verify `FLAGGED` again.

**Acceptance Scenarios**:

1. **Given** a member whose commit changed one file outside its `owns`, and
   the lead's waivers for that file's flags at that HEAD, **When** the
   member makes a commit that leaves that file alone and the lead runs
   `teamwire verify <team> --role <role>`, **Then** the file's flags show as
   waived and the verdict is `PASS`.
2. **Given** the same waivers, **When** the member makes a commit that
   changes the waived file, **Then** the file's flags show as not waived and
   the verdict is `FLAGGED`.
3. **Given** the same waivers, **When** the member merges a branch that
   changes the waived file, **Then** the file's flags show as not waived.
4. **Given** a waiver for a flag that names no file, such as `no-owns`,
   **When** the member makes any new commit, **Then** that flag shows as not
   waived, as today.

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
  result that has held messages. The reprompt text is followed by the held
  messages' ids, oldest first, and a pointer to `teamwire inbox`. The hold
  ends.
- A woken member's new generation ends with no `done`, because its session
  dies. The watch reports it the way it reports any generation whose
  session died.
- The lead records a message as read that was already delivered or already
  read. The command changes nothing and says so.
- A team has a role whose name looks like a message id, such as `m012`. The
  lead runs `teamwire received <team> m012`. The command refuses and names
  both the role and the message.
- A member with a `handoff:` result has held messages. `teamwire status`
  lists each held message on its own line, not under deliveries that need
  attention.
- A task line in `tasks.md` names a path with a line suffix, such as
  `send.rs:297-321`, or names its paths on indented lines under the task.
  The command counts `send.rs` and reads the indented paths as the task's.
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
- A member changes a waived file and later changes it back, so its content
  at HEAD is the same as at the waived HEAD. The waiver holds, because
  verify compares the file's content at the two HEADs, not its history.
- The repository no longer holds the commit where the lead waived a flag,
  for example after a rebase and a garbage collection. The waiver ends.
- A waived flag's file was deleted at the waived HEAD and is still absent
  at HEAD now. The content counts as the same, so the waiver holds.
- An `uncommitted:<path>` flag is about a change that no commit holds. Its
  waiver ends at the next commit, as today.

## Requirements *(mandatory)*

### Functional Requirements

**US1: delivery and member state (F1, F2, F3)**

- **FR-001**: The lead's hooks MUST list each result and each message to
  the lead that was never typed into the lead's pane, with its file path, at
  the lead's next tool call and at the lead's turn end, and MUST keep
  listing it until the lead marks it read. The hooks add text to the lead's
  turn and MUST type nothing into the pane. This MUST ship for every lead
  harness: Claude Code, Codex and pi. For a result, the hooks MUST name the
  same file as the watch's `Result:` path. The member's seat text MUST NOT
  tell a member to resend a message to the lead, because a resend would type
  a second copy. For the same reason, the line `send` prints when the lead's
  copy of a message was not delivered MUST NOT name `resend`. It MUST say
  that the lead's hook lists the copy, or, for a copy that may have been
  typed, that `teamwire status` lists it.
- **FR-002**: teamwire MUST NOT type a message after the command that
  sent it has exited. ADR 0003 point 7 and INV-8 stay in force.
- **FR-003**: `teamwire received` MUST accept message ids (such as `m008`)
  besides roles, and record each named message as read by the lead without
  typing it again. An argument is a message id only when it is `m` followed
  by digits and no role of the team has that name; when a role has that name
  too, the command MUST refuse and name both. A message already delivered or
  already read MUST stay as it is, and the command MUST say so. The lead's
  MCP tools MUST accept the same where they reach `received`.
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
  member works. A session that waits at a dialog mid-turn counts as busy, and
  `teamwire status` MUST show such a member as `working (at a dialog)`. A
  member whose session is dead or idle MUST still count as
  settled. A member whose session the watch cannot see MUST count as
  settled, and the watch's line MUST say it could not see the session.
- **FR-008**: A message that wakes a member whose newest result in its
  current generation is `done`, `blocked` or `failed`, without a `handoff:`
  summary, MUST start a new generation that the watch
  tracks and that ends when the member records `done`. teamwire MUST record
  that generation with its own event type, `member.woken`. The metrics MUST
  NOT count it as a reprompt.
- **FR-009**: A message to a member whose newest result has a `handoff:`
  summary MUST be stored and not typed. `teamwire handoff` MUST pass the
  held messages to the fresh session. A `reprompt` to that member MUST name
  the held messages by id right after the reprompt text, oldest first,
  MUST point to `teamwire inbox` to read them, and MUST end the hold. `teamwire status` MUST list each held message on its own line,
  not under deliveries that need attention, and the dashboard MUST NOT show
  it as needing a look.
- **FR-010**: One new ADR MUST record the US1 decisions and name each point
  it supersedes: WAT-6, ADR 0007 point 6, ADR 0024 point 2 (`received`
  takes roles only), ADR 0035 point 1 (the Claude lead tool hook's tool
  list), and any ADR point or requirement that says only a `reprompt` starts
  a generation.

**US2: the `tasks.md` reader (F4, F5)**

- **FR-011**: teamwire MUST provide a command that reads a spec-kit
  `tasks.md` and the repository it belongs to, and writes nothing. The
  command MUST read a path with a `:N` or `:N-M` line suffix as the path
  without the suffix. An indented line under a task line MUST count as part
  of that task, until the next task line, heading or blank line. A path
  that holds `*` MUST match the repository's tracked files as a glob. A
  missing or unreadable `tasks.md`, or one with no task lines, MUST exit 1
  with a named error.
- **FR-012**: The command MUST print, for each pair of phases and each pair
  of user stories, the files both name.
- **FR-013**: The command MUST list the tasks that name no file, and the
  tasks whose files no other task names. A token of a task line is a
  repository path only when it holds `/`, does not start with `/`, is not
  part of a markdown link's `](...)` syntax, and either names a tracked
  file or directory or ends in a file extension. So a route such as
  `"/agui/deck_editor"`, an event name such as `knit.progress`, and an
  absolute path are not paths. A token with no `/` is a path only when it
  names a tracked file at the repository root exactly, so `README.md`,
  `.gitignore` and `Cargo.toml` are paths, and `knit.progress` and `lib.rs`
  are not. A directory token, such as `apps/ai`, counts
  in the overlap table as a prefix of the files under it, and adds no file
  to the mention search.
- **FR-014**: The command MUST print one draft `owns` list per user story,
  plus one for the setup and polish phases, and a line naming which stories
  share files. Each draft is built from the task paths plus the repository files that mention
  those paths, each added file with the reason it was added. A file
  mentions a task path when it holds the exact path text, a CI workflow
  `paths:` glob that matches the path, the path's dotted module name (such
  as `harness.tools` for `harness/tools.py`), or the path's full file name
  with its extension (such as `tools.py`). The exact-path rule MUST also
  match right after any `/` or `./` prefix, so `./apps/ai/harness/tools.py`
  and `$ROOT/apps/ai/harness/tools.py` mention `apps/ai/harness/tools.py`.
  After a `/`, the file-name rule MUST count only when the slash-joined text
  that ends at the name is a suffix of the task path. So `harness/tools.py`
  and `ai/harness/tools.py` mention `apps/ai/harness/tools.py`, and
  `other/tools.py` does not. The dotted
  module rule keeps its token boundary. A bare stem such as `tools` MUST
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
  state both. The closer's seat text MUST say the same: run the local gate
  the brief names, and report the gates left to CI.
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

**US4: waivers (F12)**

- **FR-024**: A waiver for a flag that names one file MUST count at a later
  HEAD of the same member while that file's content at the later HEAD is the
  same as at the HEAD where the lead waived it. A file absent at both HEADs
  counts as the same.
- **FR-025**: Any change to the waived file's content between the waived
  HEAD and HEAD now MUST end the waiver, also a change that a rebase or a
  merge brings in. A waiver whose HEAD the repository no longer holds MUST
  end.
- **FR-026**: A waiver for a flag that names no file, such as `no-commits`,
  `no-owns` or `report-no-line:<sha>`, and a waiver for
  `uncommitted:<path>`, MUST count only at the HEAD where it was recorded,
  as today.
- **FR-027**: A new ADR MUST record the US4 rule and supersede ADR 0009
  point 2 only for the flags that name one file.

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
  reprompt or a waking message to the member's `done`. A generation that a
  waking message starts is recorded as `member.woken`, apart from a
  reprompt.
- **Held message**: A message to a member whose newest result has a
  `handoff:` summary. It is stored, not typed, until a handoff or a
  reprompt passes it on. `teamwire status` lists it on its own line.
- **Task line**: One line of a spec-kit `tasks.md`, with a task id, an
  optional story label, a phase, and the file paths it names.
- **Owns draft**: A proposed list of files a member may change, with the
  reason each file is on it.
- **Waiver**: The lead's acceptance of one verify flag of one member, with a
  reason and the HEAD where the lead recorded it. A flag that names one file
  carries that file's path.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: In a replay of the deckh sequence on the test fakes, the lead
  is told about every blocked result and question, with its file, at its
  next tool call, against about 90 minutes in the live run. The closer
  checks this read-only, by replaying deckh's sequence on the fakes.
- **SC-002**: The lead clears an undelivered message it has read in one
  command, with zero messages typed again into its pane.
- **SC-003**: In that replay, the watch prints `TEAM-COMPLETE` zero times
  while a member is busy, against two times in the live run (rca F2.2:
  the first `TEAM-COMPLETE`, at 14:02:06, came before the answers woke
  `impl`). The closer
  checks this in the same replay.
- **SC-004**: The lead needs zero dummy reprompts to keep a woken member
  tracked, and a member with a `handoff:` result does zero work in its old
  session after a message reaches it.
- **SC-005**: Run on deckh's `tasks.md` and the knit-3.0 repository at that
  run's base, the command's `owns` draft includes all three files the live
  run missed, and its overlap table holds all four phase pairs of the
  findings file's F4 table, with the shared files that deckh's `tasks.md`
  names. A `tasks.md` reader sees only the files the task lines name, so
  files that only `impl`'s commits touched are not expected. The closer checks this read-only, by running the
  command on deckh's `tasks.md` in knit-3.0.
- **SC-006**: Every fix in scope has a test that fails when the fix is
  reverted.
- **SC-007**: An operator can tell from one `teamwire status --role` call
  how long a member's current tool call has run, on each of the three
  harnesses.
- **SC-008**: In a replay of deckh's waiver sequence on a test repository,
  the lead waives each file flag once, and verify says `PASS` at every later
  commit that leaves the file alone, against the same two waivers recorded
  at four HEADs in the live run.

## Assumptions

- F7 and F9 are out of scope. F7 is fixed in v0.24.0 by commit `b485037`.
  F9 needs a new member kind and is a later feature.
- deckh's members ran build `74b1ca7`, not v0.24.0 (`eda2e0b`). Each root
  cause must hold on `develop`; a finding already fixed there is reported,
  not fixed again.
- The `tasks.md` reader reads the task line format that spec-kit's `tasks`
  command writes: `- [ ] T001 [P] [US1] Description with file paths`, plus
  indented continuation lines and `:N-M` line suffixes, as this feature's
  own `tasks.md` has.
- The command reads `tasks.md` only. Reading a free-form brief is out of
  scope.
- The `owns` draft is a starting point for the lead, not a final list. The
  lead still runs the planning check in the skill.
- The command proposes no split. It drafts per user story, and the lead
  merges stories into members.
- The fixes reuse the existing delivery, message, report, watch and close
  code, and the existing presets. No new crate unless the layering rules in
  `docs/conventions.md` section 5 call for one.
- US1 adds one event type, `member.woken`. The store's list of event types
  is closed, so the store moves to a new schema version. A store that the
  new build has opened cannot be opened by an older build, as with every
  earlier schema version. The upgrade only goes forward, so the user
  installs the new build only after team `deckh` closes.
- US4 stores no new data. Each waiver already records the HEAD where the
  lead waived it, and that HEAD is all the content comparison needs.
- Tests use the fakes in `tests/fakes/`. No test types into a real pane or
  starts a real agent session.
