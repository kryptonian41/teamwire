---

description: "Task list for 001-deckh-fixes"
---

# Tasks: Fixes from team deckh's live run

**Input**: Design documents from `specs/001-deckh-fixes/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/cli.md, quickstart.md

**Tests**: These are required. The spec (FR-021) and the constitution
(principle II) ask for a test that fails without each fix. Each test task
comes before its fix task. Run the new test on the code without the fix and
see it fail **on its assertion**. A failure from a missing name does not
count. So when a test needs a name that does not exist yet, the test task
adds that name as a stub with no behaviour, in the file the fix task
changes. Record the command and the failing assertion line in the commit's
progress line.

**Guard tests**: Some tests pass on the base, because the base already
behaves as they assert. Each task below marks such a test **guard**. A
guard test keeps a rule the fix must not break. Write it with the failing
tests, run it on the base and record that it passes, and keep it passing
after the fix. Only a test that fails on the base is called failing here.

**Existing tests that change** (user decision, 2026-11-13): an existing test
may change its expected value when it pins a rule that ADR 0043, 0044 or
0045, or a user answer in `spec.md`, changes. No assert may be removed. Each
task names the tests it may change, with file and line. List every test you
changed, with its file and line and the rule it pinned, in your report.

**Guidance**: Each fix task names the skill, preset, reference, doc, README
and `ui/src/shared/` text it changes. That text goes in the **same commit**
as the code (constitution principle III).

**Root causes, the decision for each fix, and the line numbers on
`develop`** are in `specs/001-deckh-fixes/research.md`. Read the section
for your finding before its first task.

**Protected paths** (AGENTS.md): `tests/fakes/`, `scripts/`, `Cargo.toml`
and `Cargo.lock` are protected. No task changes any of them. Ask the lead
before editing one.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: The task can run in parallel. It touches different files and
  depends on no task that is still open.
- **[Story]**: US1, US2, US3 or US4.

---

## Phase 1: Setup

**Purpose**: Confirm the base and the commands before changing code.

- [X] T001 Confirm the worktree's code matches `develop` (`eda2e0b`) with `git diff --stat eda2e0b -- crates docs examples ui` (expect no output), and run `cargo test -p teamwire-watch --locked` once as the baseline; record both in your report (changes no file)

---

## Phase 2: Foundational

**Purpose**: No code prerequisite is shared by all four stories. Each
story starts from `develop`. The stories share files only for guidance
text and a few status and schema files, listed in research.md under "Files
shared between stories".

- [X] T002 Read `specs/001-deckh-fixes/research.md`, `specs/001-deckh-fixes/data-model.md`, `specs/001-deckh-fixes/contracts/cli.md` and `.specify/memory/constitution.md` before any story task (changes no file)

**Checkpoint**: Any story may start.

---

## Phase 3: User Story 1 - Results and questions reach the lead, and the watch stays on (Priority: P1) 🎯 MVP

**Goal**: The lead's hooks list each result and question that was never
typed, with its file, and nothing is typed again. `received <id>` and
`answer --re` settle a message. The watch keeps a member whose session
works. A waking message starts a generation, recorded as `member.woken`. A
message to a member that asked for a handoff is held for the handoff, and
`status` lists it on its own line.

**Independent Test**: Run `cargo test -p teamwire-store -p teamwire-report -p teamwire-msg -p teamwire-watch -p teamwire-otel -p teamwire-launch -p teamwire-harness-claude --locked`, `cargo test -p teamwire-cli --locked --test status --test hook_lead`, `cargo test -p teamwire-cli --locked --bin teamwire status`, and, from `ui/`, `bun test src/shared src/web/model` and `bun run typecheck`. The tests listed in research.md under F1, F2 and F3 pass. Each test marked failing fails with its fix reverted.

### ADR for US1

- [X] T003 [US1] Write `docs/adr/0043-lead-reads-undelivered-and-woken-members-are-tracked.md`. Record decisions F1-a to F1-f, F2 and F3-a to F3-f from `specs/001-deckh-fixes/research.md`, including the `member.woken` event type, schema version 11, held rows on their own `status` line, and the seat text that no longer tells a member to resend to the lead. Name each superseded point: WAT-6 (`docs/requirements.md:177`), ADR 0007 point 6 (`docs/adr/0007-completion-reporting.md:91-93`), INV-14 (`docs/requirements.md:57`) and REP-3 (`docs/requirements.md:129`), widened to a waking message, ADR 0024 point 2 (`docs/adr/0024-the-lead-marks-a-result-received.md:32-37`, `received` takes roles only), and ADR 0035 point 1 (`docs/adr/0035-the-lead-tool-hook-notes-member-work.md:41-49`, the Claude Code tool list). Also name ADR 0040 point 5's hint sentence (`docs/adr/0040-user-sender.md:86-87`, `run teamwire resend <team> <id> --to lead`). Record that schema version 11 only goes forward, as ADR 0038 says of version 10, and that the user installs the new build only after team `deckh` closes. State that ADR 0003 point 7 and INV-8 stay in force. Do not edit any older ADR.

### F1: the lead learns of undelivered items without typing

- [X] T004 [US1] Write failing tests `a_question_and_a_result_never_typed_are_listed_with_their_files`, `a_row_the_lead_received_is_not_listed` and `a_result_with_no_result_file_names_the_report` in `crates/teamwire-report/tests/undelivered_to_lead.rs`. Add `pub fn undelivered_to_lead(home, session, platform, clock) -> Vec<String>` as a stub returning an empty list in `crates/teamwire-report/src/lead_stop.rs`, and export it from `crates/teamwire-report/src/lib.rs`. Each test fails on its line assertion (`comms/m001.md`, the outbox path, the `reports/<role>.md` path).
- [X] T005 [US1] Implement `undelivered_to_lead` in `crates/teamwire-report/src/lead_stop.rs`, reusing `led_by` (l.38). For each open team the session leads, add one line per lead delivery in state `undelivered`, plus one line per completion with report state `Undelivered`. Each line holds the id or role, the file path, and `teamwire received <team> <id|role>`, with the phrase `was never typed into your pane` (see `specs/001-deckh-fixes/contracts/cli.md`). Time comes from the `clock` argument.
  - A result line uses the watch's path rule. Move `result_written` and `result_path` from `crates/teamwire-watch/src/text.rs:93-114` into `crates/teamwire-deliver/src/lines.rs`, beside `CommsCopy` (l.100), export them from `crates/teamwire-deliver/src/lib.rs`, and import them in `crates/teamwire-watch/src/plan.rs:19` and `crates/teamwire-watch/src/text.rs`. Both crates are layer 4 and cannot call each other (`docs/conventions.md:316`).
  - If a query is missing, add it to `crates/teamwire-store/src/delivery.rs`.

  T004 passes, and `cargo test -p teamwire-watch --locked` still passes.
- [X] T006 [US1] Write failing tests:
  - In `crates/teamwire-cli/tests/hook_lead.rs` (or the existing lead-hook test file under `crates/teamwire-cli/tests/`), `the_tool_and_stop_hooks_carry_the_undelivered_note`: it drives `lead_tool_in` and `lead_stop_in` with `--harness claude-code`, `codex` and `pi` on a team whose question to the lead is `undelivered`, and asserts that each reply holds `was never typed into your pane`.
  - At the bottom of `crates/teamwire-harness-claude/src/lead_setup/plugin.rs`, the unit test `the_lead_tool_hook_runs_on_every_tool`, which asserts that the plugin's `PostToolUse` entry has no `matcher`.
  - In `crates/teamwire-launch/src/contract/tests_findings.rs`, change `a_talking_member_does_not_resend_a_message_that_was_not_delivered` (l.186-200): its assert that the text holds `` `/opt/x/teamwire resend demo <message id>` `` becomes an assert that the text does not hold it, and it gains an assert for the new sentence from `specs/001-deckh-fixes/contracts/cli.md` ("The member's seat text"). Keep its other asserts. This is a changed test (it pins the resend rule that F1-f changes).
  - In `crates/teamwire-cli/tests/send.rs`, the failing test `the_lead_copy_hint_names_the_hook_not_resend`: on the `live_rig()` of `a_users_note_exits_0_when_only_the_lead_copy_fails`, the stderr line that starts `lead copy not delivered: m001` names the lead's hook and holds no `resend`. That rig's copy is `undelivered`. If the rig can also make an `unconfirmed` copy, add that case: its line says `teamwire status lists it` and holds no `resend`.
- [X] T007 [US1] Call `teamwire_report::undelivered_to_lead` from `lead_stop_in` (extend `note`, l.75-81) and from `lead_tool_in` (join with `lead_tool_note`'s result, l.118-123) in `crates/teamwire-cli/src/cmd/hook/lead.rs`. Remove the edit-and-shell-only `TOOL_HOOK_MATCHER` from `crates/teamwire-harness-claude/src/lead_setup/plugin.rs:26-29, 92`, so every tool call runs the hook, and say why in a comment (the answer to `AskUserQuestion` frees a blocked pane). In `crates/teamwire-launch/contracts/talking_head.md:12-16`, replace the sentence that tells a member to retry a message to the lead with `resend` by the sentence in `specs/001-deckh-fixes/contracts/cli.md`. In the same commit, change the hint in `crates/teamwire-cli/src/cmd/send.rs:139-142`: for an `undelivered` copy, `lead copy not delivered: {id}; the lead's hook lists it at the lead's next tool call`; for an `unconfirmed` or `pending` copy, which may have been typed and which the hook does not list, `lead copy not delivered: {id}; it may have reached the lead's pane, and teamwire status lists it`. Keep the `lead copy not delivered:` start, which the dashboard matches (`ui/src/web/model/actions.ts:33`), and update the hint in `docs/api/cli.md:543-548`.
  - Changed test: `a_users_note_exits_0_when_only_the_lead_copy_fails` (`crates/teamwire-cli/tests/send.rs:869`) expects today's hint at `:884`. Its expected line becomes the new hint. Keep every assert.
  - Changed test: `the_eight_files_name_the_binary_and_the_hook` (`crates/teamwire-harness-claude/src/lead_setup/plugin.rs:305`) pins the matcher at `plugin.rs:342`. Its expected JSON loses the `"matcher"` key. Keep every other assert.

  T006 passes.
