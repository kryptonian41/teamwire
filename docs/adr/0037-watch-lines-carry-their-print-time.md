# ADR 0037: A watch line carries its print time, and the first poll holds `TEAM-COMPLETE`

Status: accepted, 2026-11-04. Supersedes this sentence of ADR 0023, and no
others:

- ADR 0023, decision 9 "A Codex lead" (lines 188-190), the clause that
  `teamwire watch <team> --deliver` types each line into the lead's pane
  "with the `[teamwire:<team>]` prefix". The prefix is now
  `[teamwire:<team> HH:MM:SSZ]`, and `--deliver` types the line exactly as
  it printed it.

Every other sentence of ADR 0023 stands. `--deliver` still types each line
it prints, through `teamwire-deliver`, still prints a line it cannot
deliver, and a lead without `reads_watch_lines` still starts it in the
background.

## Context

A read-only survey of this machine's own teams (team `evtriage`, 2026-11-04;
its reports are in `~/.teamwire/archive/evtriage-2026-11-04/`) found three
faults in how a watch line reaches a lead. ADR 0036 records the first, which
is pi's delivery mode. This ADR records the other two, which belong to the
watch itself. Decision 3 below closes the first-poll exit that ADR 0036's
Consequences name as a separate fault it does not touch.

**A lead could not date a line.** Every line looked alike but for its member
count, so a lead had no way to tell a current line from one an exited watch
printed minutes earlier. Across 52 lines paired from the store to the
transcripts, the median delay to a pi lead was 107 seconds and the worst 29
minutes 46 seconds. 16 `TEAM-COMPLETE` lines reached a lead that had already
added or reprompted members. Leads acted on them: one spent five turns and
two extra `teamwire status` calls undoing one burst of six old lines.

**A watch could exit a moment after it started.** `TEAM-COMPLETE` is
recomputed from the roster on every poll, and a fresh watch starts with empty
memory, so a watch over a team whose live members were all settled printed
`TEAM-COMPLETE` on its first poll and exited at once. In team
`fb-2026-11-03` (store `watch.line` seq 304) the watch read the roster 235 ms
before `launch --add` wrote a new reviewer's row, printed `TEAM-COMPLETE`,
exited, and that reviewer ran unwatched for 3 minutes 30 seconds. The same
first-poll exit happened at seq 238 of the same team, and in five teams
across the 396 lines the survey read.

The repeat itself was intended and written down: WAT-16 said a restarted
watch prints `TEAM-COMPLETE` again for a team that already finished. What
nobody had decided was what happens when a lead starts a watch in the same
turn as a `launch --add`, which is what the lead's own skill told it to do.

## Decision

The user took both decisions on 2026-11-04.

1. **Every line `teamwire watch` prints carries the poll's time in its
   prefix**, in UTC to the second: `[teamwire:<team> HH:MM:SSZ] <events>`.
   For example
   `[teamwire:fb 17:24:07Z] WATCHING 3 members | TEAM-COMPLETE 3/3 done`.
   The time comes from the `Clock`, so a test can fix it. Every event joined
   into one line carries the same prefix, `TEAM-CLOSED` and `STORE-ERROR`
   included, and `--deliver` types the line it printed (WAT-21).

2. **The other `[teamwire:<team>]` lines keep their undated prefix**: a
   member's result line, a message line, the Stop-hook and tool-hook notes,
   the AFK notes, and the pi extension's own "the watch stopped" line. A
   lead reads those the moment they are typed into its pane, so their age is
   not in question, and dating them would churn every test that pins one.

3. **A running watch never prints `TEAM-COMPLETE` on the poll that prints
   `WATCHING`.** It leaves it to the next poll, and prints it then if the
   team is still complete. A member added or reprompted in between is
   watched as any unsettled member is. `teamwire watch --once` keeps
   printing `TEAM-COMPLETE` in its single poll, because scripts and tests
   use it to read the state once (WAT-20).

4. **An empty roster ends a watch that has watched a live member.** Once the
   lead has removed the last live member, nothing is left to watch, so the
   watch prints `TEAM-COMPLETE 0/0 done` and exits. A watch over a team that
   never had a live member keeps polling, because the first wave may still
   be launching. Without this, decision 3 would leave a watch polling for
   ever over a roster that emptied between its first and second poll.

The hold is tied to the poll that prints `WATCHING`, not to the first poll of
the process. A poll that failed with `STORE-ERROR` read no roster, so it
cannot have raced a `launch --add`, and the poll that does read one is the
one that must hold.

## Rejected

- **Have `launch --add` record the member before any watch can read a
  settled roster.** A marker the watch could see would have to be written
  before launch validates the spec, and launch may then refuse the member
  and have to take the marker back. The hold needs no new state outside the
  watch process.
- **Key `TEAM-COMPLETE` to a roster version, so a repeat on an unchanged
  team stays quiet.** This keeps a watch from repeating the line, which
  WAT-16 wants it to do: a lead that starts a watch over a finished team
  must hear that the team is finished. It also does nothing for the race,
  because the roster version the first poll reads is the one from before the
  new member.
- **Put the watch's process id, rather than its time, in the prefix.** A pid
  tells a lead that two lines came from different watches, but not which is
  the older, and a lead comparing a line with its own last action needs the
  time.
- **Date every `[teamwire:` line.** See decision 2.
- **Tell the lead in its skill to run `teamwire status` before acting on any
  watch line.** No code would change, but every line would cost the lead a
  tool call. The lead's skill now says to run `status` only when the line's
  time is older than its last `launch --add` or reprompt.

## Consequences

- A lead can date every watch line against its own last action, and the
  lead's skill (`examples/skills/teamwire/SKILL.md`, "Watching for the
  failures nobody sees") tells it to compare the time with `date -u +%T` and
  to run `teamwire status` rather than act on an older line.
- A finished team costs one more poll before its watch exits, so a
  `teamwire watch --interval 20` over a team that is already complete prints
  `WATCHING` and then `TEAM-COMPLETE` 20 seconds later. Two lines, two turns
  for the lead, where it used to be one.
- The prefix is ten characters longer, so one 300-character event holds ten
  characters less and one 2000-character line holds fewer events. A backlog
  takes a little longer to drain. An event that ends `Result: <path>` keeps
  the whole path and cuts the summary before it instead (WAT-1).
- **The race is narrowed, not closed.** A poll reads the roster in one
  transaction and prints in the next, under a millisecond later. A
  `launch --add` that commits inside that window is still invisible to the
  poll that prints `TEAM-COMPLETE`, so the watch still exits and that member
  still runs unwatched. The window the hold removes is the 235 ms between a
  watch starting and its first poll's print, which is the one a lead hits by
  running `watch` and `launch --add` in the same turn. Closing the
  millisecond window would need the roster read and the print in one write
  transaction, which would hold the write lock across the watch's host and
  harness calls, and no store write may stay open during an external call.
  The user accepted the remaining window on 2026-11-04. A lead that wants
  certainty after a `launch --add` reads `teamwire status`.
- A watch whose team empties under it now exits instead of polling for ever.
  Before decision 4, such a watch also held the team's watch lease, so the
  lead's next `teamwire watch` was refused while it ran.
