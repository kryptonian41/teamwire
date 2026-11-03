# ADR 0027: A member hands off to a fresh session in the same seat

Status: accepted, 2026-10-21.

## Context

A member's context window fills as it works. Past some point its answers get
worse, and with auto-compaction off it stops with an error at the end of its
window. Before this ADR the lead had no way to give a role a fresh session.
Launch refuses a role that ever started (LCH-5). Reopen resumes the recorded
session, so its context stays full. A reprompt types into the same session,
and a planner reached 72% of its window that way: 720,091 of 1,000,000 tokens,
matching the figure the pane showed.

The evidence for a threshold is thin. No published study measures "about 40%
of the window" for a coding agent. The figure is a practitioner's rule of
thumb. The one measured 40 to 50% result is for a single 7B open model. Every
benchmark that was read (Chroma Context Rot, NoLiMa, RULER, Databricks)
reports where quality falls in absolute tokens, and the effective length does
not grow with the claimed window. The highest data point for a Claude model is
120,000 tokens. Anthropic's and Cursor's guidance for long runs is a fresh
session that reads a written handoff.

## Decision

1. **A member hands off when the lead runs a command.** Nothing hands off on
   its own. The lead runs `teamwire handoff <team> --role R`. The member
   keeps its row, worktree, branch, `base_sha`, token, per-run files, brief
   and report. It gets a new session and a new generation.

2. **The fill is read from the end of the member's transcript.** The
   `Harness::context_fill` method returns `Fill { tokens, window }`. For
   Claude, `tokens` is the input, cache-read and cache-creation tokens of the
   newest main-chain assistant line. Only the last 4 MiB of the transcript is
   read, because the member's PostToolUse hook reads the fill after every tool
   call and a transcript has no size limit. For Codex, `tokens` is
   `last_token_usage.input_tokens` of the newest `token_count` event, over
   `model_context_window`. The Codex reader follows the field names in the
   Codex source and has not run against a live Codex session. `Ok(None)` means
   unknown and is never read as zero. A compaction boundary after the newest
   assistant line also gives `None`. The Claude reader skips an API-error line
   (`isApiErrorMessage`, or model `<synthetic>` with zero usage, such as
   "Prompt is too long") and an assistant line with no `message`, and reads the
   next one back. Counting an error line would show a full member as empty.
   - Beaten: the `teamwire.member.tokens` counter. The counter sums every
     request of a session, so it cannot give the fill. One closer showed
     18.4M cumulative cache-read tokens while its last request held 312K.

3. **The threshold is stored in tokens.** The spec key is
   `context_handoff_tokens`, on the team and on a member, and the member's
   value wins. Zero is refused. With no value, the point is
   `DEFAULT_HANDOFF_PCT` (40) percent of the window. **That 40% has no
   measured basis for Claude.** The user asked for it, and it is a rule of
   thumb. On a 1,000,000-token window it is 400,000 tokens, which is past every
   measured Claude data point, so a team that cares sets tokens directly. Launch
   records the merged value in the `member.started` event under
   `context_handoff_tokens` (a number, or null), so the hook, the watch and
   `status` read the same value from the store. A fresh Claude Code session
   already holds about 60,000 tokens (system prompt and tools), so a point
   must sit well above that. Launch warns for a value below 100,000.

4. **The window comes from a table, with a guess for the unknown.** Claude
   models with a 1M window by default (Opus 4.7 and later, Sonnet 5 and later,
   Haiku 5.5 and later, Fable) count as 1,000,000. Older models, and every model
   when `CLAUDE_CODE_DISABLE_1M_CONTEXT=1`, count as 200,000. A model string
   ending `[1m]`, and a bare alias such as `opus`, count as 1,000,000. A
   transcript does not record the `[1m]` suffix, so Opus 4.6 and Sonnet 4.6 count
   as 200,000. A model the table does not know also counts as 200,000. The small
   guess is deliberate: it makes the note fire early, and a large guess could
   stop it firing.

5. **The handoff command closes the old place first.** A reopen opens its new
   tab first, because its session is already dead. A handoff's old session is
   alive, and the host refuses a second live agent of the same name. So the
   command checks, closes the old place, then opens the new one. The checks
   are the ones of `close --role` that protect work: a running turn, a dialog,
   an unknown state, and uncommitted paths. The wait for the old place's
   processes to end runs for every member mode, worktree or pane. The command never stops a running
   turn and never types into the old pane. The old session stays recorded until
   the new one has passed the trust step. A failure after the close began
   exits 2, and a store that cannot be read exits 6. A failed place close stops
   before any spawn, and the error names `teamwire reopen <team> --role R`,
   which resumes the old session once it has ended. After a failure past the
   spawn, the command closes the new tab and writes the old session id back
   when the row holds another one. A harness that records its own session
   (Codex) can leave the new id there. When no old session was recorded, the
   error says `reopen` has nothing to resume.
   - Before it closes anything, the command checks the handoff document. A
     document that is missing, unreadable or older than the generation's start
     gives a `warning:` line, and the first line points at the brief, the
     member's report and `git log <branch>` instead.
   - `teamwire-launch` and `teamwire-close` are both layer 4 and may not depend
     on each other (`docs/conventions.md` section 5). `teamwire-cli` runs the
     steps in order, so the order lives in one file, `cmd/handoff.rs`.

