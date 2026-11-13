# Research: Amend a running member's check and owns

All line numbers are on `develop` at `7e78c1a`. Every cause below was read
in the code by the spec member. The findings file is
`/Users/apoorvagarwal/Projects/teamwire-agent-local/deckh-findings-2026-11-10.md`.

## US1: `teamwire amend` (F13)

### Root cause (verified)

Launch is the only code that writes a member's check and owns.

- The store writes `check_cmd` and `owns` once, in the insert of the member
  row (`crates/teamwire-store/src/member.rs:111` `insert_member` and `:118`
  `insert_member_with`). The store's other member setters change the place,
  the branch, the session, the token hash and the state
  (`member.rs:247`, `:258`, `:267`, `:277`, `:289`). None changes `check_cmd`
  or `owns`.
- Launch renders the same two values into the member's seat rules file,
  `<team dir>/contract/<role>.md`, once
  (`crates/teamwire-launch/src/member_steps.rs:293-309` `write_contract`,
  path from `crates/teamwire-launch/src/member_parts.rs:36-40`).
- `teamwire handoff` and `teamwire reopen` build the fresh session from that
  same file and never rewrite it. `MemberLaunch::for_resume` points at
  `<team dir>/contract/<role>.md` (`crates/teamwire-core/src/harness.rs:340-358`),
  and each harness reads it when it builds the launch: Claude Code passes the
  path (`crates/teamwire-harness-claude/src/launch.rs:51`), Codex reads the
  text into its argv (`crates/teamwire-harness-codex/src/launch.rs:44`), pi
  passes the path (`crates/teamwire-harness-pi/src/launch.rs:42-77`).
- The CLI has no command that changes either value
  (`crates/teamwire-cli/src/args.rs:36-70`, the `Command` enum).

So a wrong check stays for the rest of the run. Verify reads the member row
at the start of each run (`crates/teamwire-verify/src/lib.rs:285` and
`:323-327`) and would use a new value at once if anything could write one.

### What `close --role` reads today

- `close --role` and its `--dry-run` both call `remove::gate`
  (`crates/teamwire-close/src/remove.rs:22`, from `plan.rs:232` and
  `remove.rs:164`). For a started implementer the gate calls `verify_gap`
  (`remove.rs:47-53`, `:79-120`).
- `verify_gap` reads all verify records of the role (`remove.rs:92-95`) and
  takes the newest one at the member's current HEAD with
  `records.iter().rfind(|r| r.record.head_sha == head)` (`remove.rs:103`).
  It accepts the member when that record's verdict is `PASS` (`remove.rs:110`).
- A verify record row carries `recorded_at`
  (`crates/teamwire-store/src/verify.rs:24-28`, `VerifyRow`). It stores no
  check text (`crates/teamwire-store/src/schema/v1.sql:103-113`), and
  `verify/<role>.log` keeps only the newest run
  (`crates/teamwire-verify/src/check.rs:61-66`). So a `PASS` made with the
  old check at the same HEAD would satisfy close after an amend.
- The whole-team `close` never reads verify records. Only member removal
  does. `team_close.rs` and `plan.rs`'s whole-team path call no part of
  `verify_gap` (grep for `verify_records` in `crates/teamwire-close/src/`
  finds only `remove.rs:94`). So the new rule belongs in `verify_gap` only.
- No code today reads a marker at the start of a message body.
  `SPEC AMENDMENT:` is recognised by members reading the text, and only
  tests pin it (`crates/teamwire-cli/tests/preset.rs:1239-1270`). The amend
  marker is the first marker that code reads.

### Decisions

