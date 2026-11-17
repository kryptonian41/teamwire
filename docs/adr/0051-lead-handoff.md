# ADR 0051: A lead whose context is full hands its teams to a fresh lead session

Status: accepted, 2026-11-17. Supersedes these sentences, and no others:

- ADR 0035, decision 5 (line 66), "The hook writes nothing", as it applies to
  the lead's tool hook.
- ADR 0018, decision 5 (line 66), "It opens team stores read-only and writes
  nothing", as it applies to the lead's Stop hook.

Both hooks still open team stores read-only and write nothing to any store.
Each now writes one empty marker file outside every store (decision 7). This
ADR also adds to ADR 0006 (decision 9).

## Context

A lead that works for a long time fills its context window, and nothing told
it so. Of 56 real lead sessions on this machine since 2026-10-07, 18 passed
400,000 tokens, and those kept working for a median of 95 more requests. A
fresh lead holds about 98,000 tokens once its skill has loaded. ADR 0027 gave
a member a fresh session in its seat. Nothing did the same for the lead, whose
session teamwire does not start: the user starts it in a pane of their own, on
Claude Code, Codex or pi.

## Decision

1. **The old pane closes.** The handoff closes the old lead's pane once the
   new lead is recorded. The old session stays resumable with the harness's
   own resume command.
   - Beaten: leaving the old pane open. Two leads would then read the same
     teams' watch lines and could answer the same result twice.

2. **Every run hands off without asking.** The rule covers ordinary runs and
   away runs (`/teamwire:afk`). The lead picks the moment: a turn end where it
   waits on members. A lead that waits for the user's answer hands off after
   the answer.

3. **The point is 40% of the window**, with a setting. As ADR 0027 decision 3
   says for members, 40% is a rule of thumb the user asked for, not a measured
   value.

4. **A lead with no open team is skipped.** The note never fires for it, and
   `teamwire lead --handoff` refuses it. A lead that is still planning keeps
   its context.

5. **The setting is `TEAMWIRE_LEAD_HANDOFF_TOKENS`**, read from the
   environment first, then from `$TEAMWIRE_HOME/config.toml`, as
   `pi_exclude_providers` is read. The value is a token count or `off`.

6. **The floor is 200,000 tokens** (`MIN_LEAD_HANDOFF_TOKENS`). The note fires
   only at or above `max(point, 200_000)`. A setting below the floor counts as
   the floor, and `teamwire doctor` prints a `warn` line. A value that is
   neither a whole number nor `off` counts as no setting, and doctor names it.
   On a 1,000,000-token window the point is 400,000. On a 200,000-token window
   it is 200,000, so the note never fires there.
   - Beaten: no floor. A fresh lead already holds about 98,000 tokens, so a
     low point would make every new lead hand off again at once.

7. **The lead's hooks still never write a team store.** The note and the held
   stop are each recorded once as an empty marker file, `noted` and `held`,
   written with `Platform::write_atomic`.
   - Beaten: an event or a column in the team store. Opening a store for a
     write can migrate an older database, and a hook must not do that. A lead
     also leads several teams, and the note is about the lead, not one team.

8. **Files live in `$TEAMWIRE_HOME/handoffs/<old session id>/`:** `handoff.md`
   (the document the old lead writes), `noted` and `held`. Nothing deletes the
   folder. A session id that is not a safe single path component has no
   folder. `HandoffFiles` in `teamwire-core` builds the paths with no file
   access, because `teamwire-report`, `teamwire-launch` and the CLI all need
   them and the first two may not depend on each other.

9. **The new lead's pane carries no teamwire agent name.** `Host::split_run`
   runs the new lead as a plain command in a pane beside the old one, in the
   same tab, with the label `lead · <team title>` (`+N` for N more teams).
   The host sees it as it sees a lead the user started, so the locator of
   ADR 0006 needs no change. This adds to ADR 0006: `teamwire reopen` already
   resumes a lead's old session in a tab named `tw_<team>_lead`
   (`crates/teamwire-launch/src/reopen_lead.rs`). The handoff is the first
   case where teamwire starts a new lead session, and that session's pane
   holds no agent name.
   - Beaten: a pane named `tw_<team>_lead`. In the user's own workspace it
     would mislead `team_workspace` in `crates/teamwire-host-herdr/src/spawn.rs`,
     which picks a team's workspace by its live `tw_<team>_*` agents.

10. **Adoption reuses `team.lead_adopted`** with its present data
    (`old_session`, `new_session`, `lead_cwd`). No event type is added, and
    `teamwire lead --adopt` keeps working as before.

