# ADR 0026: Corrections to accepted ADRs, the `member.refused` event, and an accepted trust risk

Status: accepted, 2026-10-20. Supersedes these sentences, and no others:

- ADR 0004, decision 1, the clause "no core type changes" (ADR 0023 changed
  `HarnessCaps`, `MemberLaunch` and `Harness`).
- ADR 0004, decision 6, the hook names `claude-stop` and `claude-post-tool`
  as the only names.
- ADR 0004, consequences (lines 110-112), the sentence that the trust step
  "reads `~/.claude.json` read-only first and only reads the screen when
  trust is missing".
- ADR 0006, decision 1, the harness view as the only way to find a session's
  process (`Harness::processes_for_session` with `Platform::env_of`).
- ADR 0007, decision 5, the sentence that a lead with no locatable place gets
  route B from the start.
- ADR 0010, line 21, the row that lists Codex CLI as not built.
- ADR 0011, decision 2, the sentence that the token comes only from
  `TEAMWIRE_TOKEN` or `--token`.
- ADR 0002, decision 5 (lines 53-54), the sentence that `--repair-files`
  rewrites "any copy".
- ADR 0012, decision 3, the use of `claude mcp add`.
- ADR 0017, decision 5, "Claude only", and the matching sentence for the
  `plugins` map.
- ADR 0018, decision 1, "one plugin" with one skill, and decision 5, a lead
  Stop hook that holds only for a missing watch.
- ADR 0022, decision 3 (lines 33-36), "the hold is the one prompt to read the
  full text".
- ADR 0025, decision 3 (line 58), "An older binary opens a version 5 database
  read only (ADR 0015)".
- ADR 0025, decision 9, "Close refuses an AFK team whose `afk/REPORT.md` is
  missing" with no exception, and its heading "Not done and why".

It does not reverse ADR 0017 line 51. Unknown spec keys are still ignored, and
launch now also warns once for each.

## Context

AGENTS.md says an ADR changes only by adding one that supersedes it. Later
ADRs did change earlier ones without saying so. ADR 0023 and ADR 0025 carry
no "Supersedes" line, and no ADR points forward to its replacement. A reader
who opens ADR 0004 or ADR 0025 today learns rules the code no longer follows.
The deepreview of 2026-10-19 listed these gaps (findings 33, 105, 108 to 112
and 115). Team `fixall` fixed the code the review found wrong, and two of its
decisions need an ADR of their own.

## Decision

1. **A forward table.** Each changed point of an accepted ADR is listed
   below with the ADR or requirement that now holds. The old ADR files stay as
   they are.

   | Old ADR and point | What changed | Changed by |
   | --- | --- | --- |
   | 0004, decision 1 | Adding a harness did change core types: `HarnessCaps.reads_watch_lines`, `MemberLaunch.accept_trust`, `Harness::caller_session` | ADR 0023, decisions 2, 3 and 8 |
   | 0004, decision 6 | Member hook names follow the member's harness (`hook turn-end`, `post-tool`, `session-start`). The `claude-*` names stay as aliases for settings an older launch wrote | ADR 0023, decision 6 |
   | 0004, lines 110-112 | The trust step reads only the screen. It reads no `~/.claude.json`. It checks the escalation markers on every read, and again after Enter | this ADR (finding 67); LCH-15 |
   | 0006, decision 1 | For Codex the harness view is the holder of the thread lock (`lsof`). Under herdr the locator needs the host's `agent_session` | ADR 0023, decision 7 and its consequences |
   | 0007, decision 5 | A lead whose harness cannot read watch lines and whose pane cannot be found is refused at launch with `lead_not_locatable`. Route B stays for leads that read watch lines | ADR 0023, decision 9 |
   | 0010, line 21 | Codex CLI ships. Gemini CLI and opencode stay unbuilt | ADR 0023 |
   | 0011, decision 2 | A Codex member's token is read from the file named by `TEAMWIRE_TOKEN_FILE` when `TEAMWIRE_TOKEN` is unset | ADR 0023, decision 4 |
   | 0002, lines 53-54 | `--repair-files` rewrites a `comms/` copy only. An `outbox/` copy holds parts the store does not keep, so it is reported and never rewritten | STO-10 |
   | 0012, decision 3 | `claude mcp add` does not exist in the code. Only `claude mcp remove` remains, for the migration | ADR 0018, decision 2 |
   | 0017, decision 5 | The `plugins` map applies to Codex members too | ADR 0023, decision 3 |
   | 0018, decisions 1 and 5 | The plugin carries a second skill, `afk`, and the lead Stop hook also holds an AFK lead once per stop chain | ADR 0025, decisions 1 and 5 |
   | 0022, decision 3 | The PostToolUse reminder and the Stop hold together remind each delivery once. A delivery that already has `reminded_at` is never held. The hold is the prompt for a message the reminder did not name | this ADR (finding 32, part f); `crates/teamwire-report/src/reminder.rs` |
   | 0025, decision 3 (line 58) | An older binary refuses a version 5 or version 6 database with `schema_newer` and changes nothing. ADR 0015 covers only a newer binary reading an older schema | STO-6; ADR 0020, consequences (rollout) |
   | 0025, decision 9 | A team that never had a member row closes without a report. Headings are compared ignoring case and commas, so "Not done and why" and "Not done, and why" both pass | AFK-7; this ADR |

