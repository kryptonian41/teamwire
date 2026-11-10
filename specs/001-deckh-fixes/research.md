# Research: Fixes from team deckh's live run

All file:line references are on `develop` (`eda2e0b`, v0.24.0). The branch
`dfix/spec` changes nothing under `crates/`, `docs/` or `examples/`, so its
lines match `develop`. The causes of F1 to F3 come from rca's report,
`/Users/apoorvagarwal/.teamwire/teams/dfix/reports/rca.md`. The spec member
re-read each cited line in the code. rca's failing reproductions are in
`/Users/apoorvagarwal/.teamwire/scratch/dfix/rca/repo`, which is read only.
The implementer writes its own tests in its worktree.

Every test below must fail on `develop` with its assertion before the fix,
and pass after it. Each test name is a proposal. The implementer may rename
a test but must keep its assertion.

---

## US1: delivery and member state (F1, F2, F3)

### Shared cause (verified)

The watch and status take a member's state from its newest completion in
its current generation (`crates/teamwire-watch/src/snapshot.rs:103-105`,
`crates/teamwire-watch/src/status.rs:99-101`). Only a `reprompt` or a
handoff starts a generation (`crates/teamwire-msg/src/send.rs:311`; ADR
0027, ADR 0032). teamwire records neither of two events:

