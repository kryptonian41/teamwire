# ADR 0059: A sent reprompt and a finished generation are proven, not assumed

Status: accepted, 2026-11-20.

Supersedes these sentences, and no others:

- ADR 0003, point 8, the sentence "each id is reminded at most once", as it
  applies to the reprompt that started a member's current generation.
  Decision 2 below adds one more hold for that reprompt.
- ADR 0021, decision 1, the sentence "A longer body keeps the pointer", as it
  applies to a reprompt from the lead or the user. Such a reprompt keeps the
  pointer but loses the excerpt (decision 1 below).

Every other point of ADR 0003 and ADR 0021 still holds. A note, an answer, a
finding and every peer message keep the 300-character excerpt.

## Context

A conformance run on 2026-11-20 led a team from a pi lead with pi members.
The archives of the teams `pismokevgb`, `piprotoeg` and `picodereg` show five
failures. Each one let an agent take an unfinished step for a finished one.

1. A lead's reprompt of 563 characters (`pismokevgb`, `m003`) reached the
   member as a 300-character excerpt. The excerpt read as a whole task. The
   member did the part it could see, answered in chat, and stopped with no
   report edit and no `done` call. The sentence that asked for a report and
   a recorded result was past the cut. A replay in `piprotoeg` failed the
   same way with a 460-character reprompt, and passed with a 164-character
   one.
2. The member's tool hook had already reminded it of the unread reprompt
   once (`delivery.reminded`, with no ack). ADR 0003 point 8 reminds each id
   once, so the Stop hook let the member stop with the reprompt unread. The
   report check at stop needs a pending result, so a member that never
   called `done` got no report hold either.
3. A pi member was asked to read its inbox and record its result through
   the MCP tools, and its transcript shows it used the shell for both. The
   member extension registered the MCP server without `exposure`, and pi
   1.5.0's default is `codemode`, so the `team_*` tools were never declared
   to the model as tools. The lead extension already passed
   `exposure: "direct"`. The shell commands worked, so the run proved
   delivery of the status, not that the member's MCP tools work.
4. A fresh pi lead adopted `piprotoeg` and was asked to reprompt one role,
   check generation 4 and its marker, then close. Each of its `team_send`
   calls named `lead`, `members` or no recipient, and each was refused. The
   lead read the last validation error as success, ran a deep verify whose
   pane checks were SKIPPED, and ran a bare `teamwire close`. Close checked
   busy members and uncommitted work, found none, and archived a team whose
   member was `blocked` in generation 3. The archive has no generation 4 and
   no marker. The `team_members` text showed only a count of members.
5. A pi closer in `picodereg` ran `git checkout -b picodereg/writer || true`
   in its clone. That command made a new local branch at the base commit
   instead of checking out the writer's commit, and the `|| true` hid the
   difference. The closer then reported the writer's new file missing. Its
   report held literal `$(...)` placeholders. The writer's declared check
   also passed on the unchanged base, so that check proved nothing about the
   file.

## Decision

1. **A long reprompt is a pointer with no excerpt.** A reprompt from the lead
   or the user whose body is longer than 300 printable characters is typed as
   one line that names the message id and the sender, gives the length, and
   says to read the full instruction first, with the inbox tool and command.
   The line says the reprompt starts a new generation that needs an explicit
   result. The line carries none of the body. A body of 300 characters or
   fewer is typed whole, as before. `reprompt_pointer` in
   `crates/teamwire-deliver/src/lines.rs` builds the line. Peer messages and
   messages from another team are unchanged.
2. **One corrective stop hold per generation.** At a member's stop, the
   reprompt that started its current generation is checked once more. The
   stop is held when that reprompt is still unread and its line did not carry
   it whole, or when it was read and the generation has no result. A
   reminder after a tool call does not count as reading. The hold is recorded
   as `corrective` on the `member.held` event, at most once per generation
   in total, so a member that still does not finish stops on its next try
   and stays visible to the lead. The hold never records a result, and only
   the member's own `done` does. It is skipped when the team is not open,
   when the member waits for an answer to its own question asked in this
   generation, for a message held for handoff or still pending, and for a
   generation that a wake, a handoff or an undone reprompt started. It obeys
   REP-5: one hold per stop, none when the harness says a hook already held
   this stop. `crates/teamwire-report/src/reprompt_hold.rs`.
3. **A pi member's MCP server declares its tools directly.** The member
   extension registers the server with `exposure: "direct"` and a
   description, as the lead extension already did (ADR 0030 decision 4).
   The tools then reach the model as `mcp__teamwire__team_*`, the same names
   as in Claude Code. The CLI commands stay available, and the contracts
   still name both. `crates/teamwire-harness-pi/src/extension.rs`.