1. **Command shape.** `teamwire amend <team> --role R [--check CMD]
   [--owns PATTERN]... --reason TEXT`. It follows `verify` and `close`
   (`crates/teamwire-cli/src/args.rs:229-275`, `crates/teamwire-cli/src/cmd/verify.rs`):
   - `team` is a positional `TeamName`; `--role` names one member.
   - clap enforces the shape: `--role` and `--reason` are required, and an
     `ArgGroup` requires at least one of `--check` and `--owns`. A shape
     error is exit 3 (usage), as clap's errors are everywhere in the CLI.
   - `--owns` is repeated, one whole pattern per flag, with no
     `value_delimiter` (user answer; the other list flags split on commas,
     `args.rs:288`, which would break `crates/{a,b}/**`).
   - An empty or all-space `--reason` or `--check`, and an empty `--owns`
     pattern (`--owns ''`), are exit 3, through
     `fail("amend", ..., USAGE)`, as `verify` treats a bad team name
     (`cmd/verify.rs:24-27`).
   - A member session is refused with `refuse_member("amend")`
     (`crates/teamwire-cli/src/cmd/shared.rs:57`), exit 1, before the store
     opens, as `close` does (`cmd/close.rs:42`). A member that could amend
     its own check could make its own verify pass.
   - Refusals from the team's state are exit 1 with a named reason: the
     role is unknown, or the member is not `started` (`creating`, `failed`,
     `removed`), or the team is not open. `handoff` requires `started` the
     same way (`crates/teamwire-launch/src/handoff.rs:132-137`).
   - A store error is exit 6.
   - Delivery of the note is reported as `send` reports it: exit 4 when the
     member is `undelivered`, 5 when `unconfirmed`
     (`docs/api/cli.md:10-20`). The scope change is saved in both cases.
     stdout prints the scope line first, then `send`'s delivery line
     (`contracts/amend-cli.md` "Output").
   - A failure to rewrite the seat rules file after the store write is exit
     2 (partial), with the error and "run the same amend again". Exit 2
     wins over 4 and 5 in the same run, because a wrong seat rules file
     misleads every later handoff; both the scope line and the delivery
     line still print.
   - `amend` cannot clear the owns list (lead decision on the plan review):
     with one whole pattern per flag, an empty list has no flag shape, and
     an empty pattern is refused.
   - No `--json`, so `ui/src/shared/teamwire.ts` does not change.
   - Rejected: refusing an amend that changes nothing. The note still
     records the reason, and refusing it gains nothing.
2. **Where the code lives.** Each part goes to the crate that owns it, and
   the CLI runs the steps in order (`docs/ARCHITECTURE.md:61-67`;
   `teamwire-launch` does not depend on `teamwire-msg`,
   `crates/teamwire-launch/Cargo.toml:16-18`):
   - `teamwire-store`: a new setter `Tx::set_member_scope(role, check:
     Option<&str>, owns: &[String])` next to `set_member_place`
     (`member.rs:247`), with no event, as the other setters.
   - `teamwire-msg`: a new `send_with_write` that runs a caller's store
     write inside the transaction that stores the message
     (`crates/teamwire-msg/src/send.rs:58` `send_with`, `:299`
     `store_message`). `send_with` calls it with a no-op, so no caller
     changes. This makes the scope change and the note one transaction:
     either both are saved or neither is. Delivery stays after the commit,
     outside any transaction (AGENTS.md hard rule).
   - `teamwire-launch`: a new `pub fn rewrite_contract` that renders the
     seat rules from the stored member with `render_contract`
     (`crates/teamwire-launch/src/contract.rs:98`) and writes
     `<team dir>/contract/<role>.md` with `Platform::write_atomic`, through
     the existing `write_file` (`member_steps.rs:256-271`). The teamwire
     binary path is the CLI's `current_exe`, as `launch` passes it
     (`crates/teamwire-cli/src/cmd/launch.rs:69`).
   - `teamwire-cli`: `cmd/amend.rs` runs the checks, calls
     `send_with_write`, then `rewrite_contract`, then prints.
   - Rejected: storing the scope first and sending the note second. A crash
     between the two would leave a changed check with no note, and close
     would then accept an old `PASS`.
