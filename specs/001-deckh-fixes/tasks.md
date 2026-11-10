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

**Guidance**: Each fix task names the skill, preset, reference, doc and
`ui/src/shared/teamwire.ts` text it changes. That text goes in the **same
commit** as the code (constitution principle III).

**Root causes, the decision for each fix, and the line numbers on
`develop`** are in `specs/001-deckh-fixes/research.md`. Read the section
for your finding before its first task.

**Protected paths** (AGENTS.md): `tests/fakes/`, `scripts/`, `Cargo.toml`
and `Cargo.lock` are protected. Ask the lead before editing any of them.
The lead granted one exception: T040 may add to `tests/fakes/src/harness.rs`
the one `FakeHarness` setter that F10's test needs, and nothing else.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: The task can run in parallel. It touches different files and
  depends on no task that is still open.
- **[Story]**: US1, US2, US3 or US4.

---

## Phase 1: Setup

**Purpose**: Confirm the base and the commands before changing code.

- [ ] T001 Confirm the worktree's code matches `develop` (`eda2e0b`) with `git diff --stat eda2e0b -- crates docs examples ui` (expect no output), and run `cargo test -p teamwire-watch --locked` once as the baseline; record both in your report (changes no file)

---

## Phase 2: Foundational

**Purpose**: No code prerequisite is shared by all four stories. Each
story starts from `develop`. The stories share files only for guidance
text, listed in research.md under "Files shared between stories".

- [ ] T002 Read `specs/001-deckh-fixes/research.md`, `specs/001-deckh-fixes/data-model.md`, `specs/001-deckh-fixes/contracts/cli.md` and `.specify/memory/constitution.md` before any story task (changes no file)

**Checkpoint**: Any story may start.

---

## Phase 3: User Story 1 - Results and questions reach the lead, and the watch stays on (Priority: P1) 🎯 MVP

**Goal**: The lead's hooks list each result and question that was never
typed, with its file, and nothing is typed again. `received <id>` and
`answer --re` settle a message. The watch keeps a member whose session
works. A waking message starts a generation. A message to a member that
asked for a handoff is held for the handoff.

**Independent Test**: Run `cargo test -p teamwire-report -p teamwire-msg -p teamwire-watch -p teamwire-launch --locked`, `cargo test -p teamwire-harness-claude --locked --test lead_setup`, and `cargo test -p teamwire-cli --locked --test status --test hook_lead`. The new tests listed in research.md under F1, F2 and F3 must pass, and each must fail with its fix reverted.

### ADR for US1

- [ ] T003 [US1] Write `docs/adr/0043-lead-reads-undelivered-and-woken-members-are-tracked.md`. Record decisions F1-a to F1-e, F2 and F3-a to F3-e from `specs/001-deckh-fixes/research.md`. Name each superseded point: WAT-6 (`docs/requirements.md:177`), ADR 0007 point 6 (`docs/adr/0007-completion-reporting.md:91-93`), INV-14 (`docs/requirements.md:57`) and REP-3 (`docs/requirements.md:129`), widened to a waking message, and the Claude lead tool-hook matcher. State that ADR 0003 point 7 and INV-8 stay in force. Do not edit any older ADR.

### F1: the lead learns of undelivered items without typing

