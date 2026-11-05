# ADR 0039: `teamwire pr-watch` can deliver its lines, and a pi lead watches pull requests with a tool

Status: accepted, 2026-11-06. Supersedes no sentence of an earlier ADR. It
gives `teamwire pr-watch` (EXT-1) what ADR 0023 gave `teamwire watch`, a
`--deliver` mode for a lead that cannot read a command's output as it is
printed, and what ADR 0036 gave the watch in pi, a tool whose lines are
steered into the session.

## Context

A lead watches its pull requests' checks with `teamwire pr-watch -R
OWNER/REPO -i 45 <prs>`, which prints one line per poll at most and exits
when every pull request is done. The lead skill told the lead to start it
with Claude Code's `Monitor` tool, which wakes the lead on each line.

Only a Claude Code lead has `Monitor`. The audit team `conform` found on
2026-11-05 that a Codex or pi lead had no way to watch pull requests
(`~/.teamwire/archive/conform-2026-11-05/reports/leadprompt.md`, finding
F5):

- `teamwire pr-watch` had no `--deliver`, so a Codex lead could not run it
  in the background and still hear its lines.
- The pi lead package refused only `teamwire watch` in bash. A pi lead that
  ran `teamwire pr-watch` through bash held that tool call, and so its own
  turn, until every pull request was done or the call timed out. It could
  not read member results meanwhile.

The user decided on 2026-11-06 that the fix is code, not a lead-skill note
telling those leads to check each pull request by hand.

## Decision

1. **`teamwire pr-watch --deliver <team>` types each line it prints into
   that team's lead pane.** The command is
   `teamwire pr-watch --deliver <team> -R OWNER/REPO -i 45 3453 3454`.
   `--deliver` takes the team's name, because a pull-request watch belongs
   to no team, and the team's store is what names the lead's pane. The
   lines go through the same function that types `teamwire watch
   --deliver`'s lines (`deliver_to_lead` in
   `crates/teamwire-cli/src/cmd/watch.rs`), under the same rules (INV-8):
   each delivery ends within 15 s, and a line that cannot be delivered is
   still printed and the watch goes on. A member session is refused, as for
   `teamwire watch`.

2. **A delivered line carries the undated prefix `[teamwire:<team>]`.**
   The line is printed and typed alike:
   `[teamwire:alpha] o/r#7 FAIL: build`. A lead's skill says every
   teamwire line starts with `[teamwire:<team>`, and a bare
   `o/r#7 FAIL: build` typed into a pane reads like the user's own words.
   The line is not dated, for the reason ADR 0037 decision 2 gives for the
   other undated lines: it is typed the moment it prints. Without
   `--deliver`, `pr-watch` prints as before, with no prefix.

3. **A Codex lead starts it in the background**:
   `nohup teamwire pr-watch --deliver <team> -R OWNER/REPO -i 45 <prs>
   >/dev/null 2>&1 &`, as it starts `teamwire watch` (ADR 0023).

4. **The pi lead package registers a tool `teamwire_pr_watch`.** It takes
   `repo` (`OWNER/REPO`, required), `prs` (pull request numbers, required)
   and `interval` (seconds, optional; `pr-watch`'s own default when left
   out). It runs `teamwire pr-watch -R <repo> [-i <interval>] <prs>` without
   `--deliver`, through the function that starts the watch tool's child, so
   each line reaches the model through the one steered `pi.sendMessage`
   call of ADR 0036. Exit 0 and exit 1 are ordinary ends: exit 1 means a
   check failed, which the last line already said. Any other exit after the
   tool returned is said once:
   `[teamwire] the pull-request watch of <repo> stopped: <why>. Start it
   again with the teamwire_pr_watch tool.` A second call for the same
   repository and pull requests is refused while the first runs.

5. **The package's bash refusal also refuses `teamwire pr-watch`**, in any
   form, and names the tool: `In pi, watch pull requests with the
   teamwire_pr_watch tool (repo, prs, interval).`

## Rejected

- **Copy the delivery code into `pr_watch.rs`.** Two copies of the rules of
  INV-8 would stop matching the first time one changed. Both commands call
  one function.
- **A plain `--deliver` flag that finds the team from the calling session.**
  A lead can lead several teams, and the session's variables name none of
  them. The team's name is the one thing the lead always has.
- **Have the pi tool pass `--deliver`.** The extension reads the child's
  output and steers it already, so `--deliver` would put each line into the
  session twice, once typed into the pane and once steered.
- **Fold pull requests into `teamwire watch`.** EXT-1 keeps the
  pull-request watch apart from the team's lifecycle, and a team watch ends
  at `TEAM-COMPLETE`, long before CI does.

## Consequences

- A Codex lead and a pi lead hear about a failed check, a merge state and
  `DONE` without holding their own turn, as a Claude Code lead does with
  `Monitor`.
- `pr-watch --deliver` takes no lease, so two of them over the same pull
  requests both type their lines. The pi tool refuses a second watch of the
  same repository and pull requests in one session; a Codex lead that
  starts a second background watch hears every line twice.
- `teamwire close` does not stop a background `pr-watch --deliver`. It runs
  until every pull request is done, on the store connection it opened when
  it started. Whether its later lines still reach the lead's pane once
  close has deleted the team's state is not checked.
- An installed pi package is out of date until `teamwire install` runs
  again, and `teamwire doctor` says so.
- The tests are `crates/teamwire-cli/tests/watch.rs`
  (`pr_watch_deliver_types_each_printed_line_into_the_leads_pane`, with a
  fake `gh` and the fake herdr lead) and
  `crates/teamwire-harness-pi/tests/pr_watch_tool.rs`.