4. **Tool text says who, in which state and generation.** The text of
   `team_members` and `team_status` names each role with its state and
   generation, because a tool result may reach the model as text only. A
   reprompt's receipt reads `m012 reprompt sent: api: handed_off, now
   generation 4`. A refused `team_send` or `team_resend` starts with
   `NOT SENT`, says no message was stored and no generation started, and
   that only a reply naming a message id and its recipient is a receipt. The
   structured content keeps the full refusal. A send the server could not
   parse is recorded as `message.refused` with code `invalid_argument`, so
   the store holds a trace of it (MCP-2). The `team_inbox` text carries each
   message whole.
5. **A whole-team close checks outcomes.** `teamwire close <team> --expect
   role:generation:status`, once per result the lead needs, refuses unless
   that member is in that generation and its newest result is in that
   generation with that status. An unknown or removed role refuses. The
   status is one a member records, `done`, `blocked` or `failed`: a
   `SKIPPED` check or a clean verify is not a result. Without `--expect` or
   `--abandon`, a close also refuses each started member whose current
   result is `blocked` or `failed`, or whose current generation has no
   result after an older one had one. It also names the lead's newest send
   when that send was refused and no later message was stored. An `--expect`
   that names a member accepts that member's outcome only.
   `--abandon WHY` closes the team as it is, on purpose, and records the
   reason as a close step. The two flags do not combine, and neither works
   with `--role`. The checks run in the plan, in `--dry-run`, and once more
   under the close lock before the archive, so a result that changed since
   the plan stops the close before anything is removed.
   `crates/teamwire-close/src/expect.rs`.
6. **A closer pins its clone to the verified commit.** `teamwire verify
   <team> --role R --pinned-in DIR` runs no check. It exits 0 only when DIR
   is the root of a git checkout, DIR's HEAD is the head of R's newest
   verify record, and R's own current HEAD is that head too. It refuses a
   missing or out-of-date record, a member whose current commit cannot be
   read, and a DIR that is a folder inside a checkout. A removed member still
   pins against its record. A member with no record and no worktree has
   nothing to pin, and the command says so. The closer's contract and the
   closer preset tell it to check out the sha that verify printed, with
   `git checkout --quiet --detach <sha>`, to pin, to write literal commands
   and exit codes, and to show that a new file is there at the pinned commit
   and absent on the base. `crates/teamwire-verify/src/pin.rs`.
7. **The contracts and the lead skill say what counts.** Every member
   contract says a long reprompt arrives as a pointer, a reminder is not
   reading, and an answer in chat or a finding is not a result. The lead
   skill says a send succeeded only when its receipt names a message id and
   the target role. A generation succeeded only when `teamwire status` shows
   its result with the status the lead needs. A message count, a clean
   verify and a `SKIPPED` check are not task success.

## Why not the alternatives

- **A longer excerpt.** Any cut can end at a sentence boundary that reads as
  the end of the task. Only a line with none of the task cannot be taken for
  all of it.
- **Hold until a result exists.** A member that cannot finish would be held
  at every stop and loop. One hold per generation corrects the observed
  miss, and the member's own `done` stays the only result.
- **Make the outcome check an opt-in flag only.** The lead in this run ran a
  bare close. A flag it did not pass would have changed nothing, so the bare
  close refuses in the observed case.
- **Refuse a member that never reported.** Many working teams close with a
  plan reviewer or an investigator that recorded nothing. Refusing them would
  break every such close, and nothing of their story is in a result.
- **Pin by branch name.** The failing closer used a branch name, and in its
  clone that name meant the base. Only the sha from the verify record names
  the commit verify judged.

## Consequences

- A bare close keeps working for a team whose members all finished `done`,
  or never reported. A close now refuses a team with a blocked, failed or
  unfinished current generation, until the lead names the result with
  `--expect` or closes on purpose with `--abandon`. A script that closed such
  teams before must add one of the two flags. That is the one change that
  can break an existing caller, and it is deliberate.
- A reprompt over 300 characters costs the member one inbox read before it
  acts. A long note keeps its excerpt.
- A member that ignores the corrective hold stops on its next try, with no
  result in its generation. The watch and `teamwire status` keep showing
  that generation as unfinished, and a bare close refuses it.
- The store gains no table and no column. The corrective hold is a field on
  the existing `member.held` event, and `--expect` reads existing results,
  so existing team databases work unchanged.
- No identity or permission changes. `--abandon` needs the lead's own
  command, as every close does.

## Where it is tested

| Decision | Tests |
| --- | --- |
| 1 | `crates/teamwire-deliver/tests/lines.rs` (300/301 boundary, peer and other-team controls) |
| 2 | `crates/teamwire-report/tests/reprompt_hold.rs` |
| 3 | `crates/teamwire-harness-pi/src/extension.rs` unit test |
| 4 | `crates/teamwire-mcp/tests/receipts.rs` |
| 5 | `crates/teamwire-close/tests/outcome.rs`, `tests/e2e/tests/checked_close.rs` |
| 6 | `tests/e2e/tests/closer_pin.rs`, `crates/teamwire-cli/tests/preset.rs` |
| 7 | `crates/teamwire-launch/src/contract/tests_pin.rs`, `crates/teamwire-cli/tests/lead_guidance.rs` |
