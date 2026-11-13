# ADR 0046: `teamwire amend` changes a running member's check and owns

Status: accepted, 2026-11-13. Supersedes nothing. It adds to ADR 0009
(verify) and to CLS-5 (removing a member) without changing their points.

## Context

Launch was the only code that wrote a member's check and owns. The store set
both once, when it inserted the member row, and launch rendered both once
into the member's seat rules file, `<team dir>/contract/<role>.md`. A
handoff or a reopen starts the fresh session from that file and never wrote
it again. No command changed either value.

Team `deckh`'s lead wrote an implementer's check that ran every test file
instead of two. One flaky file outside the team's work failed verify. The
closer found the right command, but the wrong one stayed for the rest of
the run (finding F13). The same team later removed spec-kit's files on the
implementer's own branch, and every verify after that failed on the missing
`tasks.md` (finding F14). A `FAIL` takes no waiver, so the lead had to close
the implementer with `--unverified`. The user agreed the decisions below on
2026-11-13.

## Decisions

1. **The command.**
   `teamwire amend <team> --role R [--check CMD] [--owns PATTERN]... --reason TEXT`
   changes one `started` member's check, owns or both.
   - `--check` replaces the check command. `--owns` is repeated, one whole
     pattern per flag, and the patterns given replace the whole owns list.
     A value is never split on commas, so `crates/{b,c}/**` stays one
     pattern. At least one of the two is required, and so is `--reason`.
   - Exit 3 for a usage error: a missing `--role`, `--reason` or change, an
     empty or all-space `--reason` or `--check`, or an empty `--owns`
     pattern. Amend cannot clear the owns list.
   - Exit 1, with nothing changed, for a member session
     (`refused.identity`), an unknown role, a member not `started`, or a
     team not open. A `creating` or `failed` member's scope is still being
     written by launch, and a removed member has no session to tell.
   - Exit 4 or 5 when the note is `undelivered` or `unconfirmed`; the scope
     is saved. Exit 2 when the scope and the note are saved but the seat
     rules file could not be rewritten; running the same amend again
     finishes it. Exit 2 wins over 4 and 5, because a wrong seat rules file
     misleads every later handoff. Exit 6 for a store error.
   - A member that could amend its own check could make its own verify
     pass, so amend is a lead command, refused from a member session before
     the store opens.
2. **The note is the record, stored with the change.** Amend sends the
   member a `note` from `lead`. Its first line is `CHECK AMENDED: <reason>`,
   so `teamwire log`'s excerpt shows the marker and the reason first. Then
   one line per changed field with its old and new value (`check: <old or
   (none)> -> <new>`, `owns: [<old>] -> [<new>]`), then the instruction to
   run the new check and record the result again.
   `teamwire_msg::send_with_write` runs `Tx::set_member_scope` inside the
   transaction that stores the note, so both are saved or neither is.
   Delivery runs after the commit, as for every message. The note wakes a
   member that has recorded a result, and it is held for the fresh session
   of a member whose newest result is a `handoff:`, as every note is
   (ADR 0043).
3. **No event type and no schema change.** `set_member_scope` writes no
   event, like the store's other member setters. The note and its
   `message.sent` event, and `member.woken` for a finished member, are the
   trace. The store stays at schema version 11.
4. **The close rule.** `close --role` finds the member's newest message from
   `lead`, of type `note`, whose body starts with `AMEND_MARKER`
   (`CHECK AMENDED:`), and counts no verify record at the member's HEAD that
   was recorded at or before that note, even a `PASS`. The refusal says
   verify must run again, and `--unverified` overrides it as it overrides
   every CLS-5 reason. A verify record counts for `close --role` only when
   it was recorded after the newest amend note and verify raised no
   `scope-changed` on it. That equals "a verify that started after the
   amend" in every case but one: an amend that kept the same check and
   owns. There a run that started before the amend ran the very check the
   amend names, so nothing unproven passes. A note that someone typed with
   `teamwire send` and that starts with the marker also counts, which only
   makes close stricter. The whole-team close reads no verify record and is
   unchanged.
5. **`scope-changed`.** Verify reads the member's check and owns at the
   start of a run and stores the record after the check. It re-reads them
   in the transaction that stores the record. When they differ from the
   values it ran with, the record gets the flag `scope-changed`, and a
   `PASS` becomes `FLAGGED`. The flag is raised after the waiver step, as
   `head-moved` is, so no waiver can clear it. So a pass made with the old
   check never vouches for the new one, even when it is stored after the
   note.
6. **The seat rules file is rewritten.** After the transaction, amend renders
   the member's seat rules again from its stored team, role, kind, check and
   owns, with `teamwire_launch::rewrite_contract`, and writes
   `<team dir>/contract/<role>.md` with `Platform::write_atomic`. All three
   harnesses read that file at handoff and reopen, so a fresh session reads
   the new scope.
7. **Shell only.** Amend is no MCP tool: the lead MCP server keeps its ten
   tools. It has no `--json` and no `--dry-run`.

## Rejected

- Storing the scope first and sending the note in a second step. A crash
  between the two would leave a changed check with no note, and close would
  then accept an old `PASS`.
- A stored start time on each verify record, so close could compare start
  times. That needs a schema step; `scope-changed` gives the same guarantee.
- A "check retired" member state for the spec-kit removal of F14. The
  `speckit` preset now says where that removal goes, and amend is the way
  out when it is already on the implementer's branch.
- Refusing an amend that changes nothing. The note still records the
  reason, and a refusal gains nothing.

## Consequences

- A wrong check is fixed with one command and one verify, with no member
  restart and no `--unverified`.
- `crates/teamwire-cli/tests/amend.rs` and `amend_flow.rs` replay the
  command end to end on the fakes, deckh's F14 sequence included.
  `crates/teamwire-close/tests/remove.rs` holds the close rule,
  `crates/teamwire-verify/tests/verify.rs` and `src/scope.rs` hold
  `scope-changed`, and `crates/teamwire-launch/tests/contract_rewrite.rs`
  holds the rewrite.
