# ADR 0050: Corrections to accepted ADRs that later code and ADRs changed on purpose

Status: accepted, 2026-11-16. Supersedes these sentences, and no others:

- ADR 0021, decision 3 (lines 33-35), the sentence that `acked` "still says
  only that the recipient read the message through `inbox` or `ack`".
- ADR 0022, decision 4 (line 37), "`acked` keeps its meaning, as ADR 0021
  point 3 says", where it carries that same sentence.
- ADR 0009, decision 3 (lines 34-36), the words "(and `close`)": the
  whole-team close does not check verify records.
- ADR 0045, decision 4 (lines 52-54), the words "and `close`" in "removing a
  member, and `close`, accept only a record at the member's HEAD now".
- ADR 0030, decision 5 (lines 127-128), the words "so launch prints
  `teamwire watch <team>`".
- ADR 0029, decision 2 (line 53), the table row "`reads_watch_lines` | no".
- ADR 0023, decision 11 (lines 229-230), the bullet that lists "Reading a
  Codex transcript" as not built.
- ADR 0030, decision 4 (lines 115-116), the sentence "`current_session`
  reads the process environment the same way".
- ADR 0019, decision 3 (lines 39-41), the sentence that the second Enter "is
  logged at `info` level only" and that a durable record needs a new event
  type.
- ADR 0001, Decision (lines 28-34), "Two subcommands run for a long time",
  where it lists `watch` and `mcp` only.
- ADR 0004, decision 1 (lines 37-38), "The wiring module is the only code
  that names a concrete adapter", as it applies to hosts.
- ADR 0008, decision 1 (line 17), "a random 128-bit token".
- ADR 0018, decision 1 (lines 29-36), the list of what the plugin carries,
  where it leaves out the `promptwire` skill. ADR 0026 already added `afk`.
- ADR 0042, decision 4 (lines 50-52), "except `committed`, which compares
  the working tree with it, and `feature pointer`, which asks git what it
  ignores", as the only lines that look at the disk.

## Context

AGENTS.md says an ADR changes only by adding one that supersedes it. ADR 0026
set the form for such a correction. The re-audit of teamwire at `b8d10cbb`
(team `reaudit`, 2026-11-16) found more sentences that the code no longer
follows. In each case a later ADR, a requirement row, or a code comment calls
the new behaviour deliberate, but no ADR names the old sentence as
superseded. The lead of team `reaudit` decided that such a sentence stays
wrong until an ADR names it. This ADR names each one. It changes no program
behaviour.

## Decision

1. **A forward table.** Each sentence listed above is shown with what the
   program does now and what made the change. The old ADR files stay as they
   are.

   | Old ADR and point | What the program does now | Changed by |
   | --- | --- | --- |
   | 0021, decision 3 | `acked` says that the recipient took the message in by one of its own actions: `inbox`, `ack`, the lead's `teamwire received <id>` for a row that was never typed (`crates/teamwire-report/src/received.rs:96-99`), or an answer whose `re` names the message (`crates/teamwire-msg/src/send.rs:387-405`). Each writes `delivery.acked`, and `teamwire.messages.unread` counts none of them | ADR 0043, decisions 2 and 3; DEL-19 |
   | 0022, decision 4 | The same as the row above. ADR 0022's own rule still adds no way to reach `acked` | ADR 0043, decisions 2 and 3 |
   | 0009, decision 3 | Only `close --role` accepts a verify record, and only one whose sha equals the member's HEAD now (`crates/teamwire-close/src/remove.rs:86-127`). The whole-team close reads no verify record: `team_plan` (`crates/teamwire-close/src/plan.rs`) checks busy, unplaced and dirty members and the AFK report, and the only verify step in `crates/teamwire-close/src/team_close.rs` (`verify_archive`) checks the archive by content | ADR 0046, decision 4; CLS-9 |
   | 0045, decision 4 | The same as the row above. A carried waiver still does not replace a new verify run for `close --role` | ADR 0046, decision 4; CLS-9 |
   | 0030, decision 5 | Launch prints `watch: call the teamwire_watch tool with team <team>` for a pi lead, whose watch start is `WatchStart::ExtensionTool` (`crates/teamwire-deliver/src/lines.rs:319, 345`; `crates/teamwire-harness-pi/src/lib.rs:120`). The lead extension refuses `teamwire watch` in pi's shell, so the lead is never given that command. `reads_watch_lines` is still true, and launch still skips the locator check of ADR 0023 point 9 | commit `b767d758`; `docs/api/cli.md` (the launch `watch:` line); test `crates/teamwire-cli/src/cmd/hook/member_tests.rs` `a_pi_lead_is_told_to_call_the_watch_tool` |
   | 0029, decision 2 | pi's `reads_watch_lines` is true (`crates/teamwire-harness-pi/src/lib.rs:119`), because the lead extension hands each watch line to the model | ADR 0030, decision 5. ADR 0030's status line names only ADR 0029 points 1 and 9 |
   | 0023, decision 11 | teamwire reads a Codex member's rollout file for the context fill: the newest `token_count` event's `last_token_usage.input_tokens` over `model_context_window` (`crates/teamwire-harness-codex/src/session.rs:195-211`). `final_message` still comes from the hook input | ADR 0027, decision 2 |
   | 0030, decision 4 | `current_session` no longer exists. Every caller passes the environment to `caller_session(get)` (`crates/teamwire-core/src/harness.rs:38`) | ADR 0031, decision 8. ADR 0031's status line names ADR 0023 point 8 only |
   | 0019, decision 3 | The second Enter is the stored event `delivery.reentered`, written with the place (`crates/teamwire-deliver/src/deliver.rs:64-78`) | ADR 0020, decision 3. ADR 0020's status line names only ADR 0019's first two Consequences bullets |
   | 0001, Decision | Three subcommands run for a long time: `teamwire watch <team>`, `teamwire mcp`, and `teamwire pr-watch` (`crates/teamwire-cli/src/cmd/pr_watch.rs`), which the lead starts to watch a pull request's checks. All three are ordinary processes that someone else starts and owns | EXT-1; ADR 0039 |
   | 0004, decision 1 | Adapter crates are still named only in the wiring module (`crates/teamwire-cli/src/wiring.rs`; the tidy rule `xtask/src/adapters.rs` `adapter_named_outside_wiring_fails`). The host choice is a closed enum `Herdr`, `Tmux`, `None` in `crates/teamwire-cli/src/cmd/host_pick.rs:12-31`, which doctor and wiring share on purpose (`host_pick.rs:1-4`), and doctor names the `herdr` and `tmux` programs (`crates/teamwire-cli/src/cmd/doctor.rs:278-282`). A new host needs a change in those two files as well as a new crate | this ADR |
   | 0008, decision 1 | The member token is 256 random bits, as 64 lowercase hex characters (`crates/teamwire-launch/src/member.rs:275-280`). The store still keeps only its SHA-256 | this ADR |
   | 0018, decision 1 | The plugin carries three skills: `teamwire`, `afk` and `promptwire`, the skill that picks vendor domain skills for a member's brief when the `promptwire` tool is installed (`crates/teamwire-harness-claude/src/lead_setup/plugin.rs:106-109`; test `the_eight_files_name_the_binary_and_the_hook`) | commit `456cccf2`; this ADR |
   | 0042, decision 4 | Three more spec-kit lines look at the disk. `spec-kit events` also warns for a `.codex/config.toml` or `.codex/hooks.json` that is only on disk, because Codex launch refuses it there (`crates/teamwire-cli/src/cmd/doctor_speckit/extensions.rs:147-157`; test `a_codex_file_only_on_disk_warns_because_codex_launch_refuses_it_there`). `spec-kit constitution` adds a note when the copy on disk differs, and its level still comes from the commit (`crates/teamwire-cli/src/cmd/doctor_speckit.rs:319-324`). `spec-kit integrations` looks at the disk only to choose between two warning texts (`crates/teamwire-cli/src/cmd/doctor_speckit/integrations.rs:148, 174`). Only the `events` line changes its level because of the disk | `docs/api/cli.md` `teamwire doctor`; the module note at `crates/teamwire-cli/src/cmd/doctor_speckit.rs:7-8`; STO-17 |

