# ADR 0040: The human user is a sender, proven by a key file

Status: accepted, 2026-11-06. Extends ADR 0008 decision 3 and its
consequence "The user typing `teamwire send --from lead` in a plain terminal
is refused". Neither is replaced: the lead still proves itself only by its
harness session, and the user still cannot speak as the lead. The user gets
a sender of its own, `user`.

## Context

ADR 0008 lets three kinds of caller send: a member by its token, the lead by
its harness session, and an agent of another team through that team's store.
The human user has no sender. A user who wants to tell a member something
types into the lead's session, or into the member's pane, where teamwire
records nothing.

The dashboard (`ui/`) now acts on a team from the browser. Its server is not
a member and not the lead's session. Copying the lead's session id into the
server would let it pass ADR 0008's lead proof. That is forgery of the
lead's identity, and the user decided on 2026-11-06 to rule it out. The user
asked instead for a sender that is plainly the user.

## Decision

1. **A third proven sender, `user`.** `Actor::User` is its own variant. Its
   string form is `user`, and in a sender field the text `user` reads as the
   user. `Recipient` has no `User` variant, because nobody sends to the
   user. Launch refuses a spec role named `user` with `reserved_role`, as it
   refuses `lead`. `Role::parse` still accepts `user`, so a store that
   already holds a member of that name still reads.
2. **The user key.** `<home>/user.key` holds 64 lowercase hex characters
   (256 random bits) and one `\n`. `<home>` is the teamwire home the command
   resolved: `--home`, else `$TEAMWIRE_HOME`, else `~/.teamwire`.
   `teamwire user-key` makes the file when it is missing or empty. It
   creates an empty file with mode 0600, then writes the key into it through
   `Platform::write_atomic`, which keeps that mode. An existing key is kept.
   A symlink or any other non-regular file at the path is refused, never
   followed. The command prints the file's absolute path and `\n`, never the
   key, and exits 0, or 6 when the file cannot be written or read.
3. **The user's proof.** `teamwire send --from user` is accepted only when
   all of these hold, and is `refused.identity` otherwise:
   - None of `TEAMWIRE_TOKEN`, `TEAMWIRE_TOKEN_FILE`, `TEAMWIRE_ROLE`,
     `TEAMWIRE_TEAM` is set to a non-empty value. A token file that is set
     but cannot be read still counts as set, so the proof tests the
     variables themselves and not the token that `caller_token` reads.
   - `TEAMWIRE_USER_KEY` is set.
   - That value and the file's text, each with ASCII white space trimmed
     from both ends, are equal. The file must not be empty. The compare
     hashes both with SHA-256 and compares the digests in constant time.

   A missing or unreadable key file proves nothing. `teamwire resend` claims
   `user` when the caller holds `TEAMWIRE_USER_KEY` and no member token, and
   the same proof decides. The user may resend only its own messages, as any
   sender may. The proof lives in `prove_actor`, the one proof path.
4. **What the user may send** (enforced by the message rules, not by the
   page):

   | To | Types |
   | --- | --- |
   | an implementer or investigator of the team | `reprompt`, `note` |
   | `lead` | `note` |

   The user sends no `answer`, `question`, `finding` or `notice`
   (`type_not_allowed`). The question-and-answer flow between agents stays
   theirs. Reviewers, closers and plan reviewers stay lead-only, and a
   reprompt to the lead has no generation to start: both are
   `recipient_not_allowed`. The user writes only in the team named on the
   command line. `role@otherteam` is `recipient_not_allowed`, by the
   cross-team rule that already refuses every `reprompt` and `note` (ADR
   0016). The repeat and burst rules apply to the user as to any sender. A
   user reprompt starts the member's next generation, as the lead's does.
5. **The lead's copy.** Every message from the user to a member also gets a
   delivery row for `lead` on the same message, so the lead never misses an
   instruction. The copy is added after every rule has passed. The repeat
   and burst rules neither count a copy nor refuse one: a lead row of a
   user's message that also went to a member is a copy and is skipped. The
   exit code of `send` comes from the addressed recipients only. When the
   copy is not delivered, `send` prints `lead copy not delivered: <id>; run
   teamwire resend <team> <id> --to lead` on stderr. A user's message that
   names the lead and a member together makes no second lead row, and that
   row counts toward the exit code, because the user chose it.
6. **The typed lines.** A member's line from the user says the human user
   wrote it, that it is an instruction like one from the lead, and that it
   is never approval for a permission prompt. The lead's copy says it is a
   copy of the user's message to the named members, for the lead's
   information, and never approval for a permission prompt. A note the user
   addressed to the lead says it is from the human user. The member contract
   (`talking_tail.md`) states the same rule.
7. **No tool speaks as the user.** The lead's MCP server always claims
   `lead`, and a member's always claims its role. The key in the server's
   own environment is never read by a tool call.

## Threat model

The threat model is ADR 0008's: a confused agent, not a hostile one. Every
member runs as the same OS user with bypassed permissions. A member could
read `<home>/user.key`, as it could read another member's `mcp.json` or the
database itself, and then send as the user. The key makes the easy mistake
impossible without deliberate effort: a member's session always carries its
member variables, and those refuse the user's proof even with the right key.
The file mode 0600 keeps out other OS users, not the user's own agents.

## Consequences

- The dashboard sends as `user` by running `teamwire user-key`, reading the
  key from the path it prints, and passing it to `teamwire send --from user`
  in `TEAMWIRE_USER_KEY`, from a process with no member variable.
- `log`, `status`, `events`, `export` and `inbox` show the sender as `user`.
  The `message.sent` event carries `sender_kind: "user"` and no role.
- The user typing `teamwire send --from lead` in a plain terminal is still
  refused. The user types `--from user` instead.
- Rotating the key is not built. A rotation would need a command that
  writes a new key over the old one through `write_atomic`, and every
  process holding the old key (the dashboard server) to read the file
  again. Until then, deleting the file and running `teamwire user-key`
  makes a new key.
- Messages from the user across teams are not built.
