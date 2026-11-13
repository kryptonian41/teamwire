---

description: "Task list for 002-amend-member-check"
---

# Tasks: Amend a running member's check and owns

**Input**: Design documents from `specs/002-amend-member-check/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/amend-cli.md, quickstart.md

**Tests**: These are required. The spec (FR-017) and the constitution
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

**Existing tests that change**: one existing test changes its expected
text, because US3 spells out the worktree commands in the seat rules:
`crates/teamwire-launch/tests/contract_worktree_rule.rs:22` (task T019). No
assert may be removed. No other existing test changes. List every test you
changed, with its file and line and the rule it pinned, in your report.

**Guidance**: Each fix phase names the skill, preset, reference, doc,
README and ADR text it changes. That text goes in the **same commit** as
the code of its phase (constitution principle III).

**Root causes, the decision for each fix, and the line numbers on
`develop`** are in `specs/002-amend-member-check/research.md`. The
command's exact output, note and refusal text are in
`specs/002-amend-member-check/contracts/amend-cli.md`. Read both before the
first task of a story.

**Protected paths** (AGENTS.md): `tests/fakes/`, `scripts/`, `xtask/`,
`docs/adr/` (except the new ADR file of T014), root `Cargo.toml`,
`Cargo.lock`, `clippy.toml`, `rustfmt.toml`, `rust-toolchain.toml` and
`.cargo/config.toml`. No task changes any of them. The new file
`docs/adr/0046-amend-changes-a-running-members-check-and-owns.md` is added,
and no existing ADR is edited.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: The task can run in parallel. It touches different files and
  depends on no task that is still open.
- **[Story]**: US1, US2 or US3.

---

## Phase 1: Setup

**Purpose**: Confirm the base and the commands before changing code.

- [X] T001 Confirm the worktree's code matches `develop` (`7e78c1a`) with `git diff --stat 7e78c1a -- crates docs examples README.md` (expect no output), and run `cargo test -p teamwire-close --locked` once as the baseline; record both in your report (changes no file)

---

## Phase 2: Foundational

No task. Every new function serves US1 only, and US3 needs nothing from
US1. The parts US2 needs (the `amend` command) are built in Phase 3.

---

## Phase 3: User Story 1 - The lead changes a running member's check or owns (Priority: P1) 🎯 MVP

**Goal**: `teamwire amend <team> --role R [--check CMD] [--owns PATTERN]... --reason TEXT`
changes a started member's check and owns, sends a `CHECK AMENDED:` note in
the same store transaction, rewrites the member's seat rules file, and
makes every older verify record stop counting for `close --role` (F13).

**Independent Test**: In `crates/teamwire-cli/tests/amend.rs`, launch a fake
member with check A, amend it to check B, and see verify run B, the
member's inbox hold the note, `close --role` refuse until a new verify, and
a handoff session read B.

### Store: the scope setter

- [X] T002 [P] [US1] Write the failing test `set_member_scope_updates_check_and_owns_only` in `crates/teamwire-store/src/member/tests.rs`, the unit-test module of `member.rs` (`member.rs:583-584`), with the testkit the module's other tests use: insert a member with `check_cmd` `"a"` and owns `["x/"]`, call `tx.set_member_scope(&role, Some("b"), &["y/".to_owned(), "z/{c,d}/**".to_owned()])` inside `Store::write`, then read the member back: `check` is `Some("b")`, `owns` is exactly the two patterns in order, and `place`, `branch`, `state` and `generation` are unchanged. A second case sets the check of a member that had none. An unknown role returns the same error `set_member_place` returns for one (`exactly_one`). Add `pub fn set_member_scope(&self, role: &Role, check: Option<&str>, owns: &[String]) -> Result<(), StoreError>` in `crates/teamwire-store/src/member.rs` as a stub that returns `Ok(())` and writes nothing, so the test fails on its first `assert_eq!`.
- [X] T003 [US1] Implement `set_member_scope` in `crates/teamwire-store/src/member.rs`, next to `set_member_place` (line 247): one `UPDATE members SET check_cmd = ?2, owns = ?3 WHERE role = ?1`, with `owns` as the same JSON text `insert_member_with` writes, and `exactly_one`. It writes no event, as the other setters (data-model.md "Member scope"). Doc comment: why there is no event.

### Messages: one transaction for the note and the scope

- [X] T004 [P] [US1] Write the failing test `send_with_write_stores_the_message_and_the_write_together` in `crates/teamwire-msg/tests/send.rs`, with the file's rig: (a) a `note` from `lead` to `api` whose write closure calls `tx.set_member_scope` stores the message, one delivery row and the new scope, all visible after the call; (b) a closure that returns an error leaves no message, no delivery row and the old scope, and `send_with_write` returns that error as a `SendError`; (c) on a team that is not open the closure never runs and the send is refused with `TeamClosed`, as `send_with` refuses today. Add `pub fn send_with_write(ctx, req, also: &dyn Fn(&teamwire_store::Tx<'_>) -> Result<(), teamwire_store::StoreError>, on_stored: &mut dyn FnMut(&Stored)) -> Result<SendReport, SendError>` in `crates/teamwire-msg/src/send.rs` as a stub that calls `send_with` and ignores `also`, so (a) and (b) fail on their scope and message asserts. Case (c) passes on the base; it is a guard case inside this failing test, so write (a) first, where the test fails. Export it from `crates/teamwire-msg/src/lib.rs` next to `send_with`.
- [X] T005 [US1] Implement `send_with_write` in `crates/teamwire-msg/src/send.rs`: move the body of `send_with` (line 58) into it, pass `also` into `store_message` (line 299), and call `also(tx)` there right after `insert_message` and before the wake and hold steps, only when the message is not a duplicate and the team is open. `send_with` becomes `send_with_write(ctx, req, &|_| Ok(()), on_stored)`, so no caller changes (`crates/teamwire-cli/src/cmd/send.rs:86`, `crates/teamwire-msg/tests/send.rs:718,852`). Delivery stays after the commit. Keep `send.rs` under 800 non-test lines (it has 479).

### Verify: a run that straddles an amend never passes

- [X] T006 [P] [US1] Write the failing unit test `scope_changed_flags_a_record_whose_member_was_amended_mid_run` at the bottom of a new file `crates/teamwire-verify/src/scope.rs` (add `mod scope;` in `crates/teamwire-verify/src/lib.rs`). It calls `scope::changed(ran_with: &Member, stored: &Member) -> Option<Flag>` with two members that differ only in `check`, then only in `owns`, and expects `Some` flag with id `scope-changed`, `waived: false`, and the detail `the lead amended this member's check or owns while verify ran; run verify again`; with equal members, `None`. Add `changed` as a stub that returns `None`. Also write the **guard** test `scope_changed_cannot_be_waived` in `crates/teamwire-verify/tests/verify.rs`, in the shape of lines 1285-1312: `waive(&mut fx.ctx(), &role("api"), &FlagId::new("scope-changed"), "ok")` returns `Err(VerifyError::UnknownFlag(_))`. It passes on the base; record that.
- [X] T007 [US1] Implement `scope::changed` in `crates/teamwire-verify/src/scope.rs` (compare `check` and `owns`), and call it in `crates/teamwire-verify/src/lib.rs` inside the `ctx.store.write` that stores the record (line 371): re-read the member with `tx.reader().member(role)`, and when `changed` returns a flag, push it to `flags` and the stored `flags`, and set the verdict to `FLAGGED` unless it is `FAIL` or `NO-CHECK`. Return the record as stored, so the printed verdict matches the row. The flag is added after the waiver step, as `head-moved` is (lines 328-341), so `waive` never accepts it. Doc comment on `scope.rs`: why (research.md US1 decision 5). Guidance in the same commit: the `scope-changed` flag in `docs/api/cli.md`'s `verify` section next to `head-moved` (line 440), and in `examples/skills/teamwire/reference.md` "Verify flags" next to `head-moved` (lines 212 and 224).

### Close: an older pass does not count after an amend

- [X] T008 [P] [US1] Write the failing tests in `crates/teamwire-close/tests/remove.rs`, with the file's `Rig`, `verify_at` (line 44) and `rig.clock`:
  - `an_amend_note_makes_an_older_pass_not_count`: a verified implementer (`verified_implementer()`, line 97) with a `PASS` at HEAD; advance the clock; store a `note` from `lead` to that role whose body starts `CHECK AMENDED: ` (insert it through the store with `insert_message`, as the file's other store writes do); `remove(...)` is refused and a reason contains `was recorded before the lead amended its check or owns` and the note's display id. Advance the clock, `verify_at` a new `PASS` at the same HEAD, and the removal now succeeds. With `CloseOpts { unverified: true, .. }` the first removal goes through and the reason is listed as overridden.
  - `a_record_at_the_same_time_as_the_note_does_not_count`: the `PASS` and the note carry the same clock time; the removal is refused.
  - **guard** `a_pass_recorded_after_a_same_scope_amend_counts`: a note `CHECK AMENDED: no change` from `lead` whose scope lines name the same check and owns, then, at a later clock time, a `PASS` at HEAD; the removal succeeds. It pins the rule of research.md US1 decision 4: a record counts when it was recorded after the newest amend note and verify raised no `scope-changed`. It passes on the base, where close reads no note; record that.
  - **guard** `a_note_without_the_marker_or_from_a_member_changes_nothing`: a `note` from `lead` without the marker, and a message from another member that starts with the marker, both newer than the `PASS`; the removal succeeds. It passes on the base; record that.
- [X] T009 [US1] Implement the rule in `verify_gap` in `crates/teamwire-close/src/remove.rs` (lines 79-120): after the record at HEAD is found and is `PASS`, read `tx.messages(&MessageFilter { sender: Some(lead), recipient: Some(role), kind: Some(MessageType::Note), ..Default::default() })` in the same `store.read` (no `limit` and no `newest`: `newest` acts only with `limit`, `crates/teamwire-store/src/message.rs:51-53`, and a limit could cut off the newest marker note behind newer plain notes), and the code itself takes the last row, oldest first, whose body starts with `AMEND_MARKER`, and when the record's `recorded_at` is at or before that note's `created_at`, return `Some(format!("{role}'s verify at HEAD {sha7} was recorded before the lead amended its check or owns ({display_id}); run teamwire verify {team} --role {role} again"))`. Put the marker in one `pub const AMEND_MARKER: &str = "CHECK AMENDED:";` in `crates/teamwire-core/src/message.rs`, after `MessageType` (line 13), with a doc comment that `close --role` reads it, and export it from `crates/teamwire-core/src/lib.rs` (the explicit list at lines 56-59) in this task, because close uses it first; T013's CLI uses it too. The whole-team close does not change (research.md US1 "What close --role reads today"). Guidance in the same commit: `docs/api/cli.md` `close` section (line 462): `--role` also refuses a record made at or before the newest amend note; `docs/requirements.md` new row CLS-9 after CLS-8 (line 258), with the test names of T008; `examples/skills/teamwire/SKILL.md` paragraph at lines 829-836 ("So `close --role` refuses ...") gains one sentence: an amend makes it refuse until verify runs again.

### Launch: rewrite the seat rules file

- [X] T010 [P] [US1] Write the failing test `rewrite_contract_writes_the_new_scope` in `crates/teamwire-launch/tests/contract_rewrite.rs` (new): with `TestHome`, write a seat rules file for a member through `render_contract` at `<home>/teams/demo/contract/api.md`, then call `teamwire_launch::rewrite_contract(platform: &dyn teamwire_core::Platform, home: &Path, team: &TeamName, member: &Member, teamwire_bin: &Path) -> Result<PathBuf, String>` for a `Member` whose check is `"cargo test -p b"` and owns `["b/"]`; the file now equals `render_contract` of the same parts with the new check and owns, byte for byte. Add `rewrite_contract` in `crates/teamwire-launch/src/contract.rs` as a stub that returns `Ok(path)` and writes nothing, and export it from `crates/teamwire-launch/src/lib.rs` (line 45).
- [X] T011 [US1] Implement `rewrite_contract` in `crates/teamwire-launch/src/contract.rs` (or a new `crates/teamwire-launch/src/contract_rewrite.rs` if `contract.rs` would pass 400 lines): build `ContractParts` from the member (team, role, kind, `check`, `owns`), the team's `reports/<role>.md` and `outbox/` paths as `Dirs` gives them (`crates/teamwire-launch/src/member_parts.rs:27-45`), and the given binary path; render with `render_contract`; write `<team dir>/contract/<role>.md` with `member_steps::write_file` (mode `0o644`, `Platform::write_atomic`). Return the path. Return a `String` error as `write_file` does.

### CLI: the command

- [X] T012 [P] [US1] Write the failing tests in a new `crates/teamwire-cli/tests/amend.rs`. Build its rig in the shape of `crates/teamwire-cli/tests/handoff.rs:40-190` (`TestHome`, `FakeHerdrEnv`, a throwaway git repository, one launched implementer `api` with check `A` and owns `["src/"]`). On the base, clap exits 3 with "unrecognized subcommand 'amend'". Each case asserts the exit code first, so a case whose first expected exit is 0, 1, 2 or 4 fails on that assert. The refusal case's first expected exit is 3, the same as the base's, so it fails on its stderr assert instead. No case fails on a missing name:
  - `amend_changes_the_check_verify_runs_and_notes_the_member`: `teamwire amend alpha --role api --check B --reason "A runs every test file"` exits 0; stdout holds `amended api in team alpha: check` and `run \`teamwire verify alpha --role api\` again`; then `verify alpha --role api` writes `$ B   # in` as the first line of `<home>/teams/alpha/verify/api.log`; `teamwire inbox` as `api` holds a `note` from `lead` whose text starts `CHECK AMENDED: A runs every test file` and holds `check: A -> B`; `teamwire log alpha` lists it.
  - `amend_replaces_owns_and_keeps_a_brace_glob_whole`: `--owns src/ --owns 'crates/{b,c}/**'` exits 0; `teamwire teams --json` shows `owns` exactly `["src/", "crates/{b,c}/**"]`; a commit under `crates/c/` gets no `outside-owns:` flag from verify.
  - `amend_refuses_without_a_change_or_a_reason_and_changes_nothing`: no `--check` or `--owns`, no `--reason`, `--reason "  "`, `--check ""` and `--owns ""` exit 3 and stderr names the missing or empty part (clap's "required arguments were not provided", or the command's own "--reason is empty", "--check is empty", "--owns has an empty pattern"), which the base's "unrecognized subcommand" does not hold; an unknown role exits 1 and names it; a second member left `creating` (inserted through the rig's store with `Tx::insert_member`, `crates/teamwire-store/src/member.rs:111`) and a third set to `failed` (`Tx::set_member_state`, `member.rs:289`) each exit 1 and say the member is not started; a member removed with `close --role api --unverified` exits 1 and says it is not started; with `TEAMWIRE_ROLE=api` set, exit 1 and `refused.identity`; last, with the team put in `closing` through the rig's store (`Tx::begin_close`, `crates/teamwire-store/src/team.rs:186`), exit 1 and the reason says the team is not open. After each, the member row (`teams --json`) and `log alpha` are unchanged.
  - `amend_rewrites_the_seat_rules_and_a_handoff_reads_them`: after an amend to check `B`, `<home>/teams/alpha/contract/api.md` holds `B` in its check line and not `A`; after `teamwire done --status done --summary "handoff: test"` as `api` and `teamwire handoff alpha --role api`, the started agent's call (as `handoff.rs:217-300` reads it) names a `--append-system-prompt-file` whose file holds `B`.
  - `amend_leaves_earlier_verify_records_unchanged`: a verify before the amend; `teamwire export alpha` rows of `verify_records` before and after the amend are equal.
  - `an_amend_to_a_member_with_no_live_process_exits_4_and_keeps_the_change`: with the member's process gone in the fake host (the shape of `crates/teamwire-cli/tests/send.rs:376`), the amend exits 4; stdout's first line is the scope line `amended api in team alpha: owns (note m...)` and `send`'s delivery line `api: undelivered (...)` follows it, as `contracts/amend-cli.md` "Output" says; `teams --json` shows the new owns.
  - `a_seat_rules_failure_exits_2_even_when_the_note_is_undelivered`: with the member's process gone and `<home>/teams/alpha/contract/api.md` replaced by a directory (so the rewrite fails), the amend exits 2, not 4; stdout holds both the scope line and the delivery line, and stderr names the seat rules file and says to run the same amend again; `teams --json` shows the new owns.
  - `the_deckh_f14_sequence_ends_with_close_role_without_unverified` (SC-002): launch `api` with check `test -s specs/x/tasks.md` and a committed `specs/x/tasks.md`; `verify` passes; `api` records `done`; a commit on `api`'s branch removes `specs/x/`; `verify` fails; `close alpha --role api` refuses; `amend alpha --role api --check true --reason "spec-kit removed"` exits 0; `close alpha --role api` still refuses (no verify after the amend); after the member is idle again in the fake host (set its status as `handoff.rs`'s `set_status` does) and `verify alpha --role api` passes, `close alpha --role api` without `--unverified` exits 0.
  - `an_amend_note_wakes_a_done_member` (US1 scenario 8): after `api` records `done`, the amend exits 0, and `teamwire events alpha --json` holds a `member.woken` event for `api` whose generation is one more than before (ADR 0043's path, reached through `amend`).
  Also write `the_codex_skill_names_amend` at the bottom of `crates/teamwire-harness-codex/src/lead_setup/plugin.rs` (next to the tests at lines 308-320: the rendered `teamwire/skills/teamwire/SKILL.md` contains `teamwire amend`) and `the_pi_skill_names_amend` in `crates/teamwire-harness-pi/tests/lead_setup.rs` (the package file `skills/teamwire/SKILL.md` that the lead setup renders, `crates/teamwire-harness-pi/src/lead_setup.rs:139`, contains `teamwire amend`). Both fail on the base because the shared `SKILL.md` names no `amend`.
- [X] T013 [US1] Implement the command, following `contracts/amend-cli.md` exactly:
  - `crates/teamwire-cli/src/args.rs`: `/// Change one running member's check or owns, and tell it.` `Amend(AmendArgs)` in `Command` after `Verify` (line 51); `AmendArgs { team: String, #[arg(long, value_name = "R", required = true)] role, #[arg(long, value_name = "CMD")] check: Option<String>, #[arg(long, value_name = "PATTERN")] owns: Vec<String> (no value_delimiter), #[arg(long, value_name = "TEXT", required = true)] reason: String }` with a clap `ArgGroup` that requires one of `check` and `owns`.
  - `crates/teamwire-cli/src/main.rs`: `pub(crate) mod amend;` in the `mod cmd` block (line 16) and `Command::Amend(a) => cmd::amend::run(a, global, &wiring)` (line 92).
  - `crates/teamwire-cli/src/cmd/amend.rs` (new, `//!` note first): parse team and role (`fail("amend", ..., USAGE)` on error, as `cmd/verify.rs:24-31`); `refuse_member("amend")` before the store opens; refuse an empty or all-space `--reason` or `--check`, and an empty `--owns` pattern, with exit 3 (`amend` cannot clear the owns list); open with `with_ctx_using`; read the team and member; refuse (exit 1) an unknown role, a member not `started`, a team not open; build the note body of `contracts/amend-cli.md` with `AMEND_MARKER` (old values from the member row, `(none)` for no check); call `teamwire_msg::send_with_write` with actor `lead`, type `note`, one recipient, and an `also` that calls `tx.set_member_scope` with the new values (a field not given keeps its old value); then `teamwire_launch::rewrite_contract` with `std::env::current_exe()`; then print the lines of `contracts/amend-cli.md`: the scope line first, then `send`'s delivery line when the note is undelivered or unconfirmed. Exit 4 or 5 from the send report as `cmd/send.rs` maps them. When the rewrite fails, print its error and "run the same amend again" on stderr and exit 2, also when the note is undelivered or unconfirmed: exit 2 wins, because a wrong seat rules file misleads every later handoff. A store error is exit 6.
  - Guidance in the same commit: `docs/api/cli.md` new section `### teamwire amend <team> --role R [--check CMD] [--owns PATTERN]... --reason TEXT` after the `verify` section (line 461), `amend` in the lead-only list of "Lead commands" (lines 24-35), and in the exit-2 row of "Exit codes" (line 16) the partial case "amend saved the change but could not rewrite the seat rules file"; `docs/api/core.md`: `Tx::set_member_scope`, `teamwire_msg::send_with_write`, `teamwire_launch::rewrite_contract`, `teamwire_core::AMEND_MARKER` (exported in T009); `docs/requirements.md` new rows LCH-33 (after LCH-32, line 232) and VER-9 (after VER-8, line 245), each naming its tests; `docs/ARCHITECTURE.md` data model rows `contract/<role>.md` (line 100, "launch, amend") and `members` (line 115, `check_cmd` and `owns` changed by amend); `docs/failure-modes.md` a row "A member's check or owns is wrong" with the test names of T012; `README.md` command table (lines 255-262) an `amend` row; `examples/skills/teamwire/SKILL.md` after the paragraph at lines 819-825: a member whose check or owns is wrong gets `teamwire amend <team> --role <role> --check CMD --reason TEXT`, then `teamwire verify` again, because an older pass no longer counts; `examples/skills/teamwire/reference.md` "Other commands" table (line 286) an `amend` row; `examples/skills/afk/SKILL.md` at line 149: a wrong check gets `teamwire amend` before the next verify.
- [X] T014 [US1] Write `docs/adr/0046-amend-changes-a-running-members-check-and-owns.md` in the format of `docs/adr/0045-a-file-waiver-holds-until-the-file-changes.md`: status accepted, the context (F13), and decisions 1-7 of research.md US1 (command shape and exit codes; the note as the record, one transaction with the scope; no event type and no schema change; the `CHECK AMENDED:` marker and the close rule; `scope-changed`; the seat rules rewrite; shell only). State the close rule exactly as built: a verify record counts for `close --role` only when it was recorded after the newest amend note and verify raised no `scope-changed`. Say that this equals "started after the amend" except when the amend kept the same check and owns; then the old run's check is the same one, so nothing unproven passes. It supersedes nothing.
- [X] T015 [US1] Run the **guard** tests and record that they pass, one `cargo test` call per test-name filter: `cargo test -p teamwire-mcp --locked --test tools tool_list_is_the_ten_tools_of_the_doc` (no MCP tool was added), `cargo test -p teamwire-cli --locked --test preset the_skill_names_only_commands_and_tools_that_exist`, and `cargo test -p teamwire-cli --locked --test preset the_afk_skill_names_only_commands_that_exist` (the skills name `teamwire amend`, which now exists). Then `cargo test -p teamwire-store -p teamwire-msg -p teamwire-verify -p teamwire-close -p teamwire-launch -p teamwire-harness-codex -p teamwire-harness-pi --locked` and `cargo test -p teamwire-cli --locked --test amend --test close --test send --test handoff` (changes no file)

**Checkpoint**: `teamwire amend` works on its own; US2 can now name it.

---

## Phase 4: User Story 2 - A team that removes spec-kit keeps a valid check (Priority: P2)

**Goal**: The `speckit` preset, the skill reference and the skill say where
the commit that removes spec-kit goes, and how a lead whose removal is on
the implementer's branch keeps a valid check with `teamwire amend` (F14).

**Independent Test**: `cargo test -p teamwire-cli --locked --test preset the_speckit_preset_puts_the_removal_on_the_pr_branch_and_names_amend`.

- [ ] T016 [US2] Write the failing test `the_speckit_preset_puts_the_removal_on_the_pr_branch_and_names_amend` in `crates/teamwire-cli/tests/preset.rs`, next to `the_speckit_preset_has_the_lead_read_the_checklist_before_the_implementer` (line 1852): `rules_text("speckit")` and `speckit_section()` (line 1089) each contain `REMOVE SPEC-KIT`, `pull request branch`, `last verified head`, `never on the implementer's own branch` and `teamwire amend <team> --role <role> --check`. It fails on the base on the first `assert!`, because no rule holds these phrases (research.md US2).
- [ ] T017 [US2] Add the rule. In `examples/presets/speckit.json` `_rules`, a new entry after CLOSE (index 8): `REMOVE SPEC-KIT. When the user wants spec-kit's files out of the pull request, cut the pull request branch from the implementer's last verified head, the head of its newest PASS, and make the removal commit there, never on the implementer's own branch. The implementer's check tests specs/NNN-name/tasks.md, so a removal on its branch fails every later verify, a FAIL takes no waiver, and close --role then refuses it. When the removal is already on the implementer's branch, run teamwire amend <team> --role <role> --check CMD --reason TEXT with a check that does not test tasks.md, such as its test command alone, then teamwire verify <team> --role <role> again.` (keep the JSON valid; backticks around commands as the other rules use them). The same text in `examples/skills/teamwire/reference.md`, section "Spec-kit features", under "### The flow" (line 420). In `examples/skills/teamwire/SKILL.md`, at the end of the paragraph at lines 125-130 ("For a feature built with GitHub's spec-kit, start from ...", which ends "commits and `converge`."), one sentence: a team that removes spec-kit before the pull request follows the preset's REMOVE SPEC-KIT rule. The `speckit` row of the shapes table (line 120) does not change. All in one commit with T016's test. Run `cargo test -p teamwire-cli --locked --test preset`.

**Checkpoint**: US2 is text plus the test; it uses US1's command.

---

## Phase 5: User Story 3 - The role table lists every git ban of each seat (Priority: P3)

**Goal**: Each row of the skill's role table lists every git command that
kind's seat rules ban, and a test compares the two (F15).

**Independent Test**: `cargo test -p teamwire-launch --locked --test role_table_git_bans --test contract_worktree_rule`.

- [ ] T018 [P] [US3] Write the failing test `the_role_table_lists_every_git_ban_of_each_seat` in a new `crates/teamwire-launch/tests/role_table_git_bans.rs`. For each kind in `MemberKind::ALL`, render the seat rules with `render_contract` and the `ContractParts` of `crates/teamwire-launch/tests/contract_worktree_rule.rs:11-20` (check and owns empty). Find the bans by the rule of research.md US3 decision 1: join whitespace to single spaces, split into sentences at `". "` and `": "`, and in each sentence that contains `Never run`, `Never use`, `Do not use` or `The same goes for`, take every backticked span that starts with `git `. Read `examples/skills/teamwire/SKILL.md` from `Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/skills/teamwire/SKILL.md")`, and read only the role table: the rows after its header line ``| Kind | Does | May not | Set `mode` |`` (`SKILL.md:282`) and its separator line, up to the first line that does not start with `|`. Never read another table: the preset-shapes table also has a row that starts ``| `closer` |`` (`SKILL.md:123`), with no "May not" cell. In the role table, find the row that starts ``| `<kind>` |`` (the kind's snake_case name, as `MemberKind`'s `as_str` gives it), take its third cell ("May not"), and collect its backticked spans that start with `git `. Assert the two sets are equal, with a message that names the kind and each missing or extra command. On the base this test fails for `implementer` (missing `git worktree add`) and for the reviewer, plan_reviewer and investigator rows; the closer row matches on the base, because the base's `remove`, `move` and `prune` spans do not start with `git `. Record the failing assertion line. Also write two **guard** tests of the test file's own helpers, which pass on the base once the helpers exist (record that): `a_row_that_names_a_git_command_no_seat_bans_fails` calls the compare helper with the implementer's bans and a row text that adds `` `git push` ``, and asserts the result names `implementer` and `git push`; `a_closer_row_in_another_table_is_not_read` gives the row lookup a text with a three-column table holding a `` | `closer` | `` row before the role table, and asserts the lookup returns the role table's `closer` row.
- [ ] T019 [US3] Fix the text, in one commit with T018's tests:
  - `crates/teamwire-launch/contracts/how_to_work.md:29`: ``Never run `git worktree add`, `remove`, `move` or `prune`, and never`` becomes ``Never run `git worktree add`, `git worktree remove`, `git worktree move` or `git worktree prune`, and never``. Split only line 29 into two lines (a minimal re-wrap); do not re-flow the rest of the bullet. Three tests pin line breaks inside this bullet, and all three stay unchanged: `never` / `write into another member's directory` and `report blocked to the` / `lead` (`crates/teamwire-launch/tests/contract_worktree_rule.rs:23-24`), and `ask the parent as` / `HOW THIS TEAM WORKS TOGETHER says` (`crates/teamwire-launch/src/contract/tests_findings.rs:296`). A greedy re-flow at the file's width breaks two of them.
  - `crates/teamwire-launch/tests/contract_worktree_rule.rs:22`: the expected text changes to the spelled-out sentence. This is the one existing test whose expectation changes; its other two asserts stay. Name it in your report.
  - `examples/skills/teamwire/SKILL.md:284-288`, the "May not" cells: implementer: add "Run `git worktree add`, `git worktree remove`, `git worktree move` or `git worktree prune`." and keep "Run `git stash`, `git reset --hard` or `git checkout -- .`."; reviewer, plan_reviewer, investigator: add the same worktree sentence; closer: replace "Use `git worktree add`, your checkout or a member's worktree." with "Run `git worktree add`, `git worktree remove`, `git worktree move` or `git worktree prune`. Use your checkout or a member's worktree."
  - Run `cargo test -p teamwire-launch --locked` and `cargo test -p teamwire-cli --locked --test preset` (the skill tests read the same file).

**Checkpoint**: All three stories are done.

---

## Phase 6: Polish & Cross-Cutting Concerns

- [ ] T020 Run `cargo fmt --all`, then `scripts/gate.sh fast`, then `scripts/test-changed.sh` (it tests the changed crates and every crate that depends on them, against `$TEAMWIRE_BASE`, else `main`); fix what fails within the two-round limit, and record each command and its result in your report (changes no file unless a fix is needed)
- [ ] T021 Check the quickstart's automated block in `specs/002-amend-member-check/quickstart.md` against what you ran, and list in your report each test name of research.md with the task that wrote it and whether it failed on the base or is a guard (changes no file)

---

## Dependencies & Execution Order

### Phase dependencies

- **Setup (Phase 1)**: no dependency.
- **Foundational (Phase 2)**: empty.
- **US1 (Phase 3)**: after Setup.
- **US2 (Phase 4)**: after US1, because its text names `teamwire amend`,
  and `the_skill_names_only_commands_and_tools_that_exist`
  (`crates/teamwire-cli/tests/preset.rs:484`) fails when the skill names a
  command that does not exist.
- **US3 (Phase 5)**: after Setup only. It edits `SKILL.md` lines 284-288,
  which US1 and US2 do not touch.
- **Polish (Phase 6)**: after all stories.

### Inside US1

- T002 → T003 (store setter) → T004 → T005 (`send_with_write` calls
  `set_member_scope` in its test).
- T006 → T007 (verify flag) needs nothing from T003-T005 except the
  member row shape.
- T008 → T009 (close rule) needs nothing from T003-T007.
- T010 → T011 (rewrite) needs nothing from T003-T009.
- T012 → T013 needs T003, T005, T009 (`AMEND_MARKER`) and T011.
- T014 after T013. T015 last.

### Stories that share files

| File | Stories |
| --- | --- |
| `examples/skills/teamwire/SKILL.md` | US1 (lines 819-836), US2 (spec-kit pointer), US3 (lines 284-288) |
| `examples/skills/teamwire/reference.md` | US1 ("Verify flags", "Other commands"), US2 ("The flow") |
| `docs/api/cli.md` | US1 only |
| `crates/teamwire-cli/tests/preset.rs` | US2 only (US1 and US3 run its existing tests) |

One implementer runs the stories in order, so the shared files cause no
conflict.

## Parallel Example: User Story 1

```text
These test tasks touch different crates and can be written together:
T002 crates/teamwire-store/src/member.rs
T004 crates/teamwire-msg/tests/send.rs (needs T002's stub to compile)
T006 crates/teamwire-verify/src/scope.rs, crates/teamwire-verify/tests/verify.rs
T008 crates/teamwire-close/tests/remove.rs
T010 crates/teamwire-launch/tests/contract_rewrite.rs
T012 crates/teamwire-cli/tests/amend.rs and the two harness files
```

## Implementation Strategy

### MVP first

1. Phase 1, then Phase 3 (US1). Stop and run T015. `teamwire amend` alone
   fixes F13 and gives the lead the way out of F14.
2. Phase 4 (US2): the preset text prevents F14.
3. Phase 5 (US3): the role table fix for F15.
4. Phase 6.

### Commits

Commit at the end of every phase that has tasks, and inside US1 after each
test-then-fix pair (T002-T003, T004-T005, T006-T007, T008-T009,
T010-T011, T012-T014), so each commit holds a test and the fix that makes it
pass, with its guidance. Each commit names its task ids.