2. **Requirement rows follow the code.** These rows of `docs/requirements.md`
   now say what the program does, and each names this ADR or the ADR that made
   the change:

   | Row | Changed by |
   | --- | --- |
   | MSG-11 (what sets `acked`) | ADR 0043, decisions 2 and 3; this ADR |
   | INV-28 (the whole-team close reads no verify record) | ADR 0046, decision 4; this ADR |
   | STO-17 (which spec-kit lines look at the disk) | this ADR |
   | WAT-10 (the lead reports `TEAM-STALLED` and a `BLOCKED` member at a login, update or trust dialog) | the lead skill, commit `e036e9c2`; this ADR |
   | WAT-3 (a waking message and a handoff count as a prompt, `crates/teamwire-watch/src/status.rs:410-424`) | ADR 0043, decision 8; this ADR |
   | OBS-2 (a hung collector can delay the watch's exit by up to 3 s, `crates/teamwire-cli/src/cmd/watch.rs:29-33`) | this ADR |
   | MSG-18 (a tool result says `handed_off`, `crates/teamwire-mcp/src/tools.rs:379-388`) | ADR 0003, point 5; this ADR |
   | MCP-3 (the inbox resource is deferred, `docs/api/mcp.md`) | this ADR |
   | SEC-3 (pi members load the lead package, which does nothing there, `crates/teamwire-harness-pi/src/lead_extension.rs:1-3`) | ADR 0030, consequences; this ADR |
   | LCH-17 (the contract also names other tools' commands, as bans and as a seat's own steps, `crates/teamwire-launch/contracts/how_to_work.md:4-5, 24, 29-30`, `kind_reviewer.md:8`, `kind_closer.md:30`) | this ADR |

3. **A failure-mode row follows the code.** In `docs/failure-modes.md`, the
   row "A whole-team close fails after its places are closed" no longer says
   the watch reports `DIED` for every member. The watch never looks at a
   member whose result was delivered (`crates/teamwire-watch/src/plan.rs:356-360`),
   and `DIED` needs three absent polls in a row
   (`crates/teamwire-watch/src/plan.rs:495-511`) with a usable host listing
   (`crates/teamwire-watch/src/observe.rs:66-72`). WAT-6 and ADR 0043
   decision 7 call that settling deliberate.

## Consequences

- No existing file under `docs/adr/` changes. A reader of an old ADR finds
  the replacement here.
- The comment at the top of `crates/teamwire-msg/src/inbox.rs` no longer
  calls `inbox` and `ack` the only way to reach `acked`.
- This ADR covers only sentences where the code is right on purpose. It
  leaves alone the re-audit rows where the code should change, and the
  questions the user decides: ADR 0025 point 10 (whether an AFK closer runs
  the full gate) and PAR-9 (whether AFK runs ship for Codex and pi leads).
- The same failure-mode row still says the team is set back to `open`. When
  the final store write fails, the code can leave the team `closing`
  instead. That is a code question, so this ADR does not change the row's
  words about it.