3. **The note.** Sender `lead`, type `note`, one recipient. The first line
   is `CHECK AMENDED: <reason>`, so `teamwire log`'s 300-character excerpt
   (`docs/api/cli.md:673-675`) shows the marker and the reason first. Then
   one line per changed field, old and new value; then the instruction to
   run the new check and record the result again. The full text is in
   `comms/<id>.md` and the member's inbox. Exact text in
   [contracts/amend-cli.md](contracts/amend-cli.md).
   - The note wakes a member that has recorded a result, as every note does
     (`send.rs:315-330`, ADR 0043; user answer). A member whose newest
     result is `handoff:` gets it held for the fresh session (REP-3).
   - The note uses no new event type and no schema change (user decision).
     The existing `message.sent` and, for a finished member,
     `member.woken` events are written as for any note.
4. **The close rule.** `verify_gap` (`remove.rs:79-120`) also reads the
   newest message with sender `lead`, type `note`, recipient the role, and a
   body that starts with `CHECK AMENDED:`, through `Reader::messages` with a
   `MessageFilter` (`crates/teamwire-store/src/message.rs:43-56`, `:243`).
   When the record it would accept has `recorded_at` at or before that
   note's `created_at`, close refuses:
   `<role>'s verify at HEAD <sha7> was recorded before the lead amended its
   check or owns (<note id>); run teamwire verify <team> --role <role>
   again`. The refusal goes through the same `unchecked` list, so
   `--unverified` overrides it, as it overrides every CLS-5 reason.
   - Together with decision 5, the rule as built is: a verify record counts
     for `close --role` only when it was recorded after the newest amend
     note and verify raised no `scope-changed` on it (a flagged record is
     not `PASS` anyway). This equals the user's "a verify that started
     after the amend" except for an amend that kept the same check and
     owns: a run that started before such an amend and was recorded after
     it counts, and it ran the very check the amend names, so nothing
     unproven passes. T008's guard `a_pass_recorded_after_a_same_scope_amend_counts`
     pins this.
   - A note that a user or the lead typed with `teamwire send` and that
     starts with the marker also counts. That only makes close stricter.
   - `close` picks the newest marker note itself from the role's lead
     notes, oldest first. It passes no `limit`, because `newest` acts only
     with `limit` (`crates/teamwire-store/src/message.rs:51-53`) and a limit
     could hide the newest marker note behind newer plain notes.
   - The whole-team `close` gets no rule, because it reads no verify record.
