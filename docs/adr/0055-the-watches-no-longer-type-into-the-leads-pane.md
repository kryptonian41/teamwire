# ADR 0055: The two watches no longer type their lines into the lead's pane

Status: accepted, 2026-11-20. Supersedes the sentences in the table at the
end of this file, and no others.

## Context

Two commands had a flag that typed each printed line into the lead's pane:

- `teamwire watch <team> --deliver` (ADR 0023, point 9).
- `teamwire pr-watch --deliver <team>` (ADR 0039, decisions 1 and 2).

Both flags were built for a lead whose harness could not read the output of
a command while the command ran. That harness was Codex, and ADR 0054
removed it in v0.29.0. ADR 0054 decision 8 kept both flags for that release
and left their removal open.

No lead needs either flag now:

- A Claude Code lead reads `teamwire watch` and `teamwire pr-watch` through
  its Monitor tool.
- A pi lead reads both watches through the tools `teamwire_watch` and
  `teamwire_pr_watch` of its lead package (ADR 0036, ADR 0039 decision 4).
  The tools run the watches without the flag and steer each line into the
  session.
- After ADR 0054, no harness, skill or launch text starts either command
  with the flag.
- Launch refuses every member on host `none`
  (`crates/teamwire-launch/src/plan.rs`), so no team needs the flag there
  either.

The lead checked on 2026-11-20 at commit `00d64df1` that no event type and
no metric exists only for a delivered watch line. The event type
`watch.line` records a printed line, with or without the flag.

## Decision

The user decided on 2026-11-20 to remove both flags, because they have no
user.

1. **Neither command accepts `--deliver`.** The command line refuses the
   flag as it refuses any unknown flag, with exit 3. The tests are
   `watch_refuses_deliver_as_a_usage_error` and
   `pr_watch_refuses_deliver_as_a_usage_error` in
   `crates/teamwire-cli/tests/watch.rs`.
2. **The code behind the flags is removed.** The function `deliver_to_lead`
   and its 15-second time limit `DELIVER_BUDGET` in
   `crates/teamwire-cli/src/cmd/watch.rs` go. The branch of `pr-watch` that
   opened a team's store to deliver goes from
   `crates/teamwire-cli/src/cmd/pr_watch.rs`, with the `[teamwire:<team>]`
   prefix it put on each line and its refusal of a member session.
3. **`teamwire pr-watch` belongs to no team.** It opens no store, its lines
   carry no prefix, and it types nothing. That is what it already did
   without the flag.
4. **`teamwire watch` never types into a session** (INV-8, DEL-16). It
   prints each line with its dated prefix (WAT-21), and the lead reads it.
5. **These parts stay.**
   - `deliver_line` and `deliver_line_within` of the crate
     `teamwire-deliver`. The delivery of a member's result
     (`crates/teamwire-report/src/turn_end.rs`) and of a message
     (`crates/teamwire-msg/src/attempt.rs`) call them.
   - The delivery of results and messages into a lead's pane, with all its
     rules (every DEL row but DEL-16).
   - The pi lead package's tools `teamwire_watch` and `teamwire_pr_watch`,
     and its refusal of both commands through bash (ADR 0036, ADR 0039
     decisions 4 and 5).
   - The event type `watch.line`.
6. **History is not edited.** Every accepted ADR, `docs/design/` and
   `bench/studies/` stay as they are. The requirement rows INV-8, DEL-16,
   WAT-21 and EXT-1 follow the code (ADR 0050, decision 2), so they are
   edited in place.

## Rejected

- **Keep the flags without a user.** Each flag kept a code path and tests
  that no lead runs. A flag that nothing starts also tells a reader of
  `docs/api/cli.md` that some lead needs it.
- **Keep only `pr-watch --deliver`.** A Claude Code lead reads `pr-watch`
  through Monitor and a pi lead through its tool, as for `watch`. The flag
  had the same missing user.
- **Make the flag print a note and run without it.** A lead that passed the
  flag would wait for typed lines that never come. A usage error says at
  once that the flag is gone.

## Consequences

- A script that starts either command with `--deliver` exits 3 at once. No
  such script exists in this repository or its examples.
- A lead on v0.29.0 or an older build that started a background watch with
  the flag keeps it running until that watch ends. The upgrade does not
  stop it.
- `tidy --base` prints `WEAKENED` for the four deleted tests and the deleted
  half of a fifth in `crates/teamwire-cli/tests/watch.rs`. Each tested the
  removed behaviour: `deliver_types_each_printed_line_into_the_leads_pane`,
  `a_line_that_cannot_be_delivered_is_still_printed`,
  `pr_watch_deliver_types_each_printed_line_into_the_leads_pane`,
  `a_member_session_cannot_deliver_pr_watch_lines`, and the half of
  `pr_watch_types_nothing_without_deliver_and_prints_what_it_cannot_deliver`
  that passed the flag. This ADR is the reason for each of those lines. The
  kept half is now `pr_watch_prints_its_lines_and_types_nothing`, and
  `without_deliver_nothing_is_typed` is now
  `the_watch_types_nothing_into_the_leads_pane`, with every assertion kept.