- [X] T008 [US1] Write failing tests in `crates/teamwire-cli/tests/status.rs`:
  - `received_with_a_message_id_clears_it_from_status_without_typing` asserts that after `teamwire received <team> m001`, `status` no longer lists `m001` under deliveries that need attention, and the fake host records no typing (today the command refuses "no member has this role").
  - `received_refuses_an_unknown_id_and_a_message_not_to_the_lead` asserts the two refusals from `specs/001-deckh-fixes/contracts/cli.md`.
  - `received_refuses_a_name_that_is_both_a_role_and_a_message_id`: with a role named `m1` and a message `m1`, the command refuses and names both.
  - `received_leaves_a_delivered_or_read_message_as_it_is`: a `handed_off` row stays `handed_off` and prints `already delivered`; an `acked` row prints `already read`.
- [X] T009 [US1] Implement message ids in `mark_received` in `crates/teamwire-report/src/received.rs`. An argument is a message id only when it matches `^m[0-9]+$` and no role of the team has that name. When a role has that name too, refuse it and name both. For a message id, call `ack_delivery(at, message, &Recipient::Lead)` inside the same `Store::write`, but only for a row in state `undelivered`, `unconfirmed` or `pending`: `ack_delivery` would also move a `handed_off` row (`crates/teamwire-store/src/delivery.rs:166-191`). Add new `Received` outcomes for read, already delivered, already read, unknown id, not a message to the lead, and both a role and an id. Parse the arguments and print the outcomes in `crates/teamwire-cli/src/cmd/received.rs`. In the same commit, update `docs/api/cli.md:593-603` (`received` takes ids), `README.md:258` (the `teamwire received` row also records a message read from its file) and `examples/skills/teamwire/SKILL.md:700-712` (use `received <id>` for a message read from its file). T008 passes.
- [X] T010 [US1] Write the failing test `an_answer_with_re_settles_the_questions_lead_row` in `crates/teamwire-msg/tests/answer.rs`, using the message rig in `crates/teamwire-msg/tests/support/rig.rs`. The test asserts that the question's `lead` row is `acked` after the lead's answer with `re` (red today, as in rca's F1b).
- [X] T011 [US1] In `store_message` (`crates/teamwire-msg/src/send.rs:297-321`), ack the delivery row `(re, <sender as recipient>)` with `ack_delivery` in the same write, when the new message is an `answer` with `re` set and that row exists. In the same commit, update `docs/api/cli.md` (the `send` section, `:524-590`: an answer settles its question) and `examples/skills/teamwire/reference.md:110-115` (the `QUESTION-UNDELIVERED` and `UNANSWERED` rows). T010 passes.
- [X] T012 [US1] Write the failing test `an_undelivered_question_line_names_its_file` in `crates/teamwire-watch/tests/poll.rs`. It asserts that the `QUESTION-UNDELIVERED` line contains `comms/m001.md`.
- [X] T013 [US1] Carry the comms path in `Question` (`crates/teamwire-watch/src/snapshot.rs:116-141`), and append ` File: <path>` in `question` (`crates/teamwire-watch/src/plan.rs:292-303`) for `QUESTION-UNDELIVERED` and `QUESTION-UNCONFIRMED`. In the same commit, update `docs/api/cli.md:218-223` (the watch line table), add the requirement rows for the lead's undelivered note and `received <id>` beside DEL-16 in `docs/requirements.md:120`, and finish `examples/skills/teamwire/reference.md:110-115` (teamwire does not retry; the hook lists the item; `received <id>` clears it).
  - Changed tests in `crates/teamwire-watch/tests/poll.rs`: `unconfirmed_question_announced_once` (l.840, assert at l.848), `an_undelivered_question_is_announced_once` (l.855, assert at l.863) and `an_undelivered_question_from_another_team_is_announced` (l.870, assert at l.879). Each asserts `line.ends_with("QUESTION-… {id} from …")`. Change each expected suffix to end with the new ` File: <team dir>/comms/<id>.md` text, still with `ends_with`.

  T012 passes.

### F2: the watch keeps a member whose session works

- [X] T014 [US1] Write tests in `crates/teamwire-watch/tests/poll.rs`:
  - Failing: `an_undelivered_result_of_a_working_member_does_not_complete_the_team` asserts no `TEAM-COMPLETE` while the fake session is `Working`. Today the watch prints `TEAM-COMPLETE 1/2 done; not done: impl undelivered`.
  - **Guard**: `an_undelivered_result_of_an_idle_member_still_settles`. It passes on the base.
  - Failing: `an_undelivered_result_with_no_visible_session_settles_and_says_so` asserts the line holds `session not visible`.

  Also write the failing tests `an_undelivered_member_that_works_reads_working` and `an_undelivered_member_at_a_dialog_reads_working_at_a_dialog` in `crates/teamwire-watch/tests/status.rs`, and the failing case `"working (at a dialog) is active"` in `ui/src/shared/state.test.ts` (today `classifyState` returns `unknown`). Copy the shape of rca's `rca_f2_f3.rs`, which is read only in `/Users/apoorvagarwal/.teamwire/scratch/dfix/rca/repo/crates/teamwire-watch/tests/`.
- [X] T015 [US1] Call `look` in the `Undelivered` and `Unconfirmed` arms of `judge_completion` in `crates/teamwire-watch/src/plan.rs:440-455`, after the one-time announcement:
  - `Alive(Working)` or `Alive(Blocked)` gives `Verdict::waiting()`.
  - `Idle` or died settles.
  - `Absent` or `NoInfo` settles and says `session not visible`.

  In `state_word` (`crates/teamwire-watch/src/status.rs:249-264`), for `Undelivered` or `Unconfirmed`, return `working` with `Seen::Alive(Activity::Working)` and `working (at a dialog)` with `Seen::Alive(Activity::Blocked)`. In `ui/src/shared/state.ts:37`, classify `working (at a dialog)` as `active`.
  - Changed tests, as a fixture change only: `poll.rs` `undelivered_result_printed_once` (l.153), `two_undelivered_same_second_two_lines` (l.178) and `unconfirmed_result_announced_once` (l.825), and `crates/teamwire-watch/tests/status.rs` `busy_team` (l.70-83). In those, set the fake session to `Idle` where they need a settled member. Never loosen an assertion.

  In the same commit, update:
  - `docs/requirements.md:177` (WAT-6) and `:55` (INV-12) if they are worded on "settled";
  - `docs/api/cli.md:228, 236` (`working (at a dialog)` among the status words);
  - `examples/skills/teamwire/reference.md:110-111, 117`;
  - `examples/skills/teamwire/SKILL.md:611-623, 666-668`.

  T014's failing tests pass, and its guard test still passes.

### F3: a waking message starts a generation; a handoff request holds messages

- [X] T016 [US1] Write failing tests for the `member.woken` event type:
  - `a_version_10_store_migrates_and_accepts_member_woken` in `crates/teamwire-store/tests/migrate_v11.rs`, which is new, modelled on `crates/teamwire-store/tests/migrate_v10.rs`. It builds a version 10 store from `SCHEMA_V1_SQL` to `SCHEMA_V10_SQL`, opens it with `Store`, asserts `SCHEMA_VERSION` 11 and every event row kept with its `seq`, and records a `member.woken` event. Today the open stays at 10 and the CHECK refuses the event.
  - In `ui/src/shared/teamwire.test.ts`, `"a member.woken event parses"`, and in `ui/src/web/model/timeline.test.ts`, `"a member.woken row is labelled woken"`.

  Add `MemberWoken => "member.woken"` to `EventType` in `crates/teamwire-core/src/event.rs`, beside `MemberReprompted` (l.36), as the stub. Add `EventType::MemberWoken` beside `EventType::MemberReprompted` in the match lists that must name every variant: `crates/teamwire-otel/src/observe.rs:145-155, 462-472` and `crates/teamwire-otel/src/spans.rs:75-90`. The store's unit test that checks the CHECK list against `EventType::ALL` (`crates/teamwire-store/src/lib.rs:312-316`) then fails too, until T017.
- [X] T017 [US1] Add schema version 11:
  - `crates/teamwire-store/src/schema/v11.sql`, which is new, rebuilds `events` with `member.woken` in the CHECK list, exactly as `crates/teamwire-store/src/schema/v10.sql` does, with the same header note on the rename and `sqlite_sequence`.
  - `SCHEMA_V11_SQL` and `SCHEMA_VERSION = 11` in `crates/teamwire-store/src/lib.rs:56, 97`; the step in `MIGRATIONS` in `crates/teamwire-store/src/store.rs:16, 46-47`; and the migration lists in `crates/teamwire-store/tests/migrate.rs:15, 578-579`.
  - Add `"member.woken"` to the zod `EventType` in `ui/src/shared/teamwire.ts:292-303`, and a `member.woken` case labelled `<role> woken`, with its generation, beside `member.reprompted` in `ui/src/web/model/timeline.ts:156-159`.

  In the same commit, update `docs/ARCHITECTURE.md:109` and `docs/api/core.md:709` (schema version 11), and add the `member.woken` row to the event table in `docs/metrics.md` (beside `:18`). T016 passes, with `cargo test -p teamwire-store --locked` and, from `ui/`, `bun test src/shared src/web/model` and `bun run typecheck`.
- [X] T018 [US1] Write tests in `crates/teamwire-msg/tests/woken.rs`:
  - Failing: `an_answer_to_a_member_with_a_done_result_starts_a_generation`: the generation is one higher.
  - Failing: `a_note_to_a_member_with_a_blocked_or_failed_result_starts_a_generation`: one case each.
  - Failing: `the_woken_generation_is_recorded_as_member_woken`: the newest event for the member is `member.woken`, and no `member.reprompted` was written.
  - **Guard**: `a_known_delivery_failure_undoes_the_woken_generation`, with a `no_place` failure. The generation is back to its old value, which already holds on the base, because the base never bumps. Its second assert, that the undo event is `member.woken` with `undone: true`, fails until T019.

  Also write:
  - Failing: `a_wake_opens_a_new_generation_span` in `crates/teamwire-otel/src/spans/tests.rs`, beside `a_handoff_opens_a_new_generation_span_with_the_new_session` (l.89). The `teamwire.member.generation` span for generation 2 starts at the `member.woken` event.
  - **Guard**: `a_woken_generation_is_not_counted_as_a_reprompt` in `crates/teamwire-otel/src/outcomes/tests.rs`: two `member.woken` events give `reprompts == 0`.
  - **Guard**: `a_member_woken_by_a_message_is_tracked` in `crates/teamwire-watch/tests/poll.rs`. The fix is in teamwire-msg, and the watch's `Bed` writes rows directly, so this test starts from the state the fix writes: the generation bumped, as `Bed::reprompt` does (`crates/teamwire-watch/tests/support/fixture.rs:551-557`). It asserts no `TEAM-COMPLETE` while the woken member works, and passes on the base.
- [X] T019 [US1] In `store_message` (`crates/teamwire-msg/src/send.rs:297-321`), for each member recipient of a message that is not a reprompt, check the newest completion. If it is in the member's current generation and its summary does not start with `handoff:`, wake the member and push the result into `bumped`. A known delivery failure then undoes the wake (REP-3).
  - In `crates/teamwire-store/src/member.rs`, add `wake_generation(at, role, message)` beside `bump_generation` (l.362-387), sharing one private helper, and writing `member.woken`. `bump_generation` keeps its signature. Give `restore_generation` (l.446-470) the event type to write for the undo.
  - In `crates/teamwire-msg/src/attempt.rs`, give `bump` (l.84-102) the choice of `bump_generation` or `wake_generation`, carry the event type in `Bumped`, and pass it on in `undo_bump` (l.350-362).
  - Take `member.woken` as a generation start wherever `member.reprompted` is one: the interrupted-reprompt check in `crates/teamwire-msg/src/attempt.rs:115-160`, `is_unprompted` in `crates/teamwire-watch/src/status.rs:378-389`, and the undo stack and `generation_start` in `crates/teamwire-otel/src/history.rs:25-40, 115-127`. Keep the reprompt counter (`crates/teamwire-otel/src/observe.rs:88-91`) and the outcome count (`crates/teamwire-otel/src/outcomes.rs:159-164`) on `member.reprompted` only.
  - Changed call sites, not expectations: the `restore_generation` calls in `crates/teamwire-store/src/member/tests.rs:230, 238, 270` gain the event type argument.
  - Tests that may change, if they pin "only a reprompt bumps": `crates/teamwire-msg/tests/reprompt.rs`, `crates/teamwire-msg/tests/stopped.rs` and `crates/teamwire-report/tests/turn_end.rs` `completion_after_reprompt_is_new_result`. analyze's rough fix broke none of them.

  In the same commit, update:
  - `docs/requirements.md:57` (INV-14), `:129` (REP-3) and `:179` (WAT-8);
  - `docs/failure-modes.md:41-42`;
  - `docs/api/cli.md:237` (status words);
  - `docs/metrics.md:102` (the generation span also starts at `member.woken`) and `:160-166` (a `member.woken` event starts a generation and is not a reprompt; its undo is not counted either);
  - `examples/skills/teamwire/SKILL.md:714-715`: any message to a finished member wakes it as a new generation.

  T018's failing tests pass, and its guard tests still pass.
- [X] T020 [US1] Write tests:
  - Failing, in `crates/teamwire-msg/tests/woken.rs`, `a_message_to_a_member_with_a_handoff_result_is_held_not_typed`: the row is `undelivered` with code `held_for_handoff`, and the fake host records no typing.
  - Failing, in the same file, `a_reprompt_names_the_held_messages_after_its_text`: the typed line holds `Held for you, oldest first: m001` after the reprompt body.
  - Failing, in `crates/teamwire-msg/tests/resend.rs`, `a_held_message_is_not_resent`: `resend` refuses and names `handoff`.
  - **Guard**, in `crates/teamwire-launch/tests/handoff.rs`, `a_handoff_first_line_counts_the_held_messages`: the first line holds `1 unread message(s)`. It passes on the base, because `UNREAD` includes `HandedOff` (`crates/teamwire-launch/src/handoff.rs:104-109`).
  - Failing, in `crates/teamwire-watch/tests/status.rs`, `a_held_message_is_listed_apart_from_stuck_deliveries`: an `undelivered` row with code `held_for_handoff` is in `held_deliveries` and not in `stuck_deliveries`.
  - Failing, at the bottom of `crates/teamwire-cli/src/cmd/status.rs`, the unit test `held_rows_print_on_their_own_line`: the text holds `held for handoff: m004 -> impl` outside the "deliveries that need attention" block.
  - Failing, in `ui/src/shared/teamwire.test.ts`, `"held_deliveries parse and are kept"`.

  So that the tests compile and fail on their assertions, add `HeldForHandoff => "held_for_handoff"` to `DeliveryErrorCode` in `crates/teamwire-core/src/message.rs:112-145`, and add `pub held_deliveries: Vec<HeldDelivery>` (with `display_id` and `to`), always empty, to `StatusView` in `crates/teamwire-watch/src/lib.rs:108-115, 190`.
- [X] T021 [US1] Implement the hold:
  - In `crates/teamwire-msg/src/send.rs` and `crates/teamwire-msg/src/attempt.rs`, when the newest completion in the current generation has a `handoff:` summary, store the row `undelivered` with `held_for_handoff`. Type nothing, print `<role>: held for handoff (<id>)`, and exit 0.
  - In `crates/teamwire-deliver/src/lines.rs:59-110`, append the sentence `Held for you, oldest first: <id> (<type> from <sender>), …. Read them with teamwire inbox.` to a reprompt's line when the member has held rows.
  - Refuse a held row in `crates/teamwire-msg/src/resend.rs:130-143`, naming `handoff` and `reprompt`.
  - In `status_with` (`crates/teamwire-watch/src/status.rs:137-158`), put a row with code `held_for_handoff` into `held_deliveries` and not into `stuck_deliveries`. Print each as `held for handoff: <id> -> <role>` in `crates/teamwire-cli/src/cmd/status.rs:184-190`, outside the "deliveries that need attention" block.
  - Add `held_deliveries` to the zod `StatusView` in `ui/src/shared/teamwire.ts`. `ui/src/web/NeedsLook.tsx` reads only `stuck_deliveries` (l.16, 75-89), as does `ui/src/web/model/teams.ts:93, 110`, so neither shows a held row and neither changes.

  In the same commit, update:
  - `crates/teamwire-launch/contracts/talking_head.md:12-16`: a held message is not resent. If a contract test in `crates/teamwire-launch/src/contract/tests.rs` or `tests_findings.rs` pins the sentence that changes, change its expected text and name it in your report.
  - `crates/teamwire-cli/src/cmd/handoff_guide.md`.
  - `examples/skills/teamwire/SKILL.md:841-847`: act on `handoff:`, where messages wait for the handoff.
  - The "Context handoff" section of `examples/skills/teamwire/reference.md`.
  - `docs/api/cli.md:524-590`: `send` prints `held for handoff` with exit 0, and `resend` refuses. Also the status section (`:236-244`): the held line.
  - `docs/api/core.md:108-111`: `HeldForHandoff` in the `DeliveryErrorCode` list.

  T020's failing tests pass. The handoff guard test passes with no handoff code change, because `crates/teamwire-launch/src/handoff.rs:86-109` already counts `undelivered` rows as unread. Do not edit that file unless the test shows otherwise.

**Checkpoint**: US1 works on its own. The lead can run the deckh sequence on the fakes without a dummy reprompt or a second typing.

---

## Phase 4: User Story 2 - The lead sees which tasks can run in parallel and which files each member needs (Priority: P2)

**Goal**: `teamwire speckit draft <tasks.md> [--repo <dir>]` prints:

- the files that phases share, and the files that stories share;
- the tasks that change no file, and the tasks whose files no other task
  names;
- one DRAFT `owns` list per story, plus one for setup and polish, with the
  mentioning files and the reason for each.

**Independent Test**: `cargo test -p teamwire-cli --locked --test speckit_draft`, `cargo test -p teamwire-cli --locked --bin teamwire speckit` and `cargo test -p teamwire-verify --locked`. Then run `cargo run -p teamwire-cli --locked -- speckit draft specs/001-deckh-fixes/tasks.md` and check that it prints a phase table and DRAFT lists.

### ADR and command stub for US2

- [X] T022 [P] [US2] Write `docs/adr/0044-speckit-draft-reads-tasks-md.md`. It supersedes ADR 0042 decision 1 (`docs/adr/0042-spec-kit-after-the-review.md:19-27, 82-87`) in part: the reader exists now. Record the parsing rule (with the `:N-M` suffix, continuation line and glob rules), the mention rule (exact path, dotted module suffix with at least two parts, full file name with its extension, CI `paths:` glob; never a bare stem), the excluded feature folder, exit 1 for a missing or empty `tasks.md`, and that the lists are drafts for the lead. Do not edit ADR 0042.
- [X] T023 [US2] Add the `speckit` command group with a `draft` subcommand (`<tasks.md>`, `--repo <dir>`) to `crates/teamwire-cli/src/args.rs:35-124`, following the `Afk` group pattern. Dispatch it in `crates/teamwire-cli/src/main.rs:13-48, 84-115`. Add `crates/teamwire-cli/src/cmd/speckit.rs` as a stub that exits 0 and prints nothing, with empty sub-files `crates/teamwire-cli/src/cmd/speckit/parse.rs`, `crates/teamwire-cli/src/cmd/speckit/overlap.rs`, `crates/teamwire-cli/src/cmd/speckit/mentions.rs` and `crates/teamwire-cli/src/cmd/speckit/render.rs` attached with `#[path]`, as `crates/teamwire-cli/src/cmd/doctor_speckit.rs:19-25` does. This task exists so that T024's tests fail on assertions, not on a missing command.

### Tests for US2 (write first, see them fail)

- [X] T024 [US2] Write the integration tests in `crates/teamwire-cli/tests/speckit_draft.rs`, copying the `git`, `teamwire` and temporary-repo helpers' shape from `crates/teamwire-cli/tests/preset.rs:45-53, 238-246`. The tests are listed in `specs/001-deckh-fixes/research.md` under US2:
  - `phases_that_name_one_file_are_listed_as_sharing_it`
  - `stories_that_share_a_file_are_named`
  - `a_task_with_no_path_is_listed_as_changing_no_file`
  - `a_task_whose_files_no_other_task_names_is_listed_as_independent`
  - `a_ci_paths_glob_that_matches_a_task_path_adds_the_workflow`
  - `a_file_naming_the_dotted_module_is_added`
  - `a_test_naming_the_full_file_name_is_added`
  - **Guard**: `a_file_naming_only_the_bare_stem_is_not_added` (passes on T023's stub)
  - **Guard**: `the_feature_folder_is_not_a_mention` (passes on T023's stub)
  - `an_unreadable_workflow_is_named_not_skipped`
  - `a_missing_or_empty_tasks_file_exits_one`: exit 1 and a message naming the file.
  - `the_drafts_are_labelled_as_drafts`

  The fixture repository holds a `tasks.md`, a `.github/workflows/replay.yml` with `on.push.paths`, an `.importlinter` naming `harness.tools`, a test naming `tools.py`, and a file naming only `tools`. Its `tasks.md` uses this feature's own line shapes: a path with a `:12-30` suffix, a task with paths on indented lines, and a `*` token. Each guard test shares its fixture with a positive case, so the run proves the command found the other mentions.
- [X] T025 [P] [US2] Make `Owns` and its matching method `pub` in `crates/teamwire-verify/src/owns.rs:9-30`, and export it from `crates/teamwire-verify/src/lib.rs:26-35`. Keep `literal_separator(true)`. This is a visibility change only, with no new behaviour. T024's `a_ci_paths_glob_that_matches_a_task_path_adds_the_workflow` covers the matching through the command.

### Implementation for US2

- [X] T026 [US2] Implement the parser in `crates/teamwire-cli/src/cmd/speckit/parse.rs`. It reads `## Phase <n>: <title>` headings and the task lines `- [ ]`, `- [x]` and `- [X]` `T<digits>`, with an optional `[P]` and an optional `[US<n>]`.
  - A path is a token in backticks, or a bare token, that holds a `/` or ends in `.<ext>`, with trailing punctuation removed and URLs left out.
  - Strip a `:N` or `:N-M` suffix from a path.
  - An indented line under a task line belongs to that task, until the next task line, heading or blank line.
  - Keep a token that holds `*` as a glob.

  Unit tests go at the bottom of the file, written first against a stub that returns no tasks: `a_line_suffix_is_stripped`, `an_indented_line_belongs_to_the_task_above` and `a_star_token_is_a_glob` fail there, each with lines shaped like this feature's own `tasks.md`. A negative case, such as the rule that leaves out URLs, passes on that stub: mark it **guard** in its comment.
- [X] T027 [US2] Implement the overlap in `crates/teamwire-cli/src/cmd/speckit/overlap.rs`: the shared paths per phase pair and per story pair, the tasks with no path, and the tasks whose paths no other task names. Before the overlap runs, `crates/teamwire-cli/src/cmd/speckit.rs` expands each glob path against the tracked files that T028 lists, with `teamwire_verify::Owns`. A glob that matches no tracked file stays as itself. Unit tests go at the bottom of the file, written first.
- [X] T028 [US2] Implement mentions in `crates/teamwire-cli/src/cmd/speckit/mentions.rs`:
  - List the files with `git ls-files -z`, run through the Platform runner as `crates/teamwire-cli/src/cmd/doctor_speckit.rs:78-100` does. Leave out the feature folder that holds the `tasks.md`.
  - Match the exact path, any dotted module suffix with at least two parts, and the full file name as a whole token.
  - Read `.github/workflows/*.yml` and `*.yaml` with `parse_yaml` (`crates/teamwire-cli/src/cmd/doctor_speckit/yaml.rs:118`), and match each `on.<event>.paths` entry with `teamwire_verify::Owns`.
  - Report each workflow the reader refuses.
  - Never match a bare stem.
  - Unit tests go at the bottom of the file, written first.
- [X] T029 [US2] Implement the output in `crates/teamwire-cli/src/cmd/speckit/render.rs` and the run in `crates/teamwire-cli/src/cmd/speckit.rs`, using the layout in `specs/001-deckh-fixes/contracts/cli.md`. Print the header, the phase pairs, the line naming the stories that share files, the tasks with no file, the independent tasks, and the unread workflows. Then print `DRAFT owns lists. Check each one before you launch: these are drafts, not final lists.`, followed by one list per story plus setup and polish, each path with its reason. A missing or unreadable file, or a file with no task lines, exits 1 with a named error. Exit 3 stays for usage errors (`docs/api/cli.md:14-20`). `--repo` defaults to the git top level of the folder that holds `tasks.md`. Keep each new file under 400 lines. T024's tests pass, and its guard tests still pass.
- [X] T030 [US2] Update the guidance in one commit:
  - `examples/skills/teamwire/SKILL.md:120, 126, 172-190`: for a spec-kit feature, run `teamwire speckit draft`, check its drafts, and keep the manual grep for the names a draft cannot see.
  - `examples/skills/teamwire/reference.md:301-309, 557, 579-596`: split per story from the command's table, and show the table and the recommended split to the user before launch.
  - `examples/presets/speckit.json` lines 10 (BUILD), 13 (SPLIT ONLY ON DISJOINT FILES) and 63 (the implementer `_note`).
  - `examples/skills/afk/SKILL.md`: the passage that ADR 0042's consequences name.
  - `docs/api/cli.md`: a new `### teamwire speckit draft <tasks.md>` section near `### teamwire preset` (`:725`), with exit 1 for a missing or empty `tasks.md`.
  - `docs/ARCHITECTURE.md`, if it lists the CLI's command groups.

  Then run `cargo test -p teamwire-cli --locked --test preset`, because it checks the skill's command names against the binary.

**Checkpoint**: US2 works on its own.

---

## Phase 5: User Story 3 - Small fixes to guidance and status output (Priority: P3)

**Goal**: The F6 checklist rule, the F8 closer gate slots in the preset and
the seat text, the F10 current tool in `status --role` on every harness, and
the F11 symlinks in the close dry run.

**Independent Test**: Run `cargo test -p teamwire-cli --locked --test preset`, `cargo test -p teamwire-cli --locked --bin teamwire status`, `cargo test -p teamwire-launch --locked --lib contract`, `cargo test -p teamwire-close --locked --test ignored_files`, `cargo test -p teamwire-watch --locked --test status`, `cargo test -p teamwire-harness-claude -p teamwire-harness-codex -p teamwire-harness-pi --locked`, and, from `ui/`, `bun test src/shared` and `bun run typecheck`.

### F6: read the checklist before the implementer

- [X] T031 [US3] Write the failing test `the_speckit_preset_has_the_lead_read_the_checklist_before_the_implementer` in `crates/teamwire-cli/tests/preset.rs`. It asserts that the speckit preset's rules hold `checklists/requirements.md`, the phrase `before the implementer launches`, and `waive`.
- [X] T032 [US3] Rewrite the rule at `examples/presets/speckit.json:11` and the Checklists passage at `examples/skills/teamwire/reference.md:505-522`, in one commit:
  - `specify` can leave items unticked, with the reason in the checklist's Notes, after three rounds or on purpose.
  - Before the implementer launches, the lead reads `checklists/requirements.md`.
  - For each unticked item, the lead either waives it in the implementer's brief (quote the item and say proceed) or reprompts the spec member to fix it.

  T031 passes.

### F8: the closer's local gate and CI gates

- [X] T033 [US3] Write failing tests:
  - `the_closer_task_has_a_local_gate_and_a_ci_gate_slot_and_reports_both` in `crates/teamwire-cli/tests/preset.rs`. It asserts that the closer task holds `Local gate: CHANGEME` and `Gates left to CI: CHANGEME`, and an instruction to report which gate ran with its exit code and which gates were left to CI. Keep `the_closer_task_lists_setups_and_has_a_restore_fallback` (`preset.rs:1520-1531`) as it is.
  - `a_closer_runs_the_local_gate_and_reports_the_gates_left_to_ci` in `crates/teamwire-launch/src/contract/tests_findings.rs`. It asserts that the rendered closer seat holds `local gate` and `left to CI`, and no longer holds `Run the full gate your brief names`.
- [X] T034 [US3] Replace the one full-gate slot with the two slots in `examples/presets/closer.json:13, 28`. The closer runs the local gate once on each head that ships, runs none of the CI gates, and reports both. When the user allows the full gate locally, the lead writes the full gate in the local slot and `none` in the CI slot. Say the same in the closer's seat text, `crates/teamwire-launch/contracts/kind_closer.md:11-19`: run the local gate your brief names once on each head that ships, run none of the gates the brief leaves to CI, and report both. Keep the phrases of that block that still hold.
  - Test that may change: `closer_and_reviewer_carry_the_checks_that_catch_a_test_that_cannot_fail` (`crates/teamwire-launch/src/contract/tests.rs:116-130`) pins `joined head when one exists`, `do not also\n  run member checks on the joined head` and `run only its failing tests on the base commit`. Keep each phrase that still holds. Where one must change, change its expected text, and keep every assert.

  In the same commit, update `examples/skills/teamwire/SKILL.md:280, 864-880` ("Fill in the preset's full gate" becomes the two slots). T033 passes.

### F10: the current tool call in `status --role`

- [X] T035 [US3] Add `crates/teamwire-core/src/tool_run.rs`, holding `pub struct ToolRun { pub name: String, pub started_at: Timestamp }`, and export it from `crates/teamwire-core/src/lib.rs`. Add `fn current_tool(&self, session: &SessionId, cwd: Option<&Path>) -> Result<Option<ToolRun>, HarnessError> { Ok(None) }` to the `Harness` trait in `crates/teamwire-core/src/harness.rs`, beside `context_fill` (l.112). This is the stub the tests below need. In the same commit, add `current_tool` beside `context_fill` in the trait listing in `docs/api/core.md:290-298`.
- [X] T036 [P] [US3] Write the failing test `the_current_tool_is_the_newest_tool_use_without_a_result` in `crates/teamwire-harness-claude/tests/sessions.rs`. Its fixture, `crates/teamwire-harness-claude/tests/fixtures/transcript-tool.jsonl`, copies real assistant `tool_use` and user `tool_result` lines from a local Claude Code transcript, with content trimmed and paths made generic. The test asserts `Bash` with the fixture's start timestamp, and `None` once the result line follows.
- [X] T037 [P] [US3] Write the failing test `the_current_tool_is_the_newest_call_without_output` in `crates/teamwire-harness-codex/tests/session.rs`, with the fixture `crates/teamwire-harness-codex/tests/fixtures/rollout-tool.jsonl` in the `response_item` / `function_call` / `function_call_output` shape. First confirm that shape against Codex's published rollout format or a real rollout file, and record which one you used in your report. research.md marks the shape INFERRED. When neither confirms it, build the fixture from research.md's shape and say so in your report.
- [X] T038 [P] [US3] Write two failing tests. `the_current_tool_comes_from_the_state_file`, in `crates/teamwire-harness-pi/tests/session.rs`, uses a `<id>.teamwire.json` state file with `tool: {name, started_at_ms}`. `the_member_extension_stamps_and_clears_the_tool`, in `crates/teamwire-harness-pi/tests/launch.rs` (or the extension's snapshot test), asserts that the generated member extension registers `tool_call` and writes `tool`.
- [X] T039 [US3] Implement `current_tool` for Claude Code in `crates/teamwire-harness-claude/src/tool.rs`, which is new. Reuse `find_transcript` and `tail_lines` from `crates/teamwire-harness-claude/src/fill.rs:76-118`, and wire the method into the `Harness` impl in `crates/teamwire-harness-claude/src/lib.rs`. T036 passes.
- [X] T040 [US3] Implement `current_tool` for Codex in `crates/teamwire-harness-codex/src/tool.rs`, which is new. Reuse `rollout_path` and `tail_text` from `crates/teamwire-harness-codex/src/session.rs:60, 262`, and wire the method into the `Harness` impl in `crates/teamwire-harness-codex/src/lib.rs`. Return `None` for any entry the parser cannot read. If T037 found no published format or real rollout to confirm the shape, mark the parser with a comment that starts `shortcut:` and names the limit (the rollout shape is unconfirmed), and list it in your report. T037 passes.
- [X] T041 [US3] Implement the pi side:
  - The member extension writes `tool: {name, started_at_ms}` on `tool_call` and clears it on `tool_result`. Use the state writer in `crates/teamwire-harness-pi/src/extension.rs:102-116, 179-200, 277`.
  - `current_tool` reads that field in `crates/teamwire-harness-pi/src/session.rs:11-17`, wired into the `Harness` impl in `crates/teamwire-harness-pi/src/lib.rs`.

  T038 passes.
- [X] T042 [US3] Write failing tests:
  - `status_role_shows_the_current_tool_and_its_duration` in `crates/teamwire-watch/tests/status.rs`. Extend the watch's fixture harness `FillHarness` (`crates/teamwire-watch/tests/support/fixture.rs:28-84`) to forward `current_tool` and answer it from a map the test sets, as it does for `context_fill`. With a `ToolRun` 900 s old, the view's `current_tool.seconds == 900`.
  - At the bottom of `crates/teamwire-cli/src/cmd/status.rs`, the unit test `status_role_prints_how_long_the_tool_has_run`: the text for a built view with `current_tool.seconds == 902` holds `tool: Bash, running 15m02s`, and a working member with no `current_tool` gets `tool: unknown (this harness does not report it)`. The CLI's integration tests run the real binary, which has no fake harness, so the text gets a unit test.
  - In `ui/src/shared/teamwire.test.ts`, `"current_tool parses and is kept"`.

  So that the tests compile, add `current_tool: Option<ToolView>` (with `name`, `started_at` and `seconds`), always `None`, to `MemberView` in `crates/teamwire-watch/src/lib.rs:57-81`. No file under `tests/fakes/` changes.
- [X] T043 [US3] Implement the watch and CLI side:
  - Add `crates/teamwire-watch/src/tool.rs`, modelled on `crates/teamwire-watch/src/context.rs:13-26`.
  - Fill `current_tool` in `status_with` (`crates/teamwire-watch/src/status.rs:307-357`) only for `--role` and a working member. `seconds` comes from `Clock`.
  - Print `tool: <name>, running <duration>`, or `tool: unknown (this harness does not report it)`, in `crates/teamwire-cli/src/cmd/status.rs:245-269`.

  In the same commit, add `current_tool: ToolView.nullish()` to the zod `MemberView` in `ui/src/shared/teamwire.ts:71-95` and run the checks that `ui/AGENTS.md` names. Also update `docs/api/cli.md:236-244` (status `--role`), and the passage in `examples/skills/teamwire/SKILL.md` that tells the lead to read `status --role` for a member that looks stuck. T042 passes.

### F11: symlinks in the close dry run

- [X] T044 [P] [US3] Write two failing tests in `crates/teamwire-close/tests/ignored_files.rs`. `a_symlinked_ignored_folder_is_named_as_a_symlink_with_its_target` asserts `.venv -> <target>` under `removes symlinks (targets kept)`, and that `.venv` is not under `deletes`. `a_broken_symlink_is_marked_broken` asserts `(broken)`. Keep the existing tests in that file (`:36-114`) as they are.
- [X] T045 [US3] In `ignored_warning` (`crates/teamwire-close/src/member_close.rs:59-90`), check each path with `std::fs::symlink_metadata`. List the symlinks apart from the deleted files, each with its target from `std::fs::read_link`, and mark a broken link `(broken)`. Leave out the "deletes" part when every path is a link. The ten-path limit applies to both lists together. In the same commit, update `docs/api/cli.md:507`. T044 passes.

**Checkpoint**: Each of F6, F8, F10 and F11 passes on its own.

---

## Phase 6: User Story 4 - A waiver for a file holds until that file changes (Priority: P4)

**Goal**: A waiver for a flag that names one file counts at a later HEAD
while that file's content is the same as at the waived HEAD. Any change to
the file, a merge or rebase included, ends it. A waiver for a flag that
names no file, or for `uncommitted:<path>`, ends at the next commit, as
today.

**Independent Test**: Run `cargo test -p teamwire-verify --locked --test verify`. The tests listed in research.md under F12 pass. The two marked "fails on `develop`" fail with the fix reverted.

### ADR for US4

- [ ] T046 [US4] Write `docs/adr/0045-a-file-waiver-holds-until-the-file-changes.md`. Record the F12 decision from `specs/001-deckh-fixes/research.md`. It supersedes ADR 0009 point 2 (`docs/adr/0009-verify-verdicts.md:30-33`) only for the flags that name one file: `test-deleted`, `config-changed`, `outside-owns`, `weakened`, `lint-removed`, `test-removed` and `asserts-removed`. State that a flag naming no file, `uncommitted:<path>` and `head-moved` keep the old rule, and that VER-5's record-at-HEAD rule for removal stays. Do not edit ADR 0009.

### Tests for US4 (write first)

- [ ] T047 [US4] Write the two failing tests in `crates/teamwire-verify/tests/verify.rs`, beside `the_spec_kit_constitution_is_check_config` (`verify.rs:382`), with the existing helpers (`repo_on_work`, `implementer`, `report_all_commits`, `run_one`, `flag_ids`, `waive`):
  - `a_file_flag_waiver_holds_while_that_file_is_unchanged`: the deckh replay. A member with `owns = ["src/"]` changes `.specify/memory/constitution.md`. The lead waives `config-changed:.specify/memory/constitution.md` and `outside-owns:.specify/memory/constitution.md`. The member commits `src/b.rs`. Assert `verdict == Verdict::Pass` and both flags `waived`.
  - `a_waiver_for_a_deleted_file_holds_while_the_file_stays_deleted`: with `deleted_test()` (`verify.rs:529-540`), waive `test-deleted:tests/a.rs`, commit `src/more.rs`, assert the flag is `waived`.

  Run `cargo test -p teamwire-verify --locked --test verify a_file_flag_waiver_holds a_waiver_for_a_deleted_file` and see both fail on the verdict or `waived` assertion. These tests need no new name.
- [ ] T048 [US4] Write five **guard** tests in `crates/teamwire-verify/tests/verify.rs` that keep the rest of the rule. They pass on `develop` and must still pass after T049:
  - `a_file_flag_waiver_ends_when_that_file_changes`: after the waivers of T047's first test, a commit changes the constitution again. The flags are not waived and the verdict is `Verdict::Flagged`.
  - `a_file_flag_waiver_ends_when_a_merge_changes_that_file`: a side branch made with `repo.git(&["checkout", "-b", ...])` changes the constitution, and the member merges it with `--no-ff`. The flags are not waived.
  - `a_waiver_at_a_commit_git_does_not_know_does_not_carry_over`: insert a waiver row with `tx.insert_waiver` and `head_sha` of forty `0`s for the constitution flag. The flag is not waived.
  - `a_waiver_for_a_flag_with_no_file_ends_at_the_next_commit`: an implementer with an empty `owns` raises `no-owns`. Waive it, commit, and assert `no-owns` is not waived.
  - `an_uncommitted_path_waiver_ends_at_the_next_commit`: waive `uncommitted:notes.txt`, commit another file, and assert the flag is not waived.

### Implementation for US4

- [ ] T049 [US4] Implement the rule, in one commit with its guidance:
  - In `crates/teamwire-verify/src/inspect.rs:11-23`, add `path: Option<String>` to `RawFlag`, keep `RawFlag::new` with `path: None`, and add `RawFlag::about(path, id, detail)`. Use `about` at the file-flag sites: `inspect.rs:45` (`test-deleted`), `:51` (`config-changed`), `:57` (`outside-owns`), `:120` and `:177` (`weakened`), `:192` (`lint-removed`, the old path), `:208` (`test-removed`) and `:223` (`asserts-removed`). Leave `bad-owns` at `:35`.
  - In `crates/teamwire-verify/src/git.rs`, beside `file_at` (`git.rs:161-164`), add `Git::blob(rev, path) -> Result<Option<String>, VerifyError>` that runs `git ls-tree -z --full-tree <rev> -- <path>`: `Ok(Some(id))` for the blob id, `Ok(None)` for empty output, `Err` when git does not know `rev`.
  - Add `crates/teamwire-verify/src/waivers.rs` with one function that returns the waived flag ids for a member at HEAD. Read the member's rows with `tx.all_waivers()` filtered by role (no store change). A flag is waived when a waiver for its id is at HEAD, or when it has a path and a waiver for its id at an older HEAD `w` has `blob(w, path) == blob(HEAD, path)`, both `Ok`. Declare the module in `crates/teamwire-verify/src/lib.rs:26-32`.
  - In `crates/teamwire-verify/src/lib.rs:310-317`, call that function in place of the `tx.waivers(&member.role, head)` read. Rewrite the crate note at `lib.rs:1-6` ("waivers bound to a HEAD", "Never: ... count a waiver for a HEAD other than the one it was recorded at") to the new rule.
  - Changed test: in `crates/teamwire-verify/tests/verify.rs`, change the file note at `:1-2` and the last block of `a_waiver_makes_the_flagged_pass_a_pass_for_that_head_only` (`verify.rs:627-632`): the comment names ADR 0045, and after the commit to `src/more.rs` it expects `Verdict::Pass` and the flag `waived`. Keep every assert.

  In the same commit, update:
  - `examples/skills/teamwire/reference.md:204-208` ("Verify flags"): replace "A later commit needs a new waiver" with the rule: a waiver for a flag that names one file holds until that file's content changes, a merge or rebase included; any other waiver, `uncommitted:<path>` included, ends at the next commit.
  - `examples/skills/teamwire/SKILL.md:790-791`: a finished implementer gets a waiver per flag; a file flag's waiver holds until that file changes, and any other flag needs one at its final HEAD.
  - `docs/api/cli.md:432-433`, the `--waive` sentence of `teamwire verify`.
  - `docs/requirements.md:240` (VER-5): add the carry-over rule, and cite ADR 0045 and the T047 and T048 tests.
  - `docs/failure-modes.md:63`: a file flag's waiver carries over only while that file is unchanged, and verify must still run at the new HEAD.

  `teamwire verify` has no `--json` output, and `Flag` keeps its fields, so `ui/src/shared/teamwire.ts` does not change. Run `cargo test -p teamwire-verify --locked` and `cargo test -p teamwire-close --locked` (close reads verify records).

**Checkpoint**: The deckh replay passes after one waiver per flag.

---

## Phase 7: Polish & cross-cutting

- [ ] T050 Check the size of every new or changed source file in `crates/` against tidy's rule (`xtask/src/sizes.rs`). Split a source file only when tidy fails, at 800 lines. Every new file stays under 400 lines: `crates/teamwire-cli/src/cmd/speckit.rs`, `crates/teamwire-cli/src/cmd/speckit/*.rs`, `crates/teamwire-verify/src/waivers.rs`, `crates/teamwire-watch/src/tool.rs`, `crates/teamwire-harness-claude/src/tool.rs`, `crates/teamwire-harness-codex/src/tool.rs` and `crates/teamwire-core/src/tool_run.rs`. Files already over 400 lines on the base are not split, unless tidy fails on them: `crates/teamwire-watch/src/plan.rs` (571), `crates/teamwire-watch/src/status.rs` (436), `crates/teamwire-msg/src/send.rs` (422), `crates/teamwire-cli/src/args.rs` (583), `crates/teamwire-core/src/harness.rs` (642), `crates/teamwire-store/src/member.rs` (561), `crates/teamwire-store/src/store.rs` (692), `crates/teamwire-otel/src/observe.rs` (536), `crates/teamwire-otel/src/spans.rs` (432), `crates/teamwire-harness-claude/src/lead_setup/plugin.rs` (449) and `crates/teamwire-verify/src/lib.rs` (440). Record each changed file's line count in your report.
- [ ] T051 Run `scripts/gate.sh fast` and `scripts/test-changed.sh` from the repository root, and record both exit codes in your report (changes no file).
- [ ] T052 Run `teamwire speckit draft specs/001-deckh-fixes/tasks.md` on this feature, and record its phase table in your report for the lead (changes no file).

SC-001, SC-003 and SC-005 are checked by the closer, read-only, and need no
implementer task: the closer replays deckh's sequence on the fakes and runs
`teamwire speckit draft` on deckh's `tasks.md` in knit-3.0
(`specs/001-deckh-fixes/quickstart.md`).

---

## Dependencies & Execution Order

### Phase dependencies

- Setup (T001) and Foundational (T002) come first. They change no file.
- US1 (Phase 3), US2 (Phase 4), US3 (Phase 5) and US4 (Phase 6) depend on
  no other story's code. They share the files listed below.
- Polish (Phase 7) comes after the stories that are built.
- One implementer runs every phase in order, so the shared files need no
  merge order.

### Within each story

- Each test task comes before its fix task, as numbered.
- US1 runs in order: F1 (T004-T013), then F2 (T014-T015), then F3
  (T016-T021). F1, F2 and F3 all change
  `crates/teamwire-watch/tests/poll.rs`. F2 and F3 both change
  `crates/teamwire-watch/src/plan.rs`, `crates/teamwire-watch/src/status.rs`
  and `crates/teamwire-msg/src/send.rs` (with F1-c). T005 moves
  `result_path` before T013 changes `plan.rs`.
- In F3, T016 and T017 (the event type and schema version 11) come before
  T018 and T019, which write `member.woken`. T020 and T021 come last.
- In US2, T023 comes before T024. T025 can run beside T023 and T024.
  T026-T029 come after T024 and T025.
- In US3, F6, F8 and F11 have no dependencies. F10's T035 comes before
  T036-T043.
- In US4, T046 comes first. T047 and T048 come before T049. T049 edits
  `crates/teamwire-verify/src/lib.rs` after US2's T025 has exported `Owns`
  there.

### Files shared between stories (the lead measures these for a split)

| File | Tasks |
|---|---|
| `docs/api/cli.md` | US1: T009, T011, T013, T015, T019, T021. US2: T030. US3: T043, T045. US4: T049 |
| `examples/skills/teamwire/SKILL.md` | US1: T009, T015, T019, T021. US2: T030. US3: T034, T043. US4: T049 |
| `examples/skills/teamwire/reference.md` | US1: T011, T013, T015, T021. US2: T030. US3: T032. US4: T049 |
| `examples/presets/speckit.json` | US2: T030. US3: T032 |
| `crates/teamwire-cli/tests/preset.rs` | US3: T031, T033 (US2's T030 runs it, but does not edit it) |
| `crates/teamwire-launch/src/contract/tests_findings.rs` | US1: T006, T021. US3: T033 |
| `crates/teamwire-watch/src/status.rs` | US1: T015, T019, T021. US3: T043 |
| `crates/teamwire-watch/src/lib.rs` | US1: T020. US3: T042 |
| `crates/teamwire-watch/tests/status.rs` | US1: T014, T015, T020. US3: T042 |
| `crates/teamwire-cli/src/cmd/status.rs` | US1: T020, T021. US3: T042, T043 |
| `crates/teamwire-cli/tests/status.rs` | US1: T008 |
| `ui/src/shared/state.ts`, `ui/src/web/model/timeline.ts` | US1 only (T014-T017), so not shared; listed for the split notes |
| `ui/src/shared/teamwire.ts` | US1: T017, T021. US3: T043 |
| `ui/src/shared/teamwire.test.ts` | US1: T016, T020. US3: T042 |
| `docs/api/core.md` | US1: T017, T021. US3: T035 |
| `docs/requirements.md` | US1: T013, T015, T019. US4: T049 |
| `docs/failure-modes.md` | US1: T019. US4: T049 |
| `crates/teamwire-verify/src/lib.rs` | US2: T025. US4: T049 |

### Tasks that change no file

T001, T002, T051 and T052.

### Parallel opportunities

These sets of tasks touch disjoint files:

- T022 (the US2 ADR), T025 (the `Owns` visibility change), T044 (the F11
  test), T036, T037 and T038 (the harness tests, after T035).
- Within US3, F6 (T031-T032), F8 (T033-T034) and F11 (T044-T045) touch no
  file of F10 except `docs/api/cli.md` and `SKILL.md`.
- US2's code (`crates/teamwire-cli/src/cmd/speckit*`,
  `crates/teamwire-verify/`) touches no file of US1 or US3. Only its
  guidance task, T030, shares files.
- US4's code is all in `crates/teamwire-verify/`. Its only code file shared
  with another story is `crates/teamwire-verify/src/lib.rs` (US2's T025).
  T046, the US4 ADR, touches no other task's file.

---

## Parallel Example: User Story 3

```bash
# After T035 adds the trait method and ToolRun:
Task: "T036 Claude test in crates/teamwire-harness-claude/tests/sessions.rs"
Task: "T037 Codex test in crates/teamwire-harness-codex/tests/session.rs"
Task: "T038 pi tests in crates/teamwire-harness-pi/tests/session.rs"
Task: "T044 F11 test in crates/teamwire-close/tests/ignored_files.rs"
```

## Implementation Strategy

### MVP first (User Story 1 only)

1. Do Phase 1 and Phase 2. They change no file.
2. Do Phase 3 (US1) in order: F1, then F2, then F3.
3. Stop and check: replay the deckh sequence on the fakes (quickstart.md, US1).

### Incremental delivery

1. US1 gives the lead its notices, and the watch stays on.
2. US2 gives the split table and the `owns` drafts for the next spec-kit
   team.
3. US3's four small fixes can ship in any order.
4. US4 ends the repeated waivers for a file that did not change.

### Split for parallel implementers

The lead and the user decided on one implementer that runs every phase in
order. The notes below stay for a later team. Each story's code is mostly
disjoint from the others' code. The shared code files are
`crates/teamwire-verify/src/lib.rs` (US2 and US4) and the status files
(`crates/teamwire-watch/src/{status.rs,lib.rs}`,
`crates/teamwire-cli/src/cmd/status.rs`, `ui/src/shared/teamwire.ts`; US1
and US3). The rest of the shared files are guidance text and tests, listed
above. A split per story needs one owner for each shared file, or a merge
order. Any merge conflict in a guidance file is a text conflict.