11. **The command records the adoption where it can.** Claude Code and pi let
    teamwire choose the new session's id (`choose_session_id`). The command
    first proves, with the locator, that the new session runs in the new pane.
    The proof waits at most 90 seconds, or `spawn_timeout_s` when that is
    lower. The old lead runs the command through its own shell tool, and
    Claude Code's Bash tool stops a command after 120 seconds by default, so a
    longer proof could be cut off before it closes the new pane and says why.
    Then it writes the adoption itself, one `Store::write` per team, and no
    write transaction is open during a host call. Codex picks its own id, so a
    new Codex lead runs `teamwire lead --adopt <team>` itself as its first
    step, and the command waits up to 180 seconds for every team to name a
    lead other than the old session. The lead guide tells the old lead to give
    the command a time limit of at least 240 seconds where its harness limits
    how long a command may run.

12. **The first prompt is a command-line argument.** A typed first line is
    limited to 600 characters and can arrive before the session is ready.
    herdr refuses a `pane run` line of 1,024 bytes or more on macOS, so the
    herdr adapter writes the command into a private run file and types only
    `sh '<folder>/run'`.

13. **`HarnessCaps::load_skill`** holds the sentence that tells a lead on that
    harness to load the teamwire skill: Claude Code's Skill tool,
    `$teamwire:teamwire` on Codex, `/skill:teamwire` on pi.

14. **One function builds the first prompt**, in `teamwire-launch`, from
    `HarnessCaps::load_skill` and `HarnessCaps::watch_start`.
    `Harness::lead_start` receives the finished prompt and never words it.

15. **Each harness carries the old lead's flags by an allow list.** The old
    flags come from the old lead process's own arguments
    (`Platform::args_of`). A flag a harness does not know is dropped and named
    in `LeadStartSpec::dropped`, by name only, because a value can be a
    secret. Each harness carries at least its permission, model and effort or
    thinking flags. The Claude Code harness takes the permission mode and the
    model from the old transcript when they changed during the session. The
    newest `permissionMode` value decides. When `claude --permission-mode`
    does not accept it, such as `default`, the new lead gets no permission
    flag at all, neither from an older transcript line nor from the old
    arguments. Each old permission flag is named in `dropped`, and
    `LeadStartSpec::warnings` names the value, which the command prints as a
    `warning:` line.
    - Beaten: skipping a newest value outside the list and taking an older
      one. A user who went from `acceptEdits` back to the default mode would
      get a new lead that accepts edits again.
    - Beaten: a deny list. A flag added in a later release would be carried
      without anyone deciding it should be.

16. **A failure before any team moved changes nothing.** A refusal (exit 1)
    touches nothing. A new session that is not seen in the new pane closes
    that pane and exits 2. A failure after one team moved rolls nothing back:
    both panes stay open, the output names the teams that moved and the teams
    that did not, and the new lead takes the rest with `teamwire lead --adopt`.
    Each team that moved still gets its sleep hold moved to its new lead, so
    the hold does not end with the old lead's pane.

17. **`teamwire lead --missed` lists what the old lead may have missed.** The
    window of a team starts 120 seconds before the old lead's `handoff.md` was
    last written, and ends at the adoption. An item is left out only when its
    newest delivery event is before the window; its creation time alone never
    leaves it out. When the document is gone, the window starts one hour
    before the adoption.

## What was ruled out

- A new event type, table or column, and a schema change: the existing
  `team.lead_adopted` event and two marker files cover what is recorded.
- Ending the old lead's process by signal. Closing its pane ends it, and the
  watch that ran in that session ends with it.
- A flag to keep the old pane open while the user is present. Decision 1 says
  it closes.
- Typing the first prompt into the new pane (decision 12).
- A new fake program for Codex, pi or tmux. The launch tests drive the order
  with the fake host and the fake harness. Each real harness and host is
  tested through its contract suite, and one binary test runs `fake-claude`
  in a `fake-herdr` pane.

## Consequences

- A lead past its point is told once and held once. A lead that ignores both
  keeps working with a full context, as before.
- The handoff closes the pane it runs in, so it must be the last action of a
  turn. A failed close is a warning that tells the old lead to end its turn.
- A new Codex lead depends on following its first prompt to adopt each team.
  If it does not, the command times out with both panes open.
- A lead on Opus 4.6 or Sonnet 4.6 whose 1M window was chosen in settings or
  with `/model`, and not with `--model`, starts its new lead on the 200K
  window, because the transcript does not record the `[1m]` suffix. Starting
  the lead with `--model <id>[1m]` avoids it.
- `--missed` may list an item the new lead has already seen. The new lead
  checks each one against the document.
- No live run checked the handoff on Codex or pi, or on a real herdr or tmux
  pane. The lead and the user run the live check.
- The lead guide is a copy of the mattpocock-skills `handoff` skill, like the
  member guide. A change upstream does not reach teamwire unless someone
  copies it.