6. **The guide is carried in the binary.** `teamwire handoff --guide` prints a
   copy of the `handoff` skill of mattpocock-skills (version 1.3.1, MIT licence),
   adapted for a member, with the credit and the licence notice. The guide has
   the member write `reports/<role>.handoff.md` and record
   `done` with a summary that starts with `handoff:`. The PostToolUse note tells
   the member to run the command. The note appears once per generation, joined
   with the unread-message reminder.
   - Beaten: a member plugin. It would need a second plugin id, and a plugin
     reaches Claude members only.
   - Beaten: the system prompt. The contract is already large, and every
     member would pay for text most never use.

7. **The event is `member.handoff`, with schema version 7.** Its data holds
   `generation`, `old_session`, `new_session` and `place`. One store write
   moves the session and place, raises the generation, sets `gen_started_at`,
   clears `reminded_at` on the member's unacknowledged deliveries and records
   the event. Without the clearing, the reminder fires once per message and
   a new session would never be reminded of a long unread message. Only a long message needs the reminder: a message whose whole body fit in the typed line is never reminded of (ADR 0021). Such a short message reached the old pane and not the new session, so the first line of the new session says how many unread messages there are and tells it to read them with `team_inbox` or `teamwire inbox`. Version 6 is taken by
   `member.refused`.
   - Beaten: extra fields on `member.reprompted`. A handoff changes the
     session and the place, a reprompt changes neither, and every reader that
     counts reprompts would have to tell the two apart.
   - Metrics treat a handoff as the start of a generation. It adds to no
     counter and is not a reprompt.

8. **The lead sees the fill.** `teamwire status` shows a `ctx` column and
   `--json` a `context` object. The watch prints `CONTEXT <role> <pct>% <n>k of
   <window>k (handoff at <n>k)` once per member and generation. The watch key
   `CONTEXT:<role>:<generation>` differs from the hook's key
   `handoff-note:<role>:<generation>`, because one key would silence the other.
   The dashboard shows the percent beside the state chip.

9. **The compaction backstop is for Claude members only.** A Claude member's
   per-run `settings.json` sets `autoCompactEnabled` to true and
   `autoCompactWindow` to twice the handoff point, at least 100,000 (the floor
   Claude Code documents) and at most the window. With the 40% default the note
   therefore comes first. A configured `context_handoff_tokens` at or above the
   model's real window means the note never fires, and compaction does. With no model, or an unknown one, the window is guessed large,
   because here the large guess is the safe one: the backstop must never fire
   before the note. A Codex member keeps its default, because its compaction
   settings were not researched.

## Evidence from a live check

On 2026-10-21 a real Claude Code member ran with `context_handoff_tokens` 20,000.
A fresh session held 61,000 to 64,000 tokens before its first tool call, so it
was past its point at once, and so was the new session after `teamwire handoff`.
The fill reader matched the pane's own figure (6%, 64k of 1000k). The note, the
handoff document, `done` with `handoff:`, the handoff command and the new
session's Stop hook all worked live. The Codex reader was not part of this
check.

## What was ruled out

- `reopen` as a second leg: it resumes the old session id, so the context is
  unchanged.
- `launch --add` with the same role: launch refuses it with `role_reused`.
- A fresh session started by hand in the old tab: its hooks are ignored,
  because the session id does not match the row, and results would arrive only
  through the watch.
- A schema change for the watch line or the once-per-generation note:
  `announce_once` takes free-text keys, so neither needs one.
- Replacing the member under a new role on the same branch: verify of the
  second member diffs only from the branch tip, and the first member's commits
  are never checked when it was closed `--unverified`.

## Consequences

- A handoff is a lead decision. A lead that ignores the `CONTEXT` line leaves
  the member to the compaction backstop (Claude) or to its own limit (Codex).
- The Codex fill reader, and the compaction behaviour of Codex, are unchecked
  against a live run.
- The handoff guide is a copy. A change to the upstream skill does not reach
  teamwire unless someone copies it.