- [ ] T004 [US1] Write failing tests `a_question_and_a_result_never_typed_are_listed_with_their_files` and `a_row_the_lead_received_is_not_listed` in `crates/teamwire-report/tests/undelivered_to_lead.rs`. Add `pub fn undelivered_to_lead(home, session, platform, clock) -> Vec<String>` as a stub returning an empty list in `crates/teamwire-report/src/lead_stop.rs`, and export it from `crates/teamwire-report/src/lib.rs`. Each test fails on its line assertion (`comms/m001.md`, the outbox path).
- [ ] T005 [US1] Implement `undelivered_to_lead` in `crates/teamwire-report/src/lead_stop.rs`, reusing `led_by` (l.38). For each open team the session leads, add one line per lead delivery in state `undelivered`, plus one line per completion with report state `Undelivered`. Each line holds the id or role, the file path, and `teamwire received <team> <id|role>`, with the phrase `was never typed into your pane` (see `specs/001-deckh-fixes/contracts/cli.md`). If a query is missing, add it to `crates/teamwire-store/src/delivery.rs`. T004 passes.
- [ ] T006 [US1] Write failing tests. In `crates/teamwire-cli/tests/hook_lead.rs` (or the existing lead-hook test file under `crates/teamwire-cli/tests/`), add `the_tool_and_stop_hooks_carry_the_undelivered_note`: it drives `lead_tool_in` and `lead_stop_in` with `--harness claude-code`, `codex` and `pi` on a team whose question to the lead is `undelivered`, and asserts that each reply holds `was never typed into your pane`. In `crates/teamwire-harness-claude/tests/lead_setup.rs`, add `the_lead_tool_hook_runs_on_every_tool`, which asserts that the plugin's `PostToolUse` entry has no edit-and-shell-only matcher.
- [ ] T007 [US1] Call `teamwire_report::undelivered_to_lead` from `lead_stop_in` (extend `note`, l.75-81) and from `lead_tool_in` (join with `lead_tool_note`'s result, l.118-123) in `crates/teamwire-cli/src/cmd/hook/lead.rs`. Remove the edit-and-shell-only `TOOL_HOOK_MATCHER` from `crates/teamwire-harness-claude/src/lead_setup/plugin.rs:26-29, 92`, so every tool call runs the hook, and say why in a comment (the answer to `AskUserQuestion` frees a blocked pane). T006 passes. Update any existing test in `crates/teamwire-harness-claude/tests/lead_setup.rs` that pins the old matcher, in the same commit.
- [ ] T008 [US1] Write failing tests in `crates/teamwire-cli/tests/status.rs`. `received_with_a_message_id_clears_it_from_status_without_typing` asserts that after `teamwire received <team> m001`, `status` no longer lists `m001` under deliveries that need attention, and the fake host records no typing (today the command refuses "no member has this role"). `received_refuses_an_unknown_id_and_a_message_not_to_the_lead` asserts the two refusals from `specs/001-deckh-fixes/contracts/cli.md`.
- [ ] T009 [US1] Implement message ids in `mark_received` in `crates/teamwire-report/src/received.rs`. An argument that parses as a display id becomes `ack_delivery(at, message, &Recipient::Lead)` inside the same `Store::write`. Add new `Received` outcomes for read, already read, unknown id, and not a message to the lead. Parse the arguments and print the outcomes in `crates/teamwire-cli/src/cmd/received.rs`. In the same commit, update `docs/api/cli.md:593-603` (`received` takes ids) and `examples/skills/teamwire/SKILL.md:700-712` (use `received <id>` for a message read from its file). T008 passes.
- [ ] T010 [US1] Write the failing test `an_answer_with_re_settles_the_questions_lead_row` in `crates/teamwire-msg/tests/answer.rs`, using the message rig in `crates/teamwire-msg/tests/support/rig.rs`. The test asserts that the question's `lead` row is `acked` after the lead's answer with `re` (red today, as in rca's F1b).
- [ ] T011 [US1] In `store_message` (`crates/teamwire-msg/src/send.rs:297-321`), ack the delivery row `(re, <sender as recipient>)` with `ack_delivery` in the same write, when the new message is an `answer` with `re` set and that row exists. In the same commit, update `docs/api/cli.md` (the `send` section, `:524-590`: an answer settles its question) and `examples/skills/teamwire/reference.md:110-115` (the `QUESTION-UNDELIVERED` and `UNANSWERED` rows). T010 passes.
- [ ] T012 [US1] Write the failing test `an_undelivered_question_line_names_its_file` in `crates/teamwire-watch/tests/poll.rs`. It asserts that the `QUESTION-UNDELIVERED` line contains `comms/m001.md`.
- [ ] T013 [US1] Carry the comms path in `Question` (`crates/teamwire-watch/src/snapshot.rs:116-141`), and append ` File: <path>` in `question` (`crates/teamwire-watch/src/plan.rs:292-303`) for `QUESTION-UNDELIVERED` and `QUESTION-UNCONFIRMED`. In the same commit, update `docs/api/cli.md:218-223` (the watch line table), add the requirement rows for the lead's undelivered note and `received <id>` beside DEL-16 in `docs/requirements.md:120`, and finish `examples/skills/teamwire/reference.md:110-115` (teamwire does not retry; the hook lists the item; `received <id>` clears it). T012 passes.

### F2: the watch keeps a member whose session works

- [ ] T014 [US1] Write failing tests in `crates/teamwire-watch/tests/poll.rs`:
  - `an_undelivered_result_of_a_working_member_does_not_complete_the_team` asserts no `TEAM-COMPLETE` while the fake session is `Working`. Today the watch prints `TEAM-COMPLETE 1/2 done; not done: impl undelivered`.
  - `an_undelivered_result_of_an_idle_member_still_settles`.
  - `an_undelivered_result_with_no_visible_session_settles_and_says_so` asserts the line holds `session not visible`.

  Also write `an_undelivered_member_that_works_reads_working` in `crates/teamwire-watch/tests/status.rs`. Copy the shape of rca's `rca_f2_f3.rs`, which is read only in `/Users/apoorvagarwal/.teamwire/scratch/dfix/rca/repo/crates/teamwire-watch/tests/`.
- [ ] T015 [US1] Call `look` in the `Undelivered` and `Unconfirmed` arms of `judge_completion` in `crates/teamwire-watch/src/plan.rs:440-455`, after the one-time announcement:
  - `Alive(Working)` or `Alive(Blocked)` gives `Verdict::waiting()`.
  - `Idle` or died settles.
  - `Absent` or `NoInfo` settles and says `session not visible`.

  In `state_word` (`crates/teamwire-watch/src/status.rs:249-264`), return `working` for `Undelivered` or `Unconfirmed` with `Seen::Alive(Activity::Working)`. Some existing tests pin today's behaviour: `poll.rs` `undelivered_result_printed_once` (l.153), `two_undelivered_same_second_two_lines` (l.178) and `unconfirmed_result_announced_once` (l.825), and `status.rs` `busy_team` (l.70-83). In those, set the fake session to `Idle` where they need a settled member. Never loosen an assertion.

  In the same commit, update:
  - `docs/requirements.md:177` (WAT-6) and `:55` (INV-12) if they are worded on "settled";
  - `docs/api/cli.md:228, 236`;
  - `examples/skills/teamwire/reference.md:110-111, 117`;
  - `examples/skills/teamwire/SKILL.md:611-623, 666-668`.

  T014 passes.

### F3: a waking message starts a generation; a handoff request holds messages

- [ ] T016 [US1] Write failing tests in `crates/teamwire-msg/tests/woken.rs`:
  - `an_answer_to_a_member_with_a_done_result_starts_a_generation`: the generation is one higher.
  - `a_note_to_a_member_with_a_blocked_or_failed_result_starts_a_generation`: one case each.
  - `a_known_delivery_failure_undoes_the_woken_generation`: a `no_place` failure.

  Also write `a_member_woken_by_a_message_is_tracked` in `crates/teamwire-watch/tests/poll.rs`. It asserts no `TEAM-COMPLETE` while the woken member works (red today, as in rca's `f3_…`).
- [ ] T017 [US1] In `store_message` (`crates/teamwire-msg/src/send.rs:297-321`), for each member recipient of a message that is not a reprompt, check the newest completion. If it is in the member's current generation and its summary does not start with `handoff:`, call `bump` (`crates/teamwire-msg/src/attempt.rs:84-102`) and push the result into `bumped`. A known delivery failure then undoes the bump (REP-3). Update any test that pins "only a reprompt bumps": `crates/teamwire-msg/tests/reprompt.rs`, `crates/teamwire-msg/tests/stopped.rs` and `crates/teamwire-report/tests/turn_end.rs` `completion_after_reprompt_is_new_result`.

  In the same commit, update:
  - `docs/requirements.md:57` (INV-14), `:129` (REP-3) and `:179` (WAT-8);
  - `docs/failure-modes.md:41-42`;
  - `docs/api/cli.md:237` (status words);
  - `examples/skills/teamwire/SKILL.md:714-715`: any message to a finished member wakes it as a new generation.

  T016 passes.
- [ ] T018 [US1] Write failing tests:
  - In `crates/teamwire-msg/tests/woken.rs`, `a_message_to_a_member_with_a_handoff_result_is_held_not_typed`: the row is `undelivered` with code `held_for_handoff`, and the fake host records no typing.
  - In the same file, `a_reprompt_names_the_held_messages_after_its_text`: the typed line holds `Held for you, oldest first: m001` after the reprompt body.
  - In `crates/teamwire-msg/tests/resend.rs`, `a_held_message_is_not_resent`: `resend` refuses and names `handoff`.
  - In `crates/teamwire-launch/tests/handoff.rs`, or the existing handoff test file under `crates/teamwire-launch/tests/`, `a_handoff_first_line_counts_the_held_messages`: the first line holds `1 unread message(s)`.

  Add `HeldForHandoff => "held_for_handoff"` to `DeliveryErrorCode` in `crates/teamwire-core/src/message.rs:112-145`, so the tests compile and fail on their assertions.
- [ ] T019 [US1] Implement the hold:
  - In `crates/teamwire-msg/src/send.rs` and `crates/teamwire-msg/src/attempt.rs`, when the newest completion in the current generation has a `handoff:` summary, store the row `undelivered` with `held_for_handoff`. Type nothing, print `<role>: held for handoff (<id>)`, and exit 0.
  - In `crates/teamwire-deliver/src/lines.rs:59-110`, append the sentence `Held for you, oldest first: <id> (<type> from <sender>), …. Read them with teamwire inbox.` to a reprompt's line when the member has held rows.
  - Refuse a held row in `crates/teamwire-msg/src/resend.rs:130-143`, naming `handoff` and `reprompt`.

  In the same commit, update:
  - `crates/teamwire-launch/contracts/talking_head.md:12-16`: a held message is not resent. Also update its snapshot tests in `crates/teamwire-launch/src/contract/tests*.rs`.
  - `crates/teamwire-cli/src/cmd/handoff_guide.md`.
  - `examples/skills/teamwire/SKILL.md:841-847`: act on `handoff:`, where messages wait for the handoff.
  - The "Context handoff" section of `examples/skills/teamwire/reference.md`.
  - `docs/api/cli.md:524-590`: `send` prints `held for handoff` with exit 0, and `resend` refuses.

  T018 passes. The handoff test passes with no handoff code change, because `crates/teamwire-launch/src/handoff.rs:86-109` already counts `undelivered` rows as unread. Do not edit that file unless the test shows otherwise.

**Checkpoint**: US1 works on its own. The lead can run the deckh sequence on the fakes without a dummy reprompt or a second typing.

---

## Phase 4: User Story 2 - The lead sees which tasks can run in parallel and which files each member needs (Priority: P2)

**Goal**: `teamwire speckit draft <tasks.md> [--repo <dir>]` prints:

- the files that phases share, and the files that stories share;
- the tasks that change no file, and the tasks whose files no other task
  names;
- one DRAFT `owns` list per story, plus one for setup and polish, with the
  mentioning files and the reason for each.

**Independent Test**: `cargo test -p teamwire-cli --locked --test speckit_draft` and `cargo test -p teamwire-verify --locked`. Then run `cargo run -p teamwire-cli --locked -- speckit draft specs/001-deckh-fixes/tasks.md` and check that it prints a phase table and DRAFT lists.

### ADR and command stub for US2

- [ ] T020 [P] [US2] Write `docs/adr/0044-speckit-draft-reads-tasks-md.md`. It supersedes ADR 0042 decision 1 (`docs/adr/0042-spec-kit-after-the-review.md:19-27, 82-87`) in part: the reader exists now. Record the parsing rule, the mention rule (exact path, dotted module suffix with at least two parts, full file name with its extension, CI `paths:` glob; never a bare stem), the excluded feature folder, and that the lists are drafts for the lead. Do not edit ADR 0042.
- [ ] T021 [US2] Add the `speckit` command group with a `draft` subcommand (`<tasks.md>`, `--repo <dir>`) to `crates/teamwire-cli/src/args.rs:35-124`, following the `Afk` group pattern. Dispatch it in `crates/teamwire-cli/src/main.rs:13-48, 84-115`. Add `crates/teamwire-cli/src/cmd/speckit.rs` as a stub that exits 0 and prints nothing, with empty sub-files `crates/teamwire-cli/src/cmd/speckit/parse.rs`, `crates/teamwire-cli/src/cmd/speckit/overlap.rs`, `crates/teamwire-cli/src/cmd/speckit/mentions.rs` and `crates/teamwire-cli/src/cmd/speckit/render.rs` attached with `#[path]`, as `crates/teamwire-cli/src/cmd/doctor_speckit.rs:19-25` does. This task exists so that T022's tests fail on assertions, not on a missing command.

### Tests for US2 (write first, see them fail)

- [ ] T022 [US2] Write the failing integration tests in `crates/teamwire-cli/tests/speckit_draft.rs`, copying the `git`, `teamwire` and temporary-repo helpers' shape from `crates/teamwire-cli/tests/preset.rs:45-53, 238-246`. The tests are listed in `specs/001-deckh-fixes/research.md` under US2:
  - `phases_that_name_one_file_are_listed_as_sharing_it`
  - `stories_that_share_a_file_are_named`
  - `a_task_with_no_path_is_listed_as_changing_no_file`
  - `a_task_whose_files_no_other_task_names_is_listed_as_independent`
  - `a_ci_paths_glob_that_matches_a_task_path_adds_the_workflow`
  - `a_file_naming_the_dotted_module_is_added`
  - `a_test_naming_the_full_file_name_is_added`
  - `a_file_naming_only_the_bare_stem_is_not_added`
  - `the_feature_folder_is_not_a_mention`
  - `an_unreadable_workflow_is_named_not_skipped`
  - `a_missing_or_empty_tasks_file_exits_non_zero`
  - `the_drafts_are_labelled_as_drafts`

  The fixture repository holds a `tasks.md`, a `.github/workflows/replay.yml` with `on.push.paths`, an `.importlinter` naming `harness.tools`, a test naming `tools.py`, and a file naming only `tools`.
- [ ] T023 [P] [US2] Make `Owns` and its matching method `pub` in `crates/teamwire-verify/src/owns.rs:9-30`, and export it from `crates/teamwire-verify/src/lib.rs:26-35`. Keep `literal_separator(true)`. This is a visibility change only, with no new behaviour. T022's `a_ci_paths_glob_that_matches_a_task_path_adds_the_workflow` covers the matching through the command.

### Implementation for US2

- [ ] T024 [US2] Implement the parser in `crates/teamwire-cli/src/cmd/speckit/parse.rs`. It reads `## Phase <n>: <title>` headings and the task lines `- [ ]`, `- [x]` and `- [X]` `T<digits>`, with an optional `[P]` and an optional `[US<n>]`. A path is a token in backticks, or a bare token, that holds a `/` or ends in `.<ext>`, with trailing punctuation removed and URLs left out. Unit tests go at the bottom of the file: written first, they fail against a stub that returns no tasks.
- [ ] T025 [US2] Implement the overlap in `crates/teamwire-cli/src/cmd/speckit/overlap.rs`: the shared paths per phase pair and per story pair, the tasks with no path, and the tasks whose paths no other task names. Unit tests go at the bottom of the file, written first.
- [ ] T026 [US2] Implement mentions in `crates/teamwire-cli/src/cmd/speckit/mentions.rs`:
  - List the files with `git ls-files -z`, run through the Platform runner as `crates/teamwire-cli/src/cmd/doctor_speckit.rs:78-100` does. Leave out the feature folder that holds the `tasks.md`.
  - Match the exact path, any dotted module suffix with at least two parts, and the full file name as a whole token.
  - Read `.github/workflows/*.yml` and `*.yaml` with `parse_yaml` (`crates/teamwire-cli/src/cmd/doctor_speckit/yaml.rs:118`), and match each `on.<event>.paths` entry with `teamwire_verify::Owns`.
  - Report each workflow the reader refuses.
  - Never match a bare stem.
  - Unit tests go at the bottom of the file, written first.
- [ ] T027 [US2] Implement the output in `crates/teamwire-cli/src/cmd/speckit/render.rs` and the run in `crates/teamwire-cli/src/cmd/speckit.rs`, using the layout in `specs/001-deckh-fixes/contracts/cli.md`. Print the header, the phase pairs, the line naming the stories that share files, the tasks with no file, the independent tasks, and the unread workflows. Then print `DRAFT owns lists. Check each one before you launch: these are drafts, not final lists.`, followed by one list per story plus setup and polish, each path with its reason. A missing file, or a file with no task lines, exits non-zero with the bad-input code from `docs/api/cli.md`. `--repo` defaults to the git top level of the folder that holds `tasks.md`. Keep each file under 400 lines. T022 passes.
- [ ] T028 [US2] Update the guidance in one commit:
  - `examples/skills/teamwire/SKILL.md:120, 126, 172-190`: for a spec-kit feature, run `teamwire speckit draft`, check its drafts, and keep the manual grep for the names a draft cannot see.
  - `examples/skills/teamwire/reference.md:301-309, 557, 579-596`: split per story from the command's table, and show the table and the recommended split to the user before launch.
  - `examples/presets/speckit.json` lines 10 (BUILD), 13 (SPLIT ONLY ON DISJOINT FILES) and 63 (the implementer `_note`).
  - `examples/skills/afk/SKILL.md`: the passage that ADR 0042's consequences name.
  - `docs/api/cli.md`: a new `### teamwire speckit draft <tasks.md>` section near `### teamwire preset` (`:725`).
  - `docs/ARCHITECTURE.md`, if it lists the CLI's command groups.

  Then run `cargo test -p teamwire-cli --locked --test preset`, because it checks the skill's command names against the binary.

**Checkpoint**: US2 works on its own.

---

## Phase 5: User Story 3 - Small fixes to guidance and status output (Priority: P3)

**Goal**: The F6 checklist rule, the F8 closer gate slots, the F10 current
tool in `status --role` on every harness, and the F11 symlinks in the close
dry run.

**Independent Test**: Run `cargo test -p teamwire-cli --locked --test preset --test status`, `cargo test -p teamwire-close --locked --test ignored_files`, `cargo test -p teamwire-watch --locked --test status`, and `cargo test -p teamwire-harness-claude -p teamwire-harness-codex -p teamwire-harness-pi --locked`.

### F6: read the checklist before the implementer

- [ ] T029 [US3] Write the failing test `the_speckit_preset_has_the_lead_read_the_checklist_before_the_implementer` in `crates/teamwire-cli/tests/preset.rs`. It asserts that the speckit preset's rules hold `checklists/requirements.md`, the phrase `before the implementer launches`, and `waive`.
- [ ] T030 [US3] Rewrite the rule at `examples/presets/speckit.json:11` and the Checklists passage at `examples/skills/teamwire/reference.md:505-522`, in one commit:
  - `specify` can leave items unticked, with the reason in the checklist's Notes, after three rounds or on purpose.
  - Before the implementer launches, the lead reads `checklists/requirements.md`.
  - For each unticked item, the lead either waives it in the implementer's brief (quote the item and say proceed) or reprompts the spec member to fix it.

  T029 passes.

### F8: the closer's local gate and CI gates

- [ ] T031 [US3] Write the failing test `the_closer_task_has_a_local_gate_and_a_ci_gate_slot_and_reports_both` in `crates/teamwire-cli/tests/preset.rs`. It asserts that the closer task holds `Local gate: CHANGEME` and `Gates left to CI: CHANGEME`, and an instruction to report which gate ran with its exit code and which gates were left to CI. Keep `the_closer_task_lists_setups_and_has_a_restore_fallback` (`preset.rs:1520-1531`) as it is.
- [ ] T032 [US3] Replace the one full-gate slot with the two slots in `examples/presets/closer.json:13, 28`. The closer runs the local gate once on each head that ships, runs none of the CI gates, and reports both. When the user allows the full gate locally, the lead writes the full gate in the local slot and `none` in the CI slot. In the same commit, update `examples/skills/teamwire/SKILL.md:280, 864-880` ("Fill in the preset's full gate" becomes the two slots). T031 passes.

### F10: the current tool call in `status --role`

- [ ] T033 [US3] Add `crates/teamwire-core/src/tool_run.rs`, holding `pub struct ToolRun { pub name: String, pub started_at: Timestamp }`, and export it from `crates/teamwire-core/src/lib.rs`. Add `fn current_tool(&self, session: &SessionId, cwd: Option<&Path>) -> Result<Option<ToolRun>, HarnessError> { Ok(None) }` to the `Harness` trait in `crates/teamwire-core/src/harness.rs`, beside `context_fill` (l.112). This is the stub the tests below need.
- [ ] T034 [P] [US3] Write the failing test `the_current_tool_is_the_newest_tool_use_without_a_result` in `crates/teamwire-harness-claude/tests/sessions.rs`. Its fixture, `crates/teamwire-harness-claude/tests/fixtures/transcript-tool.jsonl`, copies real assistant `tool_use` and user `tool_result` lines from a local Claude Code transcript, with content trimmed and paths made generic. The test asserts `Bash` with the fixture's start timestamp, and `None` once the result line follows.
- [ ] T035 [P] [US3] Write the failing test `the_current_tool_is_the_newest_call_without_output` in `crates/teamwire-harness-codex/tests/session.rs`, with the fixture `crates/teamwire-harness-codex/tests/fixtures/rollout-tool.jsonl` in the `response_item` / `function_call` / `function_call_output` shape. First confirm that shape against Codex's published rollout format or a real rollout file, and record which one you used in your report. research.md marks the shape INFERRED.
- [ ] T036 [P] [US3] Write two failing tests. `the_current_tool_comes_from_the_state_file`, in `crates/teamwire-harness-pi/tests/session.rs`, uses a `<id>.teamwire.json` state file with `tool: {name, started_at_ms}`. `the_member_extension_stamps_and_clears_the_tool`, in `crates/teamwire-harness-pi/tests/launch.rs` (or the extension's snapshot test), asserts that the generated member extension registers `tool_call` and writes `tool`.
- [ ] T037 [US3] Implement `current_tool` for Claude Code in `crates/teamwire-harness-claude/src/tool.rs`, which is new. Reuse `find_transcript` and `tail_lines` from `crates/teamwire-harness-claude/src/fill.rs:76-118`, and wire the method into the `Harness` impl in `crates/teamwire-harness-claude/src/lib.rs`. T034 passes.
- [ ] T038 [US3] Implement `current_tool` for Codex in `crates/teamwire-harness-codex/src/tool.rs`, which is new. Reuse `rollout_path` and `tail_text` from `crates/teamwire-harness-codex/src/session.rs:60, 262`, and wire the method into the `Harness` impl in `crates/teamwire-harness-codex/src/lib.rs`. T035 passes.
- [ ] T039 [US3] Implement the pi side:
  - The member extension writes `tool: {name, started_at_ms}` on `tool_call` and clears it on `tool_result`. Use the state writer in `crates/teamwire-harness-pi/src/extension.rs:102-116, 179-200, 277`.
  - `current_tool` reads that field in `crates/teamwire-harness-pi/src/session.rs:11-17`, wired into the `Harness` impl in `crates/teamwire-harness-pi/src/lib.rs`.

  T036 passes.
- [ ] T040 [US3] Write the failing tests `status_role_shows_the_current_tool_and_its_duration` in `crates/teamwire-watch/tests/status.rs` and `status_role_prints_how_long_the_tool_has_run` in `crates/teamwire-cli/tests/status.rs`. With the fake harness returning a `ToolRun` 900 s old, the view's `current_tool.seconds == 900`, and the output holds `tool: Bash, running 15m`. If the fake harness cannot return a `ToolRun` yet, add one setter to `FakeHarness` in `tests/fakes/src/harness.rs` (protected; the lead granted this file for this one setter only), beside `set_liveness` (`tests/fakes/src/harness.rs:151`), and name the grant in the commit's progress line.
- [ ] T041 [US3] Implement the watch and CLI side:
  - Add `crates/teamwire-watch/src/tool.rs`, modelled on `crates/teamwire-watch/src/context.rs:13-26`.
  - Add the field `current_tool: Option<ToolView>` (with `name`, `started_at` and `seconds`) to `MemberView` in `crates/teamwire-watch/src/lib.rs:57-81`.
  - Fill the field in `status_with` (`crates/teamwire-watch/src/status.rs:307-357`) only for `--role` and a working member. `seconds` comes from `Clock`.
  - Print `tool: <name>, running <duration>`, or `tool: unknown (this harness does not report it)`, in `crates/teamwire-cli/src/cmd/status.rs:245-269`.

  In the same commit, add `current_tool: ToolView.nullish()` to the zod `MemberView` in `ui/src/shared/teamwire.ts:71-95` and run the checks that `ui/AGENTS.md` names. Also update `docs/api/cli.md:236-244` (status `--role`), and the passage in `examples/skills/teamwire/SKILL.md` that tells the lead to read `status --role` for a member that looks stuck. T040 passes.

### F11: symlinks in the close dry run

- [ ] T042 [P] [US3] Write two failing tests in `crates/teamwire-close/tests/ignored_files.rs`. `a_symlinked_ignored_folder_is_named_as_a_symlink_with_its_target` asserts `.venv -> <target>` under `removes symlinks (targets kept)`, and that `.venv` is not under `deletes`. `a_broken_symlink_is_marked_broken` asserts `(broken)`. Keep the existing tests in that file (`:36-114`) as they are.
- [ ] T043 [US3] In `ignored_warning` (`crates/teamwire-close/src/member_close.rs:59-90`), check each path with `std::fs::symlink_metadata`. List the symlinks apart from the deleted files, each with its target from `std::fs::read_link`, and mark a broken link `(broken)`. Leave out the "deletes" part when every path is a link. The ten-path limit applies to both lists together. In the same commit, update `docs/api/cli.md:507`. T042 passes.

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

- [ ] T044 [US4] Write `docs/adr/0045-a-file-waiver-holds-until-the-file-changes.md`. Record the F12 decision from `specs/001-deckh-fixes/research.md`. It supersedes ADR 0009 point 2 (`docs/adr/0009-verify-verdicts.md:30-33`) only for the flags that name one file: `test-deleted`, `config-changed`, `outside-owns`, `weakened`, `lint-removed`, `test-removed` and `asserts-removed`. State that a flag naming no file, `uncommitted:<path>` and `head-moved` keep the old rule, and that VER-5's record-at-HEAD rule for removal stays. Do not edit ADR 0009.

### Tests for US4 (write first)

- [ ] T045 [US4] Write the two failing tests in `crates/teamwire-verify/tests/verify.rs`, beside `the_spec_kit_constitution_is_check_config` (`verify.rs:382`), with the existing helpers (`repo_on_work`, `implementer`, `report_all_commits`, `run_one`, `flag_ids`, `waive`):
  - `a_file_flag_waiver_holds_while_that_file_is_unchanged`: the deckh replay. A member with `owns = ["src/"]` changes `.specify/memory/constitution.md`. The lead waives `config-changed:.specify/memory/constitution.md` and `outside-owns:.specify/memory/constitution.md`. The member commits `src/b.rs`. Assert `verdict == Verdict::Pass` and both flags `waived`.
  - `a_waiver_for_a_deleted_file_holds_while_the_file_stays_deleted`: with `deleted_test()` (`verify.rs:529-540`), waive `test-deleted:tests/a.rs`, commit `src/more.rs`, assert the flag is `waived`.

  Run `cargo test -p teamwire-verify --locked --test verify a_file_flag_waiver_holds a_waiver_for_a_deleted_file` and see both fail on the verdict or `waived` assertion. These tests need no new name.
- [ ] T046 [US4] Write five tests in `crates/teamwire-verify/tests/verify.rs` that keep the rest of the rule. They pass on `develop` and must still pass after T047:
  - `a_file_flag_waiver_ends_when_that_file_changes`: after the waivers of T045's first test, a commit changes the constitution again. The flags are not waived and the verdict is `Verdict::Flagged`.
  - `a_file_flag_waiver_ends_when_a_merge_changes_that_file`: a side branch made with `repo.git(&["checkout", "-b", ...])` changes the constitution, and the member merges it with `--no-ff`. The flags are not waived.
  - `a_waiver_at_a_commit_git_does_not_know_does_not_carry_over`: insert a waiver row with `tx.insert_waiver` and `head_sha` of forty `0`s for the constitution flag. The flag is not waived.
  - `a_waiver_for_a_flag_with_no_file_ends_at_the_next_commit`: an implementer with an empty `owns` raises `no-owns`. Waive it, commit, and assert `no-owns` is not waived.
  - `an_uncommitted_path_waiver_ends_at_the_next_commit`: waive `uncommitted:notes.txt`, commit another file, and assert the flag is not waived.

### Implementation for US4

- [ ] T047 [US4] Implement the rule, in one commit with its guidance:
  - In `crates/teamwire-verify/src/inspect.rs:11-23`, add `path: Option<String>` to `RawFlag`, keep `RawFlag::new` with `path: None`, and add `RawFlag::about(path, id, detail)`. Use `about` at the file-flag sites: `inspect.rs:45` (`test-deleted`), `:51` (`config-changed`), `:57` (`outside-owns`), `:120` and `:177` (`weakened`), `:192` (`lint-removed`, the old path), `:208` (`test-removed`) and `:223` (`asserts-removed`). Leave `bad-owns` at `:35`.
  - In `crates/teamwire-verify/src/git.rs`, beside `file_at` (`git.rs:161-164`), add `Git::blob(rev, path) -> Result<Option<String>, VerifyError>` that runs `git ls-tree -z --full-tree <rev> -- <path>`: `Ok(Some(id))` for the blob id, `Ok(None)` for empty output, `Err` when git does not know `rev`.
  - Add `crates/teamwire-verify/src/waivers.rs` with one function that returns the waived flag ids for a member at HEAD. Read the member's rows with `tx.all_waivers()` filtered by role (no store change). A flag is waived when a waiver for its id is at HEAD, or when it has a path and a waiver for its id at an older HEAD `w` has `blob(w, path) == blob(HEAD, path)`, both `Ok`. Declare the module in `crates/teamwire-verify/src/lib.rs:26-32`.
  - In `crates/teamwire-verify/src/lib.rs:310-317`, call that function in place of the `tx.waivers(&member.role, head)` read. Rewrite the crate note at `lib.rs:1-6` ("waivers bound to a HEAD", "Never: ... count a waiver for a HEAD other than the one it was recorded at") to the new rule.
  - In `crates/teamwire-verify/tests/verify.rs`, change the file note at `:1-2` and the last block of `a_waiver_makes_the_flagged_pass_a_pass_for_that_head_only` (`verify.rs:627-632`): the comment names ADR 0045, and after the commit to `src/more.rs` it expects `Verdict::Pass` and the flag `waived`. Keep every assert. List this changed expectation in your report, because the lead's weakening check looks for one.

  In the same commit, update:
  - `examples/skills/teamwire/reference.md:204-208` ("Verify flags"): replace "A later commit needs a new waiver" with the rule: a waiver for a flag that names one file holds until that file's content changes, a merge or rebase included; any other waiver, `uncommitted:<path>` included, ends at the next commit.
  - `examples/skills/teamwire/SKILL.md:790-791`: a finished implementer gets a waiver per flag; a file flag's waiver holds until that file changes, and any other flag needs one at its final HEAD.
  - `docs/api/cli.md:432-433`, the `--waive` sentence of `teamwire verify`.
  - `docs/requirements.md:240` (VER-5): add the carry-over rule, and cite ADR 0045 and the T045 and T046 tests.
  - `docs/failure-modes.md:63`: a file flag's waiver carries over only while that file is unchanged, and verify must still run at the new HEAD.

  `teamwire verify` has no `--json` output, and `Flag` keeps its fields, so `ui/src/shared/teamwire.ts` does not change. Run `cargo test -p teamwire-verify --locked` and `cargo test -p teamwire-close --locked` (close reads verify records).

**Checkpoint**: The deckh replay passes after one waiver per flag.

---

## Phase 7: Polish & cross-cutting

- [ ] T048 Check that every new or changed source file in `crates/` stays under 400 lines, and split any that does not: `crates/teamwire-msg/src/send.rs`, `crates/teamwire-watch/src/plan.rs`, `crates/teamwire-watch/src/status.rs`, `crates/teamwire-close/src/member_close.rs`, `crates/teamwire-cli/src/cmd/speckit.rs`, `crates/teamwire-cli/src/cmd/speckit/*.rs`, `crates/teamwire-verify/src/inspect.rs` and `crates/teamwire-verify/src/waivers.rs`.
- [ ] T049 Run `scripts/gate.sh fast` and `scripts/test-changed.sh` from the repository root, and record both exit codes in your report (changes no file).
- [ ] T050 Run `teamwire speckit draft specs/001-deckh-fixes/tasks.md` on this feature, and record its phase table in your report for the lead (changes no file).

---

## Dependencies & Execution Order

### Phase dependencies

- Setup (T001) and Foundational (T002) come first. They change no file.
- US1 (Phase 3), US2 (Phase 4), US3 (Phase 5) and US4 (Phase 6) depend on
  no other story's code. They share guidance files and
  `crates/teamwire-verify/src/lib.rs`, listed below.
- Polish (Phase 7) comes after the stories that are built.
- One implementer runs every phase in order, so the shared files need no
  merge order.

### Within each story

- Each test task comes before its fix task, as numbered.
- US1 runs in order: F1 (T004-T013), then F2 (T014-T015), then F3
  (T016-T019). F1, F2 and F3 all change
  `crates/teamwire-watch/tests/poll.rs`, and F2 and F3 both change
  `crates/teamwire-watch/src/plan.rs` and `crates/teamwire-msg/src/send.rs`
  (F1-c and F3).
- In US2, T021 comes before T022. T023 can run beside T021 and T022.
  T024-T027 come after T022 and T023.
- In US3, F6, F8 and F11 have no dependencies. F10's T033 comes before
  T034-T041.
- In US4, T044 comes first. T045 and T046 come before T047. T047 edits
  `crates/teamwire-verify/src/lib.rs` after US2's T023 has exported `Owns`
  there.

### Files shared between stories (the lead measures these for a split)

| File | Tasks |
|---|---|
| `docs/api/cli.md` | US1: T009, T011, T013, T015, T017, T019. US2: T028. US3: T041, T043. US4: T047 |
| `examples/skills/teamwire/SKILL.md` | US1: T009, T015, T017, T019. US2: T028. US3: T032, T041. US4: T047 |
| `examples/skills/teamwire/reference.md` | US1: T011, T013, T015, T019. US2: T028. US3: T030. US4: T047 |
| `examples/presets/speckit.json` | US2: T028. US3: T030 |
| `crates/teamwire-cli/tests/preset.rs` | US3: T029, T031 (US2's T028 runs it, but does not edit it) |
| `crates/teamwire-watch/src/status.rs` | US1: T015. US3: T041 |
| `crates/teamwire-watch/tests/status.rs` | US1: T014, T015. US3: T040 |
| `crates/teamwire-cli/tests/status.rs` | US1: T008. US3: T040 |
| `docs/requirements.md` | US1: T013, T015, T017. US4: T047 |
| `docs/failure-modes.md` | US1: T017. US4: T047 |
| `crates/teamwire-verify/src/lib.rs` | US2: T023. US4: T047 |

### Tasks that change no file

T001, T002, T049 and T050.

### Parallel opportunities

These sets of tasks touch disjoint files:

- T020 (the US2 ADR), T023 (the `Owns` visibility change), T042 (the F11 test), T034,
  T035 and T036 (the harness tests, after T033).
- Within US3, F6 (T029-T030), F8 (T031-T032) and F11 (T042-T043) touch no
  file of F10 except `docs/api/cli.md` and `SKILL.md`.
- US2's code (`crates/teamwire-cli/src/cmd/speckit*`,
  `crates/teamwire-verify/`) touches no file of US1 or US3. Only its
  guidance task, T028, shares files.
- US4's code is all in `crates/teamwire-verify/`. Its only code file shared
  with another story is `crates/teamwire-verify/src/lib.rs` (US2's T023).
  T044, the US4 ADR, touches no other task's file.

---

## Parallel Example: User Story 3

```bash
# After T033 adds the trait method and ToolRun:
Task: "T034 Claude test in crates/teamwire-harness-claude/tests/sessions.rs"
Task: "T035 Codex test in crates/teamwire-harness-codex/tests/session.rs"
Task: "T036 pi tests in crates/teamwire-harness-pi/tests/session.rs"
Task: "T042 F11 test in crates/teamwire-close/tests/ignored_files.rs"
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
order. The notes below stay for a later team. Each story's code is disjoint
from the others' code, except `crates/teamwire-verify/src/lib.rs` (US2 and
US4). The shared files are guidance text and the status
code and tests above. A split per story needs one owner for each shared
file, or a merge order. Any merge conflict in a guidance file is a text
conflict.