- Bringing a typed watch back means a new decision, for a new harness that
  cannot read a running command.

## What still holds in ADR 0039

ADR 0039 gave `pr-watch` both the flag and the pi tool. After this ADR,
these parts of it still hold. Where a line holds both a part that still holds
and a part this ADR supersedes, the item names the part that holds, and the
table below names the other part.

- Line 1, the title's second clause: "a pi lead watches pull requests with a
  tool".
- Line 3: ADR 0039 supersedes no sentence of an earlier ADR.
- Lines 6-7: ADR 0039 gives `pr-watch` "what ADR 0036 gave the watch in pi, a
  tool whose lines are steered into the session".
- Context, lines 23-26: a pi lead that runs `teamwire pr-watch` through bash
  holds its own turn.
- Decision 4 (lines 58-69): the pi tool `teamwire_pr_watch`, which runs
  `teamwire pr-watch` without the flag and steers each line.
- Decision 5 (lines 71-73): the pi package refuses `teamwire pr-watch`
  through bash.
- Rejected, lines 83-84: the pi tool does not pass the flag, and "the
  extension reads the child's output and steers it already". The rest of
  that sentence, on lines 84-85, gives a reason that is now false, and the
  table below supersedes it.
- Rejected, lines 86-88: pull requests are not folded into `teamwire watch`.
- Consequences, lines 92-94, the pi part: a pi lead hears about a failed
  check, a merge state and `DONE` without holding its own turn. ADR 0054
  already superseded the Codex part.
- Consequences, lines 96-97, the sentence that the pi tool refuses a second
  watch of the same repository and pull requests in one session. Lines
  103-104: an installed pi package is out of date until `teamwire install`
  runs again. Lines 107-108: the test
  `crates/teamwire-harness-pi/tests/pr_watch_tool.rs`.

## Superseded sentences

| ADR | Lines | What the sentence said |
| --- | --- | --- |
| 0023 | 188-190 | `teamwire watch <team> --deliver` types each line it prints into the lead's pane, through `teamwire-deliver`. ADR 0054 already superseded the rest of that bullet, lines 190-193 |
| 0026 | 76 | The rows INV-8 and DEL-16 describe `teamwire watch --deliver`, as ADR 0023 decision 9 changed them. Both rows now cite this ADR |
| 0037 | 9-10 | `--deliver` types the line exactly as the watch printed it |
| 0037 | 12-15 | `--deliver` still types each line it prints, through `teamwire-deliver`, and still prints a line it cannot deliver. ADR 0054 already superseded the clause about a lead without `reads_watch_lines` |
| 0037 | 59 | The clause of decision 1 that `--deliver` types the line it printed (WAT-21). The rest of decision 1 stands |
| 0039 | 1 | The title's first clause, "`teamwire pr-watch` can deliver its lines". Its second clause, about the pi tool, stands |
| 0039 | 3-6 | The clause of the status sentence that gives `pr-watch` "what ADR 0023 gave `teamwire watch`, a `--deliver` mode for a lead that cannot read a command's output as it is printed". Line 3's first sentence and the clause about the pi tool on lines 6-7 stand |
| 0039 | 33-43 | Decision 1: `pr-watch --deliver <team>` types each line into that team's lead pane through `deliver_to_lead`, each delivery ends within 15 s, and a member session is refused |
| 0039 | 45-52 | Decision 2: a delivered line carries the undated prefix `[teamwire:<team>]`. The last sentence, that `pr-watch` prints with no prefix, stands for every run |
| 0039 | 77-82 | The first two rejected options, which chose how the flag reached the delivery code and named the team |
| 0039 | 84-85 | The reason "so `--deliver` would put each line into the session twice, once typed into the pane and once steered". The flag now types nothing, because both commands refuse it. The rejected option on line 83 stands |
| 0039 | 95-96, 99-102 | `pr-watch --deliver` takes no lease, so two of them both type their lines, and `teamwire close` does not stop a background `pr-watch --deliver` |
| 0039 | 105-107 | The test `pr_watch_deliver_types_each_printed_line_into_the_leads_pane` |
| 0054 | 17, 22-25 | Point 9 of ADR 0023 keeps four parts, one of them that `teamwire watch <team> --deliver` types each line through `teamwire-deliver` (`deliver_to_lead`). Point 9 now keeps three parts: the wiring's choice of the lead harness, the selector of `teamwire install`, and the shared skill text |
| 0054 | 106-109 | Decision 8: both flags stay in v0.29.0, and their removal is a separate decision |