5. **A verify that ran across an amend.** Verify reads the member row at its
   start and stores its record after the check (`lib.rs:323-371`). A verify
   that started before the amend could store its record after the note,
   with the old check's verdict, and the close rule above would accept it.
   So verify re-reads the member's `check_cmd` and `owns` inside the write
   that stores the record. When they differ from the values it ran with, it
   adds the flag `scope-changed` ("the lead amended this member's check or
   owns while verify ran; run verify again") and the verdict is `FLAGGED`
   unless it was already `FAIL`. The flag is raised after the waiver step,
   as `head-moved` is (`lib.rs:328-341`), so `verify --waive` refuses it as
   an unknown flag (`lib.rs:242-244`). The compare lives in a new module,
   `crates/teamwire-verify/src/scope.rs`, because `lib.rs` already has 437
   lines.
   - This is the plan's own decision. It keeps the user's rule exact: a pass
     made with the old check never vouches for the new one.
6. **Who may be amended.** Every kind. The lead may message reviewers,
   closers and plan reviewers (`crates/teamwire-msg/src/rules.rs:228-233`,
   MSG-3), so the note reaches them as any member. Only a `started` member
   (decision 1).
7. **No MCP tool** (user decision). The MCP tool list stays the ten tools of
   `crates/teamwire-mcp/src/tools.rs:31`; the existing exact-list test
   `crates/teamwire-mcp/tests/tools.rs:21`
   `tool_list_is_the_ten_tools_of_the_doc` is the guard. It passes on the
   base and must still pass.

### Tests that fail without the fix

| Test (new unless marked) | File | Fails on the base because |
| --- | --- | --- |
| `amend_changes_the_check_verify_runs_and_notes_the_member` | `crates/teamwire-cli/tests/amend.rs` | `teamwire amend` is not a command: clap exits 3 with "unrecognized subcommand". Assert the exit is 0, then that `verify --role api` writes the new check's command line at the top of `verify/api.log` (`$ <check>   # in ...`, `check.rs:48-54`) and that `inbox` of `api` holds a `note` from `lead` starting `CHECK AMENDED:`. |
| `amend_replaces_owns_and_keeps_a_brace_glob_whole` | `crates/teamwire-cli/tests/amend.rs` | No command. After the fix: `--owns crates/a/ --owns 'crates/{b,c}/**'`, a commit under `crates/c/`, and verify raises no `outside-owns:` flag; `teamwire teams --json` shows exactly the two patterns. |
| `amend_refuses_without_a_change_or_a_reason_and_changes_nothing` | `crates/teamwire-cli/tests/amend.rs` | The base also exits 3 (unknown subcommand), so this case fails on its stderr assert, not its exit code. After the fix: exit 3 for no `--check`/`--owns`, no `--reason`, an empty `--reason`, `--check ""`, `--owns ""`; exit 1 for an unknown role, a `creating`, `failed` or removed member, and a team not open; exit 1 `refused.identity` with `TEAMWIRE_ROLE` set; in each case the member row and `log` are unchanged. |
| `an_amend_to_a_member_with_no_live_process_exits_4_and_keeps_the_change` | `crates/teamwire-cli/tests/amend.rs` | No command (exit 3, not 4). After the fix: the scope line, then `send`'s delivery line; the new owns are saved. |
| `a_seat_rules_failure_exits_2_even_when_the_note_is_undelivered` | `crates/teamwire-cli/tests/amend.rs` | No command (exit 3, not 2). After the fix: exit 2 wins over 4. |
| `the_deckh_f14_sequence_ends_with_close_role_without_unverified` | `crates/teamwire-cli/tests/amend.rs` | No command: the amend step exits 3, not 0. Covers SC-002 end to end on the fakes. |
| `an_amend_note_wakes_a_done_member` | `crates/teamwire-cli/tests/amend.rs` | No command (exit 3, not 0). After the fix: a `member.woken` event for `api` (spec US1 scenario 8). |
| `amend_rewrites_the_seat_rules_and_a_handoff_reads_them` | `crates/teamwire-cli/tests/amend.rs` | No command. After the fix: `<home>/teams/alpha/contract/api.md` holds the new check under "Your check", and a `teamwire handoff alpha --role api` after `done --summary "handoff: ..."` starts an agent whose `--append-system-prompt-file` names a file that holds the new check (the shape of `crates/teamwire-cli/tests/handoff.rs:217`). |
| `amend_leaves_earlier_verify_records_unchanged` | `crates/teamwire-cli/tests/amend.rs` | No command. After the fix: the verify record from before the amend has the same head, verdict, exit code and log hash (`teamwire export` rows) as before it. |
| `an_amend_note_makes_an_older_pass_not_count` | `crates/teamwire-close/tests/remove.rs` | On the base, `verify_gap` reads no message, so a `PASS` at HEAD followed by a stored `CHECK AMENDED:` note from `lead` still removes the member. The test expects the refusal text naming the note. Then a `PASS` at a later clock time removes it. Uses the file's `verify_at` helper (`remove.rs:44`) and `rig.clock`. |
| `a_record_at_the_same_time_as_the_note_does_not_count` | `crates/teamwire-close/tests/remove.rs` | Same cause: on the base the `PASS` counts. Pins the "at or before" boundary with equal timestamps. |
| `a_pass_recorded_after_a_same_scope_amend_counts` | `crates/teamwire-close/tests/remove.rs` | **Guard test**: passes on the base, where close reads no note. Pins that a record recorded after a same-scope amend note counts (decision 4). |
| `a_note_without_the_marker_or_from_a_member_changes_nothing` | `crates/teamwire-close/tests/remove.rs` | **Guard test**: passes on the base, because the base reads no note at all. It pins that only a lead note with the marker counts. |
| `scope_changed_flags_a_record_whose_member_was_amended_mid_run` | `crates/teamwire-verify/src/scope.rs` (unit test) | The module does not exist. The test gives the record step a member snapshot whose check differs from the stored row and expects the `scope-changed` flag and verdict `FLAGGED`; with equal values, no flag. |
| `scope_changed_cannot_be_waived` | `crates/teamwire-verify/tests/verify.rs` | **Guard test**: passes on the base, because `waive` accepts only flags of the diff (`lib.rs:242-244`) and `scope-changed` is not one. Shape of `verify.rs:1285-1312` (`head-moved`): `waive(..., "scope-changed", ...)` returns `UnknownFlag`. It keeps the flag unwaivable after the fix. |
| `set_member_scope_updates_check_and_owns_only` | `crates/teamwire-store/src/member.rs` (unit test) or `crates/teamwire-store/tests/` | The setter does not exist. |
| `rewrite_contract_writes_the_new_scope` | `crates/teamwire-launch/tests/contract_rewrite.rs` (new) | The function does not exist; with T010's stub, the file keeps the old check. |
| `send_with_write_stores_the_message_and_the_write_together` | `crates/teamwire-msg/tests/send.rs` | The function does not exist. Asserts that a write closure that errors leaves no message and no delivery row, and that a closed team runs no write. |
| `the_codex_skill_names_amend` | `crates/teamwire-harness-codex/src/lead_setup/plugin.rs` (unit test, next to `:308-320`) | The shared `SKILL.md` names no `teamwire amend`. |
| `the_pi_skill_names_amend` | `crates/teamwire-harness-pi/src/lead_setup.rs` (unit test) | Same. |
| `the_skill_names_only_commands_and_tools_that_exist` (existing) | `crates/teamwire-cli/tests/preset.rs:484` | **Guard**: unchanged; it fails if the skill names `teamwire amend` before the command exists, so the command and the skill text go in one task phase. |
| `tool_list_is_the_ten_tools_of_the_doc` (existing) | `crates/teamwire-mcp/tests/tools.rs:21` | **Guard**: passes on the base and after; it fails if anyone adds an MCP tool for amend. |

Each "fails on the base" row was checked by reading the base code: the
command, the setter, the send function, the scope module and the flag do
not exist on `7e78c1a`, and `verify_gap` reads no message
(`remove.rs:79-120`).

### Guidance and docs that change in the same phase

| File | Change |
| --- | --- |
| `docs/api/cli.md` | New section `### teamwire amend <team> --role R [--check CMD] [--owns PATTERN]... --reason TEXT` after `verify` (line 429). Add `amend` to the lead-only list in "Lead commands" (lines 24-35). In "Exit codes", the exit-2 row (line 16) gains amend's partial case: the change saved, the seat rules file not rewritten. In `close` (line 462): `--role` also refuses after an amend until a verify recorded after the amend note. In `verify` (line 429-461): the `scope-changed` flag. |
| `docs/api/core.md` | The new public items: `Tx::set_member_scope`, `teamwire_msg::send_with_write`, `teamwire_launch::rewrite_contract`, the `scope-changed` flag. |
| `docs/requirements.md` | New rows: LCH-33 (amend: shape, refusals, one transaction, seat rules rewrite, shell only), VER-9 (verify uses the amended scope; `scope-changed`), CLS-9 (a record at or before the newest amend note does not count). |
| `docs/ARCHITECTURE.md` | Data model table (line 100): `contract/<role>.md` is written by launch and amend. `members` row (line 115): `check_cmd` and `owns` are set by launch and changed by amend. |
| `docs/failure-modes.md` | New row: "A member's check is wrong" → `teamwire amend`, with the test names above. |
| `docs/adr/0046-amend-changes-a-running-members-check-and-owns.md` | New ADR: decisions 1-7 above. Supersedes nothing; it adds to ADR 0009 (verify) and CLS-5 without changing their points. |
| `README.md` | Command table (lines 255-262): add the `teamwire amend` row. |
| `examples/skills/teamwire/SKILL.md` | After the paragraph on a `FLAGGED` or failing implementer (lines 819-825): a member whose check or owns is wrong gets `teamwire amend`, then verify again; an old pass does not count. Codex and pi leads read this text too (`crates/teamwire-harness-codex/src/lead_setup/plugin.rs:62`, `crates/teamwire-harness-pi/src/lead_setup.rs:95-100` wrap the shared body). |
| `examples/skills/teamwire/reference.md` | "Other commands" table (line 286): an `amend` row. "Verify flags" (line 203): `scope-changed`. |
| `examples/skills/afk/SKILL.md` | Line 149 (`teamwire verify <team> --role <owner>` after each fix round): name `teamwire amend` for a wrong check. The AFK skill may name only commands that exist (`preset.rs:443`), so this goes after the command. |

Kept as they are, with the reason:

- `examples/skills/codex-lead.md`, `examples/skills/pi-lead.md`: they hold
  only what differs per harness, and neither names `verify` or `close`.
  The shared body carries `amend` to both.
- `crates/teamwire-launch/contracts/*.md`: no seat text names lead
  commands, and members must not run `amend`.
- `docs/api/mcp.md`, `crates/teamwire-mcp/`: no tool (decision 7).
- `ui/src/shared/teamwire.ts`: no `--json` output changes shape;
  `teamwire teams --json` already reads `owns` from the store
  (`ui/src/shared/teamwire.ts:213`).

## US2: removing spec-kit before the pull request (F14)

### Root cause (verified)

- The `speckit` preset's implementer check starts with
  `test -s specs/NNN-name/tasks.md` (`examples/presets/speckit.json:74`;
  the BUILD rule at `:10` says the check "needs `tasks.md`").
- No rule in the preset, the skill or the reference says where a commit
  that removes spec-kit goes (grep for "remov" with "spec-kit" in
  `examples/presets/speckit.json`, `examples/skills/teamwire/SKILL.md`,
  `examples/skills/teamwire/reference.md` finds only the doctor actions at
  `reference.md:374-379`).
- When that commit is on the implementer's own branch, every later verify
  runs the check on a tree without `tasks.md` and gets exit 1, verdict
  `FAIL` (`crates/teamwire-verify/src/lib.rs:342-345`). A `FAIL` comes from
  the check's exit code, which no waiver changes. `close --role` then
  refuses the implementer (`crates/teamwire-close/src/remove.rs:111-118`),
  and only `--unverified` gets past it. Team deckh hit exactly this
  (`deckh/impl` from `f589691d30`; last pass `2552d6ebd3`).

### Decision

Text only, plus US1's `amend` (user decision 3: no "check retired" state).

- A new rule in the preset's `_rules`, after CLOSE, named `REMOVE SPEC-KIT`:
  when the user wants spec-kit's files out of the pull request, the lead
  cuts the pull request branch from the implementer's last verified head
  (the head of its newest `PASS`) and makes the removal commit there, never
  on the implementer's own branch, so the implementer's check keeps passing
  on its branch. When the removal is already on the implementer's branch,
  the lead runs `teamwire amend <team> --role <role> --check CMD --reason
  TEXT` with a check that does not test `tasks.md`, such as the test
  command alone, then runs `teamwire verify <team> --role <role>` again.
- The same text, in the skill reference's "Spec-kit features" section,
  under "The flow" (`reference.md:420`), so `speckit_section()`
  (`crates/teamwire-cli/tests/preset.rs:1089`) holds it.
- `SKILL.md`: one sentence where it points to the spec-kit flow, naming
  the rule. The Codex and pi leads read the shared text.

### Test that fails without it

| Test | File | Fails on the base because |
| --- | --- | --- |
| `the_speckit_preset_puts_the_removal_on_the_pr_branch_and_names_amend` | `crates/teamwire-cli/tests/preset.rs`, next to `:1852` | `rules_text("speckit")` holds neither "pull request branch", "last verified head", "never on the implementer's own branch" nor "teamwire amend". The test asserts all four phrases in the preset rules and in `speckit_section()`. |

## US3: the role table and the seat rules' git bans (F15)

### Root cause (verified)

The seat rules each kind receives are the concatenation in
`render_contract` (`crates/teamwire-launch/src/contract.rs:98-121`):
`head.md`, the kind's text, the scope, the talking rules, the ask rules,
`how_to_work.md` and `finish.md`. `how_to_work.md` goes to every kind.

| Kind | git bans its seat rules state | Role table "May not" today (`examples/skills/teamwire/SKILL.md`) | Missing |
| --- | --- | --- | --- |
| implementer | `git worktree add`, `remove`, `move`, `prune` (`how_to_work.md:29`); `git stash` (`kind_implementer.md:38`); `git reset --hard`, `git checkout -- .` (`kind_implementer.md:40-41`) | `git stash`, `git reset --hard`, `git checkout -- .` (line 284) | the four worktree commands |
| reviewer | the four worktree commands (`how_to_work.md:29`); `git worktree add` (`kind_reviewer.md:9`) | none (line 285) | the four |
| plan_reviewer | the four (`how_to_work.md:29`) | none (line 286) | the four |
| investigator | the four (`how_to_work.md:29`) | none (line 287) | the four |
| closer | the four (`how_to_work.md:29`); `git worktree add` (`kind_closer.md:52`) | `git worktree add` (line 288) | `remove`, `move`, `prune` |

No test compares the table with the seat rules. Team deckh's lead read the
implementer row, saw no worktree ban, and wrote a brief its member could not
follow.

### Decisions

1. **How a test finds a ban.** In each kind's rendered seat rules, with
   whitespace joined to single spaces, split into sentences at ". " and at
   ": ". A sentence that contains `Never run`, `Never use`, `Do not use` or
   `The same goes for` bans every backticked span in it that starts with
   `git `. Other backticked `git ...` spans are commands a seat is told to
   run (the closer's `git clone`, `git reset --hard` and `git clean -fd` in
   its clone, `kind_closer.md:30-37`) or part of a story (the implementer's
   "At Bun" sentence, `kind_implementer.md:41-42`), and are not bans.
   - `Do not pass` is left out on purpose: the closer's "Do not pass `-x`
     to `git clean`" (`kind_closer.md:39-40`) bans a flag of a command the
     closer must run, and counting it would ban `git clean` itself. The
     role table does not list it.
2. **Spell out the worktree commands.** `how_to_work.md:29` changes from
   ``Never run `git worktree add`, `remove`, `move` or `prune` `` to
   ``Never run `git worktree add`, `git worktree remove`, `git worktree
   move` or `git worktree prune` ``, so each ban is a whole `git` span that
   the rule above finds. Members read the same ban, written in full.
3. **The role table** lists, in each kind's "May not" cell, each ban in
   backticks, in full: "Run `git worktree add`, `git worktree remove`,
   `git worktree move` or `git worktree prune`." for every kind; the
   implementer row keeps "Run `git stash`, `git reset --hard` or
   `git checkout -- .`."
4. **The test** compares, for each kind in `MemberKind::ALL`, the set of
   bans from decision 1 with the set of backticked `git ` spans in that
   kind's row of the role table. It reads only the role table: the rows
   after its header line ``| Kind | Does | May not | Set `mode` |``
   (`SKILL.md:282`) and its separator, up to the first line that does not
   start with `|`. It never reads another table. The preset-shapes table
   also has a row that starts ``| `closer` |`` (`SKILL.md:123`), with
   three cells and no "May not"; a lookup of the first matching row would
   read that one (plan review F1). It fails on a ban missing from the row and
   on a row span the seat rules do not ban, and names the kind and the
   command.

### Test that fails without it

| Test | File | Fails on the base because |
| --- | --- | --- |
| `the_role_table_lists_every_git_ban_of_each_seat` | `crates/teamwire-launch/tests/role_table_git_bans.rs` (new) | On the base, the implementer, reviewer, plan_reviewer and investigator rows lack `git worktree add`. The closer row matches on the base, because the base's `remove`, `move` and `prune` spans do not start with `git `. It reads `examples/skills/teamwire/SKILL.md` from `CARGO_MANIFEST_DIR/../../examples`, as `crates/teamwire-cli/tests/preset.rs:376-383` does. On the base, `how_to_work.md`'s bare `remove`, `move`, `prune` spans are not `git` spans, so the test finds only `git worktree add` for them; the failure on the base is still the missing `git worktree add` in four rows. |
| `a_row_that_names_a_git_command_no_seat_bans_fails` | same file | **Guard test** of the test file's compare helper: passes on the base once the helper exists. A row text with `git push` added against the implementer's bans fails with the kind and `git push`; it proves direction two of FR-016 can fail. |
| `a_closer_row_in_another_table_is_not_read` | same file | **Guard test** of the row lookup: passes on the base once the helper exists. A three-column table with a `closer` row before the role table is skipped. |

### Existing test whose expectation changes

- `crates/teamwire-launch/tests/contract_worktree_rule.rs:22`
  `every_kind_is_told_not_to_administer_worktrees` asserts the old text
  ``Never run `git worktree add`, `remove`, `move` or `prune` ``. It changes
  to the spelled-out text of decision 2. Its other two asserts
  (`never\n  write into another member's directory` and
  `report blocked to the\n  lead`) stay. No assert is removed.

No other test pins the changed text: grep for ``remove`, `move` `` outside
`target/`, `.worktrees/` and `bench/` finds only this test. A third test
pins a line break in the same bullet:
`crates/teamwire-launch/src/contract/tests_findings.rs:296`
(`ask the parent as\n  HOW THIS TEAM WORKS TOGETHER says`). So the fix
splits only line 29 into two lines and re-flows nothing else; all three
pinned breaks stay unchanged. A greedy re-flow at the file's width breaks
two of them (plan review F6, checked in scratch by the plan reviewer).

### Guidance that changes in the same phase

- `crates/teamwire-launch/contracts/how_to_work.md:29-30` (decision 2).
- `examples/skills/teamwire/SKILL.md:284-288` (decision 3).
- Kept: `examples/skills/teamwire/reference.md` has no role table.
  `docs/` has no copy of the table.

## Grep of every name the work adds or changes

| Name | Hits outside `specs/`, `target/`, `bench/` | Where it goes |
| --- | --- | --- |
| `amend` (command) | none (only "amend the contract" in test strings of `crates/teamwire-verify/tests/report_check.rs:101-106` and `verify.rs:141,428`, unrelated) | new everywhere listed above |
| `check_cmd` | `crates/teamwire-store/src/member.rs`, `schema/v1.sql`, `schema/v9.sql`, `docs/ARCHITECTURE.md:115` | store setter (US1); ARCHITECTURE row (US1). Schemas unchanged. |
| `owns` in `--json` | `ui/src/shared/teamwire.ts:213` (`teams --json`) | unchanged shape |
| `contract/<role>.md` | `docs/ARCHITECTURE.md:100`, `member_parts.rs:36-40`, `harness.rs:357` | ARCHITECTURE row (US1) |
| `verify_gap` | `crates/teamwire-close/src/remove.rs:52,81` | US1 close rule |
| `CLS-5` | `docs/requirements.md:71,255`, `crates/teamwire-close/src/*.rs` comments | new CLS-9 row; CLS-5 text unchanged |
| `head-moved` (shape for `scope-changed`) | `docs/api/cli.md:440`, `reference.md:212,224`, `docs/adr/0045...:45` | `scope-changed` next to it in cli.md and reference.md; ADR 0045 not edited |
| `git worktree add` | `how_to_work.md:29`, `kind_closer.md:52`, `kind_reviewer.md:9`, `SKILL.md:288`, `contract_worktree_rule.rs:22` | US3 |
| `test -s specs/` | `speckit.json:10,41,74` | unchanged; US2 adds the REMOVE SPEC-KIT rule |
| `teamwire --help` list | `crates/teamwire-cli/tests/preset.rs:385-395` `subcommands()` | gains `amend` with no test change |