2. **Requirement rows follow their ADRs.** These rows of
   `docs/requirements.md` were written before the ADR that changed them, and
   each now cites it:

   | Row | Changed by |
   | --- | --- |
   | SEC-2 | ADR 0016 |
   | INV-8, DEL-16 (`teamwire watch --deliver`) | ADR 0023, decision 9 |
   | INV-22, LCH-12 (the Codex contract as one escaped argument) | ADR 0023, decision 3 |
   | MSG-11 (what "read" means) | ADR 0021 and ADR 0022 |
   | LCH-20, LCH-21, LCH-6, REP-7, OBS-4 (Codex) | ADR 0023 |
   | MCP-1 (nine tools) | ADR 0016 |
   | MSG-20 (the one duplicate) | ADR 0003, consequences |
   | CLS-4 (`--stop` on an unknown state) | ADR 0019, decision 5 |
   | ADP-11 | ADR 0010 |
   | WAT-11 | ADR 0024 |
   | AFK-7 | ADR 0025, decision 9, and this ADR |

3. **A refused member is an event.** `member.refused` is a new event type.
   Launch writes it for a member it turns away, with `reason_code`
   `afk_budget_spent` or `team_closed`, and with `launch`, `requested`,
   `dispatch`, `reason` and `steps` in its data. The metric
   `teamwire.launch.members` counts it as `outcome=refused`, and the launch
   span counts refused members. Launch checks the AFK budget at each
   member's start and again right before its spawn. A member refused after
   launch recorded it is undone as a failed launch is, and its row is left
   `failed` with no `member.failed` event, so the member is counted once.
   - The event list is a closed CHECK list, and SQLite cannot change a CHECK
     in place. Schema version 6 therefore rebuilds `events` with the longer
     list, as version 4 did for `delivery.reentered`. Every row and every
     `seq` is kept.
   - An older binary refuses a version 6 database (STO-6). The rollout
     warning of ADR 0020 applies: install the new binary when no team is
     running, or restart the watch, the lead MCP server and the member hooks
     after the install.
   - Beaten: write no event and drop `outcome=refused` from the docs. The
     metric `docs/metrics.md` promised would then never exist.

4. **The trust risk of finding 3 is accepted.** Launch answers a plain
   folder-trust dialog for the Claude adapter, as LCH-15 says. Claude Code
   2.1.303 draws warning lines only for allow rules, added directories and
   headers helpers. It shows the same plain dialog for a folder whose
   settings declare hooks, project MCP servers or an `apiKeyHelper`. Those
   then run with the member's rights, and INV-25 does not stop them. The user
   decided on 2026-10-19 not to fix this.
   - Read a folder's `.claude/` and `.mcp.json` before launching members into
     it for the first time. The lead skill and `docs/failure-modes.md` say so.
   - Beaten: refuse the plain dialog when the folder holds `.claude/` or
     `.mcp.json`. That would refuse every repository that tracks those files
     until the lead opened Claude Code there by hand. The Codex adapter does
     refuse in that case (ADR 0023, consequences).
   - Beaten: narrow the project settings with `--setting-sources`. The dialog
     would still be answered, and the folder would stay trusted for the user's
     later sessions.

5. **Unknown spec keys.** ADR 0017 line 51 still holds: a spec key the types
   do not know is ignored. Launch now also prints one warning for each such
   key, naming the role or the team and the key (LCH-2). A test keeps the list
   of known keys equal to the spec types.

## Consequences

- No existing file under `docs/adr/` changes. A reader of an old ADR finds
  the replacement here.
- Open pull request #1 (`tasklog/integrate`) also uses number 0026 and schema
  version 6. By the user's decision of 2026-10-19 that branch renumbers after
  this one merges.
- A Claude Code lead can still be misled by a folder that declares hooks. The
  risk is the user's to weigh, and this ADR is the place that says so.