- a turn that a non-reprompt message starts in a finished session (F3,
  which also keeps F2's `undelivered` verdict in place);
- a message that the lead read from its file without having it typed (F1).

The decisions below record both. One new ADR, `docs/adr/0043-*.md`, holds
the decisions of US1 and names each superseded point:

- WAT-6 (`docs/requirements.md:177`);
- ADR 0007 point 6 (`docs/adr/0007-completion-reporting.md:91-93`);
- INV-14 (`requirements.md:57`) and REP-3 (`requirements.md:129`), "a
  reprompt starts a new generation", widened to a waking message;
- the Claude lead tool-hook matcher (decision F1-d below).

ADR 0003 point 7 and INV-8 stay in force. No process types a message after
the command that sent it has exited.

### F1. A blocked lead pane leaves results and questions undelivered

**Root cause (verified, rca F1.1):**

1. The sending command makes one delivery attempt. A blocked pane refuses
   it with `session_blocked` before any typing
   (`crates/teamwire-deliver/src/deliver.rs:111-135`). The row is written
   `undelivered` and the command exits 4
   (`crates/teamwire-msg/src/attempt.rs:255-300`).
2. Nothing types the row again, by design
   (`docs/adr/0003-delivery-and-integrity.md:109-114`). The Stop hook skips
   an `undelivered` completion
   (`crates/teamwire-report/src/turn_end.rs:153`).
3. The lead has no command that records "read from its file":
   - `received` takes roles only, and marks only completions
     (`crates/teamwire-report/src/received.rs:23-46`).
   - `ack` refuses the lead (`crates/teamwire-cli/src/cmd/ack.rs:30-34`).
   - The store can already record it: `ack_delivery` accepts any
     recipient (`crates/teamwire-store/src/delivery.rs:166-191`).
4. An `answer --re` changes no delivery row of its question
   (`crates/teamwire-msg/src/send.rs:297-321`).
5. The watch line `QUESTION-UNDELIVERED <id> from <role>` names no file
   (`crates/teamwire-watch/src/plan.rs:292-303`). The `UNDELIVERED` result
   line already names it: `Result: {path}` (`plan.rs:440-447`).

**Decisions:**

- **F1-a. The lead hook lists undelivered items.**
  - Add `teamwire_report::undelivered_to_lead(home, session)` in
    `crates/teamwire-report/src/lead_stop.rs`, beside `teams_without_watch`
    (l.17), and reuse `led_by` (l.38). Export it from
    `crates/teamwire-report/src/lib.rs`.
  - For each open team the session leads, the function returns one line
    per lead delivery in state `undelivered`, which means never typed.
    `unconfirmed` rows may have been typed, so they are left out.
  - It also returns one line per completion whose `report_state` is
    `Undelivered`.
  - Each line names the team, the id or role, the file path, and the clear
    command, for example:
    `teamwire: m008 (question from impl) was never typed into your pane. Read <team dir>/comms/m008.md, then run teamwire received deckh m008.`
  - Call the function from both harness-neutral hook sites in
    `crates/teamwire-cli/src/cmd/hook/lead.rs`. In `lead_stop_in`, extend
    `note` (l.75-81). In `lead_tool_in`, join the result with
    `lead_tool_note`'s result (l.118-123).
  - Do not call it inside `lead_tool_note`, which returns early for any
    call that is not member work (`crates/teamwire-report/src/lead_tool.rs:27-37`).
  - All three lead harnesses reach these sites, and each turns the note
    into text in the lead's turn without typing:
    - Claude Code and Codex read `additionalContext` from `tool_note_reply`
      (`crates/teamwire-cli/src/cmd/hook.rs:259-265`).
    - pi appends it to the tool result
      (`crates/teamwire-harness-pi/src/lead_extension.rs:339-353`).
    - Each harness's `hold_reply` adds the note at turn end
      (`crates/teamwire-harness-claude/src/hook.rs:93-98`,
      `crates/teamwire-harness-codex/src/hook.rs:134-135`,
      `crates/teamwire-harness-pi/src/hook.rs:75-76`).
- **F1-b. `received` accepts message ids.**
  - `mark_received` in `crates/teamwire-report/src/received.rs` treats an
    argument that parses as a message display id (`m` plus digits) as a
    message. It calls `ack_delivery(at, message, &Recipient::Lead)` inside
    the same `Store::write`.
  - It returns a new outcome for each case: marked, already read, unknown
    id, and not a message to the lead.
  - `crates/teamwire-cli/src/cmd/received.rs` prints the outcome per id.
  - The MCP server has no `received` tool (rca F1.5), so the lead's MCP
    tools change nothing. Per the user's answer, the MCP tools change only
    "if `received` is reachable there", and it is not.
- **F1-c. An answer settles its question.**
  - In `store_message` (`crates/teamwire-msg/src/send.rs:297-321`), when
    the new message is an `answer` with `re = q` and a delivery row
    `(q, <answer's sender as recipient>)` exists, the same write acks that
    row with `ack_delivery`.
  - The lead answering a member's question acks the question's `lead` row.
  - A `--re` that names a message with no row for the sender changes
    nothing. That covers a message that is not a question to the sender.
- **F1-d. The Claude lead tool hook runs on every tool call.**
  - `TOOL_HOOK_MATCHER` (`crates/teamwire-harness-claude/src/lead_setup/plugin.rs:29`)
    covers only edit and shell calls. So the hook does not run on
    `AskUserQuestion`, which is the call whose answer frees a blocked pane
    in deckh.
  - The plan drops the matcher so every tool call runs the hook, as Codex
    already does (`crates/teamwire-harness-codex/src/lead_setup/plugin.rs:112-128`)
    and pi already does (`lead_extension.rs:309`).
  - `lead_tool_note` still returns early for anything that is not member
    work.
- **F1-e. The question line names its file.** `QUESTION-UNDELIVERED` and
  `QUESTION-UNCONFIRMED` lines append ` File: <team dir>/comms/<id>.md`.
  The `Question` struct (`crates/teamwire-watch/src/snapshot.rs:116-141`)
  carries the path.

**Alternatives rejected:**

- The watch retries once the pane is free. This would supersede ADR 0003
  point 7 and INV-8, and it is the old double-typing bug. The user rejected
  it.
- Only a plain watch line. The lead would still read files by hand, and a
  watch that ended (deckh's Monitor expired at 12:48:59) says nothing.

**Tests that fail without the fix:**

| Test (file) | Assertion that fails on `develop` |
|---|---|
| `crates/teamwire-report/tests/undelivered_to_lead.rs` `a_question_and_a_result_never_typed_are_listed_with_their_files` | The returned lines name `m001` with `comms/m001.md` and the member's result with its outbox path (today the function does not exist; write the test against the existing store first, so it fails on the missing lines, not a missing name: add the function as a stub returning empty in the test commit) |
| same file, `a_row_the_lead_received_is_not_listed` | After `received <id>`, the list is empty |
| `crates/teamwire-cli/tests/hook_lead.rs` (or the existing lead hook test file) `the_tool_and_stop_hooks_carry_the_undelivered_note` | `lead_tool_in` and `lead_stop_in` output contains `was never typed` for a blocked-pane question, on `--harness claude-code`, `codex` and `pi` |
| `crates/teamwire-harness-claude/tests/lead_setup.rs` `the_lead_tool_hook_runs_on_every_tool` | The plugin's `PostToolUse` entry has no matcher limited to edit and shell calls |
| `crates/teamwire-cli/tests/status.rs` `received_with_a_message_id_clears_it_from_status_without_typing` | `status` no longer lists `m001` under deliveries that need attention, and the fake host records no new typing (today `received deckh m001` refuses "no member has this role") |
| `crates/teamwire-msg/tests/answer.rs` (or rca's `rca_f1.rs` shape) `an_answer_with_re_settles_the_questions_lead_row` | The question's `lead` row is `acked` after the answer (rca F1b red today) |
| `crates/teamwire-watch/tests/poll.rs` `an_undelivered_question_line_names_its_file` | The line contains `comms/m001.md` |

**Guidance in the same change:**

- `examples/skills/teamwire/reference.md:110-115`: the rows for
  `UNDELIVERED`, `QUESTION-UNDELIVERED` and `UNANSWERED`. Say that teamwire
  does not retry, that the hook lists the item, and that `received <id>`
  clears it.
- `examples/skills/teamwire/SKILL.md:700-712`: resend and answer. Add
  `received <id>` for a message read from its file, and say that an answer
  with `--re` settles the question.
- `docs/api/cli.md:593-603` (`received` takes ids), `:218-223` (the watch
  line names the file), and `:236-244` (status).
- `docs/requirements.md`: add a row for the lead's undelivered note and
  one for `received <id>`, beside DEL-16 (`:120`).
- `docs/failure-modes.md`: the INV-5 and INV-8 rows stay true, so leave
  them unchanged.
- The ADR 0043 text.
- `ui/src/shared/teamwire.ts`: no change. No `--json` shape changes. An
  acked row already has state `acked`, which the `DeliveryState` enum
  (`:18`) holds.

### F2. `TEAM-COMPLETE` while a member with an undelivered result works

**Root cause (verified, rca F2.1):**

- `judge_completion` never looks at the session for an `Undelivered` or
  `Unconfirmed` result. It settles the member with
  `settle("undelivered")` or `settle("unconfirmed")`
  (`crates/teamwire-watch/src/plan.rs:440-455`).
- Only the `Pending` arm calls `look` (`plan.rs:400`). The team then
  completes (`plan.rs:256-258`).
- `status` shows `undelivered` without checking the session
  (`crates/teamwire-watch/src/status.rs:262-263`). It does compute `seen`
  (`status.rs:325`).
- This is documented behaviour: WAT-6 (`docs/requirements.md:177`) and ADR
  0007 point 6 (`docs/adr/0007-completion-reporting.md:91-93`).

**Decision:**

- In the `Undelivered` and `Unconfirmed` arms, after the announcement
  (which stays once per completion), call `look(env, info, mm)`.
- The member's verdict follows what `look` sees:

  | `look` sees | Verdict |
  |---|---|
  | `Seen::Alive(Activity::Working)` or `Alive(Activity::Blocked)` | `Verdict::waiting()`. The member is mid-turn, not settled. |
  | `Seen::Alive(Activity::Idle)` | settled, as today |
  | `look` returns `None`, which means the member died and was announced | settled, as today |
  | `Seen::Absent` or `Seen::NoInfo` | settled, as today. The announcement or a one-time line adds `(session not visible)` |

- `status`'s `state_word` (`status.rs:249-264`) returns `working` for
  `Undelivered` or `Unconfirmed` when `seen == Seen::Alive(Activity::Working)`.
  `working` is already in the dashboard's word list, so the UI does not
  change.
- Superseded points: WAT-6 and ADR 0007 point 6, narrowly. ADR 0043 names
  both, and `docs/requirements.md:177` gets the new rule.

**Decision made by the spec member:** a `Blocked` session counts as not
settled. The member shows a dialog mid-turn, which is neither idle nor dead.

**Alternative rejected:** count any working session as not settled, for
every report state. Option B in rca F2.6 does that, and it widens the
change past the user's answer.

**Tests that fail without the fix:**

| Test (file) | Assertion that fails on `develop` |
|---|---|
| `crates/teamwire-watch/tests/poll.rs` `an_undelivered_result_of_a_working_member_does_not_complete_the_team` (rca's `rca_f2_f3.rs` shape) | No line starts `TEAM-COMPLETE` while the fake session is `Working`; today it prints `TEAM-COMPLETE 1/2 done; not done: impl undelivered` |
| same file, `an_undelivered_result_of_an_idle_member_still_settles` | `TEAM-COMPLETE` is printed once the fake session is `Idle` (guards the "one dead member never keeps the team open" rule; passes before and after) |
| same file, `an_undelivered_result_with_no_visible_session_settles_and_says_so` | The line contains `session not visible` |
| `crates/teamwire-watch/tests/status.rs` `an_undelivered_member_that_works_reads_working` | The status word is `working`, not `undelivered` |

**Tests that pin today's behaviour, to update in the same change** (rca
F2.5): `poll.rs` `undelivered_result_printed_once` (l.153),
`two_undelivered_same_second_two_lines` (l.178) and
`unconfirmed_result_announced_once` (l.825); `status.rs` `busy_team`
(l.70-83). Each may need its fake session set to `Idle`. Updating a test's
fixture is allowed, but loosening an assertion is not.

**Guidance in the same change:**

- `docs/requirements.md:177` (WAT-6) and `:55` (INV-12, if worded on
  settled).
- `docs/api/cli.md:228` (`TEAM-COMPLETE` row) and `:236` (status words).
- `examples/skills/teamwire/reference.md:110-111, 117`, and
  `examples/skills/teamwire/SKILL.md:611-623, 666-668`, which say when the
  watch exits.
- ADR 0043.

### F3. A message that wakes a finished member runs outside teamwire

**Root cause (verified, rca F3.1):**

- Only a `reprompt` bumps the generation
  (`crates/teamwire-msg/src/send.rs:311-317`; `bump` at
  `crates/teamwire-msg/src/attempt.rs:84-102`).
- `send` never reads the recipient's completions. `rules.rs` has no
  completion check.
- The watch settles the member on the old `delivered` result
  (`crates/teamwire-watch/src/plan.rs:351-355`). `status` shows
  `prompted outside teamwire` (`crates/teamwire-watch/src/status.rs:252-256`).
- `handoff` refuses a busy member
  (`crates/teamwire-close/src/handoff.rs:20-28`, through
  `crates/teamwire-close/src/facts.rs:97-102`).

**Decisions:**

- **F3-a. A waking message starts a generation.**
  - In `store_message`, for each member recipient of a message that is not
    a `reprompt`, read the member and its newest completion. If that
    completion is in the member's current generation, whatever its status
    (`done`, `blocked` or `failed`), and its summary does not start with
    `handoff:`, call `bump(tx, at, id, recipient)` in the same write. A
    reprompt already calls it.
  - Push the result into `bumped`, so that a known delivery failure undoes
    the bump the way it undoes a reprompt's (REP-3, `attempt.rs:181-`). An
    `unconfirmed` delivery keeps the bump.
  - The new generation ends when the member records `done`, through the
    existing Stop hook path (`crates/teamwire-report/src/turn_end.rs:105-155`).
- **F3-b. A message to a member that asked for a handoff is held.**
  - When that newest completion's summary starts with `handoff:`, the same
    write stores the delivery row as `undelivered` with error code
    `held_for_handoff`. That code is a new `DeliveryErrorCode` variant in
    `crates/teamwire-core/src/message.rs:112-145`.
  - The attempt loop types nothing for that row. The sending command
    prints `<role>: held for handoff` and does not treat the hold as a
    failure (exit 0).
  - `error_code` is free `TEXT`, so the code needs no schema migration.
    The dashboard has no error-code enum.
- **F3-c. The handoff passes held messages on.** The handoff already
  counts `undelivered` rows as unread, and its first line tells the fresh
  session to read them (`crates/teamwire-launch/src/handoff.rs:86-109,
  195-196, 284-290`). So held rows reach the fresh session with no new
  handoff code. A test pins that.
- **F3-d. A reprompt types the held messages after its own text.**
  - When a `reprompt` goes to a member with `held_for_handoff` rows, its
    typed line appends one sentence after the reprompt text:
    `Held for you, oldest first: m010 (answer from lead), m012 (note from lead). Read them with teamwire inbox.`
  - The plan builds that sentence where the reprompt's line is built
    (`crates/teamwire-deliver/src/lines.rs:59-110`, `message_line` and
    `message_line_to`).
  - The reprompt's bump starts a new generation, so later messages are not
    held. That ends the hold.
  - The member's `inbox` read acks the held rows, as for any message.
- **F3-e. `resend` refuses a held row.** `resend`
  (`crates/teamwire-msg/src/resend.rs:130-143`) refuses a row with code
  `held_for_handoff` and names `handoff` or `reprompt`. Otherwise a member
  following the contract's "resend once"
  (`crates/teamwire-launch/contracts/talking_head.md:12-16`) would type
  into the session that asked to be handed off.

**Decision made by the spec member:** the reprompt names the held
messages and points to `teamwire inbox`. The member reads their text there,
as it does for every message. The plan does not paste the full bodies into
the reprompt's line. One line typed into a pane is the agreed shape for
every message (`message_line`), and a second typed line right after it
could meet a busy pane.

**Alternatives rejected:**

- Hold every message to a finished member. The user rejected this: other
  members' questions would wait for the lead.
- Refuse the send (rca F3.6 option C).
- Count the woken turn as working without a generation (rca F3.6 option
  D). A handoff would still refuse, and the next `done` would stay in the
  old generation.

**Tests that fail without the fix:**

| Test (file) | Assertion that fails on `develop` |
|---|---|
| `crates/teamwire-msg/tests/woken.rs` `an_answer_to_a_member_with_a_done_result_starts_a_generation` | The member's generation is one higher after the send (today unchanged, `send.rs:311`) |
| same file, `a_note_to_a_member_with_a_blocked_or_failed_result_starts_a_generation` | Same, for `blocked` and `failed` results (one case each) |
| same file, `a_known_delivery_failure_undoes_the_woken_generation` | The generation is back to its old value after a `no_place` failure |
| same file, `a_message_to_a_member_with_a_handoff_result_is_held_not_typed` | The row is `undelivered` with code `held_for_handoff`, and the fake host records no typing |
| same file, `a_reprompt_names_the_held_messages_after_its_text` | The typed line holds `Held for you, oldest first: m001` after the reprompt body |
| `crates/teamwire-msg/tests/resend.rs` `a_held_message_is_not_resent` | `resend` refuses and names `handoff` |
| `crates/teamwire-launch/tests/handoff.rs` (or the existing handoff test file) `a_handoff_first_line_counts_the_held_messages` | The fresh session's first line contains `1 unread message(s)` for one held row (guards F3-c; must fail when the held row is written in a state outside `UNREAD`) |
| `crates/teamwire-watch/tests/poll.rs` `a_member_woken_by_a_message_is_tracked` (rca `f3_…` shape) | No `TEAM-COMPLETE` while the woken member works (red today) |

**Tests that pin today's behaviour, to update in the same change** (rca
F3.5):

- `crates/teamwire-watch/tests/status.rs` `outside_prompt_shown` (l.222).
  That test still holds for a prompt typed by hand, so keep it.
- `crates/teamwire-msg/tests/reprompt.rs` and `stopped.rs`.
- `crates/teamwire-report/tests/turn_end.rs` `completion_after_reprompt_is_new_result`.

**Guidance in the same change:**

- `examples/skills/teamwire/SKILL.md:714-715`. Say that any message to a
  finished member wakes it as a new generation, except after a `handoff:`
  result, where the message is held until `handoff` or `reprompt`. Also
  update `:841-847` (act on `handoff:`).
- The "Context handoff" section of `examples/skills/teamwire/reference.md`.
- `crates/teamwire-cli/src/cmd/handoff_guide.md`.
- `crates/teamwire-launch/contracts/talking_head.md:12-16`, which says a
  held row is not resent, and its snapshot tests under
  `crates/teamwire-launch/src/contract/tests*.rs`.
- `docs/requirements.md`: WAT-8 (`:179`), INV-14 (`:57`), REP-3 (`:129`).
- `docs/failure-modes.md:41-42` (INV-14 rows).
- `docs/api/cli.md:524-590` (`send` prints `held for handoff`, exit 0; and
  `resend` refuses a held row) and `:237` (status words).
- ADR 0043.

---

## US2: the `tasks.md` reader (F4, F5)

**Root cause (verified):**

- ADR 0042 decision 1 deferred the reader
  (`docs/adr/0042-spec-kit-after-the-review.md:19-27, 82-87`): "Until it
  exists, the lead writes the phase list ... and the disjoint-files test
  for a split by hand".
- The lead's planning check asks for a manual grep of every changed name
  (`examples/skills/teamwire/SKILL.md:177-190`).
- The split rule tells the lead to read the task paths by hand
  (`examples/skills/teamwire/reference.md:579-596`;
  `examples/presets/speckit.json` lines 10 and 13).
- No command computes the overlap or the files that mention a path. That
  is how F4's missed parallel tasks (T048, T051) and F5's three missing
  files happened.

**Decisions:**

- **Command.** `teamwire speckit draft <tasks.md> [--repo <dir>]` is a new
  `speckit` command group in `crates/teamwire-cli/src/args.rs:35-124`
  (the `Afk` group pattern), dispatched in
  `crates/teamwire-cli/src/main.rs:84-115`.
  - The code lives in `crates/teamwire-cli/src/cmd/speckit.rs`, with
    sub-files under `crates/teamwire-cli/src/cmd/speckit/` attached with
    `#[path]`, as `doctor_speckit.rs:19-25` does.
  - Planned files: `parse.rs` (task lines), `overlap.rs` (phase and story
    pairs, independent tasks), `mentions.rs` (repository search), and
    `render.rs`.
  - Each file stays under 400 lines.
  - The command is read only. It writes no file and no store row.
  - The output is text only, with no `--json` (FR-011 to FR-015), so the
    dashboard does not change.
- **Parsing.**
  - A phase starts at a `## Phase <n>: <title>` heading.
  - A task line is `- [ ] T<digits>`, or `- [x]` or `- [X]`, then an
    optional `[P]`, an optional `[US<n>]`, and the description.
  - A path is a token inside backticks, or a bare token, that contains a
    `/` or ends in `.<ext>`, with trailing punctuation removed. URLs are
    left out.
  - The story is the `[US<n>]` label. A task with no label belongs to the
    "setup and polish" group.
- **Overlap.**
  - For every pair of phases and every pair of stories, list the paths
    both name.
  - List the tasks that name no path.
  - List the tasks whose paths no other task names.
- **Owns drafts.** There is one draft per story, plus one for setup and
  polish. Each draft holds the task paths plus the files that mention any
  of them. The repository files come from `git ls-files -z`, run through
  the Platform runner the same way `doctor_speckit.rs:78-100` runs git.
- **What counts as a mention.** A file mentions a task path when the file
  holds one of these:
  - the exact path text;
  - a dotted module name of the path: any dotted suffix of the path without
    its extension, with at least two parts, such as `harness.tools` for
    `apps/ai/harness/tools.py`;
  - the full file name with its extension, such as `tools.py`, as a whole
    token;
  - a `paths:` glob in a `.github/workflows/*.yml` or `*.yaml` file that
    matches the path.

  A bare stem such as `tools` never counts. Each added file gets its
  reason, such as `names module harness.tools` or
  `CI paths glob apps/ai/**`.
- **YAML and globs.**
  - Workflow files are read with `parse_yaml` from
    `crates/teamwire-cli/src/cmd/doctor_speckit/yaml.rs:118`.
  - The reader refuses anchors and tags (`yaml.rs:7-8`). For a workflow it
    cannot read, the command prints a line naming that workflow. The
    command does not count such a workflow as matching nothing.
  - Glob matching reuses `teamwire_verify::Owns`
    (`crates/teamwire-verify/src/owns.rs:9-30`, `literal_separator(true)`).
    Today that type is `pub(crate)`, so the change makes it `pub` and
    exports it from `crates/teamwire-verify/src/lib.rs`.
  - `teamwire-cli` already depends on `teamwire-verify`
    (`crates/teamwire-cli/Cargo.toml:32`), so no `Cargo.toml` or
    `Cargo.lock` changes.
- **Excluded from mentions.** The feature folder that holds the `tasks.md`
  (`specs/NNN-name/`) names every path, so its files are left out.
- **Errors.** A missing `tasks.md`, or one with no task lines, exits
  non-zero and names the problem. The command uses the exit code that
  `docs/api/cli.md` gives for bad input.
- **ADR.** `docs/adr/0044-*.md` supersedes ADR 0042 decision 1 in part:
  the reader now exists. The new ADR records the matching rule and the
  rule that the drafts are drafts. ADR 0042 itself is not edited.

**Decision made by the spec member:** the reader takes `--repo`, which
defaults to the git top level of the folder that holds `tasks.md`. The
lead can then run it from anywhere.

**Alternatives rejected:**

- A preset step that has the lead do the overlap by hand. The deckh lead
  already had that, and it missed the tasks.
- Adding `globset` to `teamwire-cli`'s `Cargo.toml`. That changes
  `Cargo.lock`, which AGENTS.md reserves for the lead.
- A new crate. The layering rules (`docs/conventions.md:305-327`) do not
  call for one.
- Bare-stem matching. The user excluded it.

**Tests that fail without the fix:** every test fails today because the
subcommand does not exist. To fail on an assertion rather than on a
missing name, the first commit adds the command as a stub that prints
nothing. The parser and the render get unit tests at the bottom of their
files, and the command gets integration tests in
`crates/teamwire-cli/tests/speckit_draft.rs`, following
`crates/teamwire-cli/tests/preset.rs:45-53, 238-246`.

| Test | Assertion |
|---|---|
| `phases_that_name_one_file_are_listed_as_sharing_it` | Output has phases 3 and 4 with `src/a.rs` under shared files |
| `stories_that_share_a_file_are_named` | The line naming stories that share files lists US1 and US2 |
| `a_task_with_no_path_is_listed_as_changing_no_file` | T048-like line listed |
| `a_task_whose_files_no_other_task_names_is_listed_as_independent` | T051-like line listed |
| `a_ci_paths_glob_that_matches_a_task_path_adds_the_workflow` | `.github/workflows/replay.yml` in the US1 draft with reason `CI paths glob` |
| `a_file_naming_the_dotted_module_is_added` | `.importlinter` naming `harness.tools` is in the draft |
| `a_test_naming_the_full_file_name_is_added` | a test file naming `tools.py` in a list is in the draft |
| `a_file_naming_only_the_bare_stem_is_not_added` | a file naming only `tools` is absent |
| `the_feature_folder_is_not_a_mention` | `specs/001-x/plan.md` is absent |
| `an_unreadable_workflow_is_named_not_skipped` | Output names the workflow with an anchor |
| `a_missing_or_empty_tasks_file_exits_non_zero` | Non-zero exit and a message naming the file |
| `the_drafts_are_labelled_as_drafts` | Output contains `DRAFT` before the owns lists |

**Guidance in the same change:**

- `examples/skills/teamwire/SKILL.md`:
  - `:120` and `:126`: the speckit row and the pointer to the preset.
  - `:172-190`: planning-check items 3 and 4. For a spec-kit feature, run
    `teamwire speckit draft` and check its drafts. Keep the manual grep for
    names the draft cannot see.
- `examples/skills/teamwire/reference.md`:
  - `:301-309`, `:557` and `:579-596`: split per story from the command's
    output, and show the table to the user before launch.
- `examples/presets/speckit.json`:
  - line 10 (BUILD) and line 13 (SPLIT ONLY ON DISJOINT FILES): run the
    command and show its table.
  - line 63 (the implementer `_note`).
- `examples/skills/afk/SKILL.md`, where ADR 0042's consequences name it
  (`docs/adr/0042-spec-kit-after-the-review.md:91-94`).
- `docs/api/cli.md`: a new `### teamwire speckit draft` section near
  `### teamwire preset` (`:725`).
- `docs/ARCHITECTURE.md`, if it lists the CLI's command groups.
- ADR 0044.

---

## US3: small fixes (F6, F8, F10, F11)

### F6. The checklist stop before `implement`

**Root cause (verified):**

- Commit `b60fdf7` fixed part of F6. The preset rule at
  `examples/presets/speckit.json:11` and
  `examples/skills/teamwire/reference.md:505-522` now name an unticked
  item of `specify`'s own `requirements.md` as a cause of the stop.
- Two gaps remain:
  - Neither text says that `specify` can leave items unticked on purpose,
    with the reason in the checklist's Notes, as deckh's migration spec
    did.
  - Neither tells the lead what to do before launching the implementer:
    read the checklist, then waive each unticked item in the brief, or have
    the spec member fix it.
- `reference.md:511-512` says to "check that `requirements.md` has no
  unticked item", with no waive path.

**Decision:**

- Rewrite the rule at `speckit.json:11` and `reference.md:505-522`. The
  new text says that `specify` can leave items unticked with a reason in
  Notes after three rounds or on purpose.
- The new text also says that before the implementer launches, the lead
  reads `checklists/requirements.md` and acts on each unticked item. The
  lead waives the item in the implementer's brief, by quoting it and
  saying "proceed", or reprompts the spec member to fix it.

**Test that fails without the fix:**

| Test | Assertion |
|---|---|
| `crates/teamwire-cli/tests/preset.rs` `the_speckit_preset_has_the_lead_read_the_checklist_before_the_implementer` | The speckit preset rules contain `before the implementer launches` (or the chosen exact phrase), `checklists/requirements.md`, and `waive`; fails on today's text |

**Guidance:** the rule text itself, plus `reference.md:505-522`.

### F8. The closer's local gate and CI gates

**Root cause (verified):**

- The closer preset's rule (`examples/presets/closer.json:13`) and its task
  (`closer.json:28`, "Run the repository's full gate (CHANGEME) once on
  each head that ships") have one gate slot.
- They give no way to say which gate this machine may run and which gates
  only CI runs.
- `examples/skills/teamwire/SKILL.md:876-880` says "Fill in the preset's
  full gate".

**Decision:**

- Replace the one slot with two: `Local gate: CHANGEME` and
  `Gates left to CI: CHANGEME`.
- The closer runs the local gate on each head that ships, and does not run
  the CI gates.
- Its report states which gate it ran, with the exit code, and which gates
  it left to CI.
- When the user allows the full gate locally, the lead puts the full gate
  in the local slot and writes `none` in the CI slot.

**Test that fails without the fix:**

| Test | Assertion |
|---|---|
| `crates/teamwire-cli/tests/preset.rs` `the_closer_task_has_a_local_gate_and_a_ci_gate_slot_and_reports_both` | The closer task contains `Local gate: CHANGEME` and `Gates left to CI: CHANGEME`, and a report instruction naming both |

The existing `the_closer_task_lists_setups_and_has_a_restore_fallback`
(`preset.rs:1520-1531`) stays.

**Guidance:**

- `examples/presets/closer.json:13, 28`.
- `examples/skills/teamwire/SKILL.md:864-880`, the closer step.
- The closer row at `SKILL.md:280`.

### F10. How long a member's current tool call has run

**Root cause (verified):**

- `status` builds a member's row from `state_word`, which gives `working`
  for any `Alive(Working)` (`crates/teamwire-watch/src/status.rs:236-274`).
  `--role` adds only `resume_command` (`status.rs:327-334`).
- `MemberView` (`crates/teamwire-watch/src/lib.rs:57-81`) has no field for
  the current tool call.
- The `Harness` trait (`crates/teamwire-core/src/harness.rs:24`) has no
  method that reports a running tool call.
- teamwire's existing `ToolCall` type
  (`crates/teamwire-core/src/tool_call.rs:10-29`) has no name and no start
  time. That type serves the lead's tool hook.

**Decision:**

- **Trait method.** Add a `Harness` method with a default, in the same
  shape as `context_fill` (`harness.rs:112`):
  `fn current_tool(&self, session: &SessionId, cwd: Option<&Path>) -> Result<Option<ToolRun>, HarnessError> { Ok(None) }`.
  - It returns a new core type, `ToolRun { name: String, started_at: Timestamp }`,
    in a new file, `crates/teamwire-core/src/tool_run.rs`.
- **Per harness:**
  - **Claude Code.** Walk the transcript tail newest first, with the same
    finder and tail reader as `context_fill`
    (`crates/teamwire-harness-claude/src/fill.rs:76-118`). Find the newest
    assistant `tool_use` block whose `id` has no later `tool_result` with
    that `tool_use_id`. Return its `name` and the line's `timestamp`.
    - rca checked this shape on a real transcript: localdb's `tool_use` at
      14:12:26.311Z, with its result 550 s later.
    - The test fixture copies real lines from a local Claude transcript,
      with content trimmed.
  - **Codex.** Walk the rollout tail (`crates/teamwire-harness-codex/src/session.rs:60, 195, 262`).
    Find the newest `response_item` of type `function_call` (or
    `local_shell_call` or `custom_tool_call`) whose `call_id` has no later
    `*_output`. The entry shape is INFERRED, because no Codex rollout
    exists on this machine.
    - The implementer confirms the shape against Codex's published rollout
      format or a real rollout file, and records which one it used.
  - **pi.** teamwire owns the member extension, so it records the start
    itself. On pi's `tool_call` event, the extension writes
    `tool: {name, started_at_ms}` into `<id>.teamwire.json`. On
    `tool_result`, it clears the field. The writes go through the
    extension's state writer (`crates/teamwire-harness-pi/src/extension.rs:102-116, 179-200, 277`).
    The lead extension already listens to `tool_call`
    (`crates/teamwire-harness-pi/src/lead_extension.rs:309`).
    `current_tool` reads that field from the state file
    (`crates/teamwire-harness-pi/src/session.rs:11-17`).
- **Watch.** `crates/teamwire-watch/src/tool.rs` is new, modelled on
  `crates/teamwire-watch/src/context.rs:13-26`. It calls
  `harness.current_tool` through `ctx.adapters`, as
  `crates/teamwire-watch/src/observe.rs:60-68` reaches `is_alive`.
  - `status_with` (`status.rs:307-357`) fills a new `MemberView` field,
    `current_tool: Option<ToolView { name, started_at, seconds }>`, only
    when `--role` names the member and the state is `working`. `seconds`
    comes from `Clock`.
- **CLI.** `crates/teamwire-cli/src/cmd/status.rs:245-269` prints
  `tool: <name>, running <duration>` under `--role`, or
  `tool: unknown (this harness does not report it)` when the method
  returns `None` for a working member.
- **Dashboard.** `ui/src/shared/teamwire.ts:71-95` gets
  `current_tool: ToolView.nullish()` in the zod `MemberView`, in the same
  commit, because the field changes `status --json`.

**Tests that fail without the fix:**

| Test | Assertion |
|---|---|
| `crates/teamwire-harness-claude/tests/sessions.rs` `the_current_tool_is_the_newest_tool_use_without_a_result` | Returns `Bash` with the fixture's start timestamp; returns `None` when the result line follows |
| `crates/teamwire-harness-codex/tests/session.rs` `the_current_tool_is_the_newest_call_without_output` | Same, on a rollout fixture |
| `crates/teamwire-harness-pi/tests/session.rs` `the_current_tool_comes_from_the_state_file` | Same, on a state file with `tool` |
| `crates/teamwire-harness-pi/tests/launch.rs` (or the extension snapshot test) `the_member_extension_stamps_and_clears_the_tool` | The generated extension source registers `tool_call` and writes `tool` |
| `crates/teamwire-watch/tests/status.rs` `status_role_shows_the_current_tool_and_its_duration` | With `FakeHarness` returning a `ToolRun` 900 s old, the view's `current_tool.seconds == 900` |
| `crates/teamwire-cli/tests/status.rs` `status_role_prints_how_long_the_tool_has_run` | Output contains `tool: Bash, running 15m` |
| `ui/` zod test (per `ui/AGENTS.md`), if the schema has fixture tests | A status JSON with `current_tool` parses |

`FakeHarness` (`tests/fakes/`) may need a setter for `current_tool`.
`tests/fakes/` is a protected path (AGENTS.md), so the implementer asks the
lead before editing it. The fake can use the trait default if the test
reaches the method another way.

**Guidance:**

- `docs/api/cli.md:236-244`, status `--role`.
- `examples/skills/teamwire/SKILL.md`, the place that tells the lead to
  read `status --role` for a member that looks stuck.
- `docs/api/` harness docs, if a page describes the `Harness` trait's
  methods.

### F11. `close --dry-run` and symlinks

**Root cause (verified):**

- `ignored_warning`
  (`crates/teamwire-close/src/member_close.rs:59-90`) lists the
  `git status --porcelain --ignored` paths (`!! `) and prints
  `removal deletes ignored files: …` (l.89).
- It never checks whether a path is a symlink.
- In deckh's worktrees, `apps/ai/.venv` and `node_modules` were symlinks
  to the main checkout. Only the links would go.
- `ignored_warning` has four callers:
  - `crates/teamwire-close/src/plan.rs:139-146` and `plan.rs:250`;
  - `crates/teamwire-close/src/remove.rs:207`;
  - `crates/teamwire-close/src/team_close.rs:281`.

**Decision:**

- For each listed path, `std::fs::symlink_metadata(member.cwd.join(p))`.
  This is a read, and it is allowed outside the Platform write rule.
- Symlinks are listed apart from the deleted files:
  `removal deletes ignored files: <files>; removes symlinks (targets kept): <path> -> <target>, ...`.
- A broken link reads `<path> -> <target> (broken)`.
- The target comes from `std::fs::read_link`.
- If every listed path is a symlink, the "deletes" part is left out.
- The ten-path limit applies to the two lists together.

**Test that fails without the fix:**

| Test (`crates/teamwire-close/tests/ignored_files.rs`) | Assertion |
|---|---|
| `a_symlinked_ignored_folder_is_named_as_a_symlink_with_its_target` | The warning contains `.venv -> <target>` under `removes symlinks`, and does not list `.venv` under `deletes` |
| `a_broken_symlink_is_marked_broken` | Contains `(broken)` |

The existing tests in that file (`:36-114`) stay as they are.

**Guidance:** `docs/api/cli.md:507`, the close dry-run line.

---

## Files per story

The lead uses this table to measure which stories share files. The
`tasks.md` lines name the same paths.

| Story | Files |
|---|---|
| US1 | `crates/teamwire-core/src/message.rs`; `crates/teamwire-store/src/delivery.rs`; `crates/teamwire-msg/src/{send.rs,attempt.rs,resend.rs}`; `crates/teamwire-msg/tests/{woken.rs,answer.rs,resend.rs,reprompt.rs,stopped.rs}`; `crates/teamwire-deliver/src/lines.rs`; `crates/teamwire-report/src/{received.rs,lead_stop.rs,lib.rs}`; `crates/teamwire-report/tests/{undelivered_to_lead.rs,turn_end.rs}`; `crates/teamwire-watch/src/{plan.rs,status.rs,snapshot.rs}`; `crates/teamwire-watch/tests/{poll.rs,status.rs}`; `crates/teamwire-launch/contracts/talking_head.md`; `crates/teamwire-launch/src/contract/tests*.rs`; `crates/teamwire-launch/tests/handoff.rs`; `crates/teamwire-harness-claude/src/lead_setup/plugin.rs`; `crates/teamwire-harness-claude/tests/lead_setup.rs`; `crates/teamwire-cli/src/cmd/{received.rs,hook/lead.rs,handoff_guide.md}`; `crates/teamwire-cli/tests/{status.rs,hook_lead.rs}`; `docs/adr/0043-*.md`; `docs/requirements.md`; `docs/failure-modes.md`; `docs/api/cli.md`; `examples/skills/teamwire/{SKILL.md,reference.md}` |
| US2 | `crates/teamwire-verify/src/{owns.rs,lib.rs}`; `crates/teamwire-cli/src/{args.rs,main.rs}`; `crates/teamwire-cli/src/cmd/speckit.rs`; `crates/teamwire-cli/src/cmd/speckit/*.rs`; `crates/teamwire-cli/tests/speckit_draft.rs`; `docs/adr/0044-*.md`; `docs/api/cli.md`; `examples/skills/teamwire/{SKILL.md,reference.md}`; `examples/presets/speckit.json`; `examples/skills/afk/SKILL.md` |
| US3 | F6: `examples/presets/speckit.json`, `examples/skills/teamwire/reference.md`, `crates/teamwire-cli/tests/preset.rs`. F8: `examples/presets/closer.json`, `examples/skills/teamwire/SKILL.md`, `crates/teamwire-cli/tests/preset.rs`. F10: `crates/teamwire-core/src/{harness.rs,tool_run.rs,lib.rs}`; `crates/teamwire-harness-{claude,codex,pi}/src/*`; their `tests/`; `crates/teamwire-watch/src/{tool.rs,status.rs,lib.rs}`; `crates/teamwire-watch/tests/status.rs`; `crates/teamwire-cli/src/cmd/status.rs`; `crates/teamwire-cli/tests/status.rs`; `ui/src/shared/teamwire.ts`; `docs/api/cli.md`. F11: `crates/teamwire-close/src/member_close.rs`, `crates/teamwire-close/tests/ignored_files.rs`, `docs/api/cli.md` |

**Files shared between stories:**

| Files | Stories |
|---|---|
| `docs/api/cli.md`, `examples/skills/teamwire/SKILL.md` and `reference.md` | all three |
| `examples/presets/speckit.json` | US2 and US3 (F6) |
| `crates/teamwire-watch/src/status.rs`, `crates/teamwire-watch/tests/status.rs`, `crates/teamwire-cli/tests/status.rs` | US1 and US3 (F10) |
| `crates/teamwire-harness-claude/` | US1 (`lead_setup/plugin.rs`) and US3 (the session reader). These are different files. |
