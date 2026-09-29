# Failure modes

Each row is one way things go wrong: how teamwire notices it, what it does,
and the test that proves it. Test names are the names the build-plan tickets
must use, as `<path>::<test function>`. Every test runs against the fakes in
`tests/fakes/` with `TEAMWIRE_HOME` set to a temporary directory. "Fake host"
means `FakeHost` (in-process) or the fake `herdr` binary, whichever the crate
under test uses.

## 1. The invariants (one row or more for each INV in `docs/requirements.md`)

| Failure | Detected by | teamwire does | Test that proves it |
| --- | --- | --- | --- |
| INV-1: another session has taken over the lead's place | The host says the place holds a different session | Types nothing; result `undelivered` with `host_other_session`; the watch prints `UNDELIVERED` once | `crates/teamwire-deliver/tests/locator.rs::takeover_session_in_lead_place_gets_nothing` |
| INV-1: the host's listing fails or times out | Host call returns an error | Types nothing; `undelivered` with `host_listing_failed` | `crates/teamwire-deliver/tests/locator.rs::host_listing_failure_refuses` |
| INV-1: the lead's session is open in two places | The host confirms the session in two places | Refuses with `two_places` | `crates/teamwire-deliver/tests/locator.rs::session_in_two_places_refuses` |
| INV-1: the place changes hands between lookup and send | Second host check just before submit | Refuses; nothing typed | `crates/teamwire-deliver/tests/locator.rs::occupant_changes_before_submit_refuses` |
| INV-1 (member side): a replaced or dead recipient | Locator finds no live process for the recorded session | Delivery `undelivered`, sender exits 4 | `crates/teamwire-msg/tests/send.rs::replaced_recipient_is_not_delivered` |
| INV-2: the lead was resumed with a new pid and a new place | Locator starts from the session id | Delivers to the new place | `crates/teamwire-deliver/tests/locator.rs::resumed_lead_in_new_place_receives` |
| INV-2: a stale registry entry beside the live one | Two processes claim the session, the host confirms one place | Delivers to the confirmed place | `crates/teamwire-deliver/tests/locator.rs::stale_claimant_ignored_when_host_confirms_one` |
| INV-3: launch from a background session | Harness reports the caller's session kind as background | Exit 1, no directory, zero host calls | `crates/teamwire-launch/tests/refusals.rs::background_lead_refused_before_anything` |
| INV-4: a repository holds cross-session messages | Not detectable from outside | Never uses that route; default sink is host typing | `crates/teamwire-harness-claude/tests/routes.rs::default_route_is_host_typing` |
| INV-5: the host says the prompt stalled or timed out | Host error code `Stalled` or `Timeout` | Delivery `unconfirmed`; no resend; a reprompt keeps its new generation | `crates/teamwire-deliver/tests/retry.rs::stall_is_unconfirmed_and_not_retried`; `crates/teamwire-msg/tests/reprompt.rs::unconfirmed_reprompt_keeps_generation` |
| INV-5: the first message to a new member stalls | Same | One submit call only; prints the manual command | `crates/teamwire-launch/tests/dispatch.rs::stalled_dispatch_not_resent` |
| INV-6: the member stops again after its result was delivered | Newest completion is `delivered` | Reports nothing | `crates/teamwire-report/tests/turn_end.rs::second_stop_reports_nothing` |
| INV-7: the lead cannot be reached for a result | Locator or host failure | Result file written, completion `undelivered` with reason, one `UNDELIVERED` watch line | `crates/teamwire-report/tests/turn_end.rs::unreachable_lead_marks_undelivered`; `crates/teamwire-watch/tests/poll.rs::undelivered_result_printed_once` |
| INV-7: two failures for one member in one second | Two completions, two ids | Two records, two watch lines, nothing overwritten | `crates/teamwire-watch/tests/poll.rs::two_undelivered_same_second_two_lines` |
| INV-8: the watch touches a session | Fake host counts every call by kind | The watch makes only read calls | `crates/teamwire-watch/tests/poll.rs::watch_never_submits_or_sends_keys` |
| INV-9: an event would repeat on every poll | `announcements` table | Announced once per key; restart does not repeat | `crates/teamwire-watch/tests/poll.rs::late_announced_once_over_thirty_polls`; `crates/teamwire-watch/tests/poll.rs::restarted_watch_does_not_repeat` |
| INV-9: several events in one poll | Poll collects all events first | One line joined with ` \| ` | `crates/teamwire-watch/tests/poll.rs::three_events_one_line` |
| INV-10: a fresh member is idle during startup | Member age under 90 s and never seen working | No quiet counting | `crates/teamwire-watch/tests/poll.rs::startup_grace_no_silent` |
| INV-11: the host shows a member `idle` or `done` | Ignored for completion | Completion only from the completions table and liveness | `crates/teamwire-watch/tests/poll.rs::host_done_flag_is_not_completion` |
| INV-12: every member failed | Completion statuses | `TEAM-COMPLETE 0/3 done; not done: a failed, b failed, c blocked` | `crates/teamwire-watch/tests/poll.rs::team_complete_counts_only_done` |
| INV-12: a member died, the others finished | `DIED` announced for it | `TEAM-COMPLETE 2/3 done; not done: c died`; the watch exits instead of paging `TEAM-STALLED` | `crates/teamwire-watch/tests/poll.rs::dead_member_settles_team_complete` |
| A member goes quiet, is nudged, works, then goes quiet again | Seen working after `SILENT` starts a new episode | A second `SILENT` line | `crates/teamwire-watch/tests/poll.rs::silent_rearms_after_member_works` |
| A dialog closes and a later one opens | Host state leaves and re-enters `Blocked` | A second `BLOCKED` line | `crates/teamwire-watch/tests/poll.rs::blocked_rearms_after_dialog_closes` |
| INV-13: a member is added mid-run | Membership read every poll | The new member is watched; `TEAM-COMPLETE` waits for it | `crates/teamwire-watch/tests/poll.rs::added_member_is_watched` |
| INV-13: a member is removed, or the team closes | `removed_at`, team `state` | Removed member ignored; closed team ends the watch silently | `crates/teamwire-watch/tests/poll.rs::removed_member_not_died`; `crates/teamwire-watch/tests/poll.rs::closed_team_ends_silently` |
| INV-14: a finished member gets new work | Generation changed | Counters and timers reset; next completion is a new result | `crates/teamwire-watch/tests/poll.rs::reprompt_rearms_member`; `crates/teamwire-report/tests/turn_end.rs::completion_after_reprompt_is_new_result` |
| INV-14: a member was prompted outside teamwire | Latest completion delivered, harness says working | Status shows "prompted outside teamwire" | `crates/teamwire-watch/tests/status.rs::outside_prompt_shown` |
| INV-15: the transcript on disk lags the turn | Hook input carries the final message | Hook field used; result says `final: hook` | `crates/teamwire-report/tests/result_file.rs::hook_message_wins_over_transcript` |
| INV-16: a member wrote its own file at the result path | No generated marker in the file | File moved to `reports/<role>.rescued-<utc>.md` and linked | `crates/teamwire-report/tests/result_file.rs::unmarked_file_rescued_and_linked` |
| INV-17: the stop is already a continuation (`stop_hook_active`) | Hook input flag | No hold; delivers | `crates/teamwire-report/tests/turn_end.rs::no_hold_when_hook_already_active` |
| INV-17: the report check itself errors | Error inside `report_gaps` | Treated as no gaps; delivers | `crates/teamwire-report/tests/gate.rs::check_error_means_no_gaps` |
| INV-18: report missing progress lines or decisions | `report_gaps` | Holds once with the facts; delivers on the next stop | `crates/teamwire-report/tests/gate.rs::hold_once_then_deliver` |
| INV-18 (no turn-end hook): same | `done` runs the check | First call returns gaps and stores nothing; second call stores | `crates/teamwire-report/tests/gate.rs::no_hook_harness_told_once` |
| INV-19: a merge or a rebase brought in foreign commits | Commits filtered by first-parent, non-merge, author time | Only the member's own commits need lines | `crates/teamwire-report/tests/gate.rs::merged_amendment_needs_no_lines`; `crates/teamwire-report/tests/gate.rs::rebased_stack_counts_own_commits_only` |
| INV-20: a summary contains `\n`, `\r`, ESC and a fake `TEAM-STALLED` | `sanitize::one_line` | One printable line, capped | `crates/teamwire-core/tests/sanitize.rs::forged_event_line_is_flattened` |
| INV-21: the liveness listing fails for ten polls | Listing error | No `DIED`; watch continues | `crates/teamwire-watch/tests/poll.rs::failed_listing_is_not_death` |
| INV-22: a multi-line contract | Contract rendered to a file | argv holds only the path | `crates/teamwire-harness-claude/tests/launch_spec.rs::contract_passed_as_file` |
| INV-23: host worktree command | Never called | Worktree made with `git worktree add` | `crates/teamwire-launch/tests/worktree.rs::worktree_made_with_git_and_base_sha_recorded` |
| INV-24: setup fails | Non-zero exit | Member refused; place closed; worktree removed without force; log kept; member row `failed`; the same role can be launched again with `--add` | `crates/teamwire-launch/tests/setup.rs::failed_setup_undoes_and_role_reusable` |
| A removed member's role is launched again | Member row `removed` | Refused; nothing touched | `crates/teamwire-launch/tests/refusals.rs::removed_role_never_reused` |
| A member runs `git add -A` in its worktree | `TASK.md` is in `info/exclude` | The brief is never committed, so verify raises no out-of-scope flag for it | `crates/teamwire-launch/tests/worktree.rs::task_md_written_and_excluded_from_git` |
| INV-25: the trust dialog also grants folder commands | Escalation markers on screen | Refused; lead told to answer it | `crates/teamwire-harness-claude/tests/trust.rs::escalated_trust_refused` |
| INV-26: last team on a lead closes | Hold record `owned` | Released only if owned and no other live team uses it | `crates/teamwire-platform/tests/sleephold.rs::release_only_last_owned` |
| INV-27: launcher's parent is a throwaway shell | Walk to the lead's harness process | Hold waits on the harness pid; none found means "no hold" said loudly | `crates/teamwire-launch/tests/sleephold.rs::hold_targets_harness_pid` |
| INV-28: removing an unverified implementer | Verify record missing or HEAD differs | Refused unless `--unverified` | `crates/teamwire-close/tests/remove.rs::unverified_implementer_refused` |
| INV-28: removing before the closer reported | Closer's completion missing | Refused unless `--unverified` | `crates/teamwire-close/tests/remove.rs::before_closer_report_refused` |
| INV-29: a member claims its check passed | verify runs the check itself | Real exit code recorded | `crates/teamwire-verify/tests/verify.rs::check_exit_code_is_recorded` |
| INV-30: a merged contract amendment | Diff base moved to the merge-base | Amendment not flagged | `crates/teamwire-verify/tests/verify.rs::merged_amendment_not_flagged` |
| INV-31: close while a member is busy, or busy state unknown | Host state or harness listing | Refused; nothing changed | `crates/teamwire-close/tests/close.rs::busy_member_refuses`; `crates/teamwire-close/tests/close.rs::unknown_busy_state_refuses` |
| INV-31: close over uncommitted work | `git status --porcelain` (ignoring `TASK.md`) | Refused | `crates/teamwire-close/tests/close.rs::dirty_worktree_refuses` |
| INV-31: archive differs from the original | `SHA256SUMS` check | Stops before deleting anything | `crates/teamwire-close/tests/archive.rs::archive_hash_mismatch_stops_close` |

## 2. Crashes, the machine, and other programs

| Failure | Detected by | teamwire does | Test that proves it |
| --- | --- | --- | --- |
| Crash in the middle of a store write | SQLite rolls back the open transaction at next open | Old state or new state, never a mix | `tests/crash/tests/kill_send.rs::kill9_during_send_keeps_integrity` (random `kill -9` over 200 runs; `integrity_check` is `ok`; every acknowledged id present once) |
| Crash after the message commit, before the host call | Delivery row still `pending` | `status` shows it stuck after 60 s; `send --key` or `resend <id>` resumes it; the key and id were printed before the host call | `crates/teamwire-msg/tests/send.rs::resume_with_key_delivers_pending_and_undelivered_only`; `crates/teamwire-cli/tests/send.rs::id_and_key_printed_before_host_call` |
| A message failed to deliver (`host_listing_failed`) and the sender tries again | Delivery `undelivered` | `resend <id>` retries the same message; a new identical send is not refused as a repeat | `crates/teamwire-msg/tests/resend.rs::resend_retries_failed_recipients_only`; `crates/teamwire-msg/tests/resend.rs::repeat_check_ignores_undelivered_copy` |
| A resend is attempted by another member | Actor is not the sender or the lead | Refused with `not_resendable`; nothing changes | `crates/teamwire-msg/tests/resend.rs::only_sender_or_lead_may_resend` |
| Crash after the host call, before the outcome is recorded | Row still `pending` | Resume may type it twice; the display id repeats and `inbox` returns it once | `tests/crash/tests/kill_send.rs::kill_after_submit_at_most_one_duplicate` |
| Crash while a result is being delivered | Completion left `delivering` | Next turn end delivers it again, once | `crates/teamwire-report/tests/turn_end.rs::delivering_left_by_crash_delivered_once_more` |
| The harness kills the Stop hook after `delivering`, and the member never ends another turn | `delivering_since` older than 90 s | Watch marks it `undelivered` (`delivery_interrupted`) and prints `UNDELIVERED` once | `crates/teamwire-watch/tests/poll.rs::interrupted_delivery_announced_once` |
| A host call inside the Stop hook hangs | The hook's 40 s deadline | Remaining calls get the time left; outcome `undelivered` (`budget_exhausted`) or `unconfirmed`; hook exits 0 before its 60 s limit | `crates/teamwire-report/tests/turn_end.rs::hanging_host_stays_within_budget` |
| A result's typing stalls (`agent_prompt_stalled`) | Completion `unconfirmed` | Watch prints `UNCONFIRMED <role> Result: <path>` once | `crates/teamwire-watch/tests/poll.rs::unconfirmed_result_announced_once` |
| A question to the lead stalls | Delivery `unconfirmed` | Watch prints `QUESTION-UNCONFIRMED` once | `crates/teamwire-watch/tests/poll.rs::unconfirmed_question_announced_once` |
| Crash while writing a readable copy (`.md`) | Hash in the store differs or file missing | `verify` flags it; `verify --repair-files` rewrites it | `crates/teamwire-verify/tests/files.rs::repair_rewrites_missing_copy` |
| Crash during launch, after the agent started | Member row `creating` with a place recorded | `close` finds and closes the place | `crates/teamwire-close/tests/close.rs::closes_member_left_creating` |
| Crash during close | Team `state = closing` with a dead owner pid | `doctor` reports it; `close --clear-stale-lock` clears it; close runs again from the first step | `crates/teamwire-close/tests/close.rs::stale_close_lock_cleared_and_rerun` |
| Two processes change different members at once | Per-member rows | Neither update lost | `crates/teamwire-store/tests/concurrency.rs::two_writers_two_members_no_loss` |
| Deferred read upgraded to a write | Every write uses `BEGIN IMMEDIATE` | Waits for the lock instead of failing | `crates/teamwire-store/tests/concurrency.rs::read_snapshot_does_not_fail_writer` |
| A corrupt row or unparseable JSON field | Parse error on read | Reported, skipped, never read as success | `crates/teamwire-store/tests/corrupt.rs::bad_row_skipped_and_reported` |
| Machine sleeps and wakes | Gap between polls over 3 intervals | Quiet counters reset; grace applies; no mass `SILENT`/`DIED` | `crates/teamwire-watch/tests/poll.rs::sleep_gap_resets_counters` |
| Two teamwire versions at once, the newer migrated | `user_version` above the binary's | Older binary exits 6 naming both versions; changes nothing | `crates/teamwire-store/tests/migrate.rs::newer_schema_refused_unchanged` |
| Two versions, same schema | Both write | Both work; each event records its writer version | `crates/teamwire-store/tests/migrate.rs::writer_version_recorded` |
| Two watches for one team | `watch_lease` row with a live pid | Second one refuses and names the first | `crates/teamwire-watch/tests/lease.rs::second_watch_refused` |
| A harness that never acknowledges (member never calls `inbox`) | Delivery stays `handed_off` | Nothing is retried; status shows unread ids per member | `crates/teamwire-msg/tests/inbox.rs::unacked_stays_handed_off_no_retry` |
| A harness that never reports stop (no turn-end hook) | Capability record | `done` records; the watch prints `COMPLETED` once | `crates/teamwire-watch/tests/poll.rs::watch_route_prints_completed_once` |
| A host call hangs | Timeout | Call returns `Timeout`; for submit that is `unconfirmed`; for launch that member fails and the rest continue | `crates/teamwire-launch/tests/refusals.rs::hanging_agent_start_fails_one_member_only` |
| The host returns busy then ready | Error code `NotReady` or `Busy` | Retries within the bound, then succeeds | `crates/teamwire-host-herdr/tests/errors.rs::pane_busy_retried_by_code` |
| The host returns an unknown error | Error code `Other` | No retry; `undelivered` with the detail | `crates/teamwire-deliver/tests/retry.rs::unknown_error_not_retried` |
| A check spawns children and hangs | Timeout | Whole process group killed; exit 124 | `crates/teamwire-platform/tests/run.rs::timeout_kills_process_group` |
| Disk full during a write | `SQLITE_FULL` | Transaction rolled back; exit 6 with free space; hooks still exit 0 | `crates/teamwire-store/tests/full.rs::disk_full_rolls_back` (a `max_page_count` limit stands in for a full disk) |
| Disk nearly full at launch | Free bytes under 200 MB | Launch refused | `crates/teamwire-launch/tests/refusals.rs::low_disk_refused` |
| The state directory is on a network filesystem | Platform check | `doctor` warns | `crates/teamwire-cli/tests/doctor.rs::network_fs_warned` |
| The OTel collector is down | Export error | Mark not moved; commands unaffected; export retried on the next run | `crates/teamwire-otel/tests/export.rs::collector_down_keeps_mark` |
| The exporter crashes after flush, before the mark | Mark unchanged | That batch is exported again once | `crates/teamwire-otel/tests/export.rs::crash_before_mark_reexports_once` |
| A hook is given garbage on stdin | Parse error | Exit 0, nothing changed | `crates/teamwire-cli/tests/hook.rs::garbage_stdin_exits_zero` |
| A hook runs in a session that is not a member | `TEAMWIRE_TEAM` unset | Exit 0 before opening any file | `crates/teamwire-cli/tests/hook.rs::non_member_returns_immediately` |
| A different session runs in a member's old place | Hook `session_id` differs from the member's | Exit 0, not treated as the member | `crates/teamwire-report/tests/turn_end.rs::other_session_is_not_member` |
| A member sends as another member or as the lead | Token or lead session mismatch | Refused with `refused.identity` | `crates/teamwire-msg/tests/identity.rs::wrong_token_refused`; `crates/teamwire-msg/tests/identity.rs::lead_without_session_refused` |
| A completion with an unknown status | Validation | Refused; nothing stored | `crates/teamwire-report/tests/done.rs::invalid_status_refused` |
| A message body changed on disk | Hash differs | `inbox` returns `hash_ok: false`; `verify --deep` flags it | `crates/teamwire-msg/tests/inbox.rs::hash_mismatch_reported` |
| An id counter row is damaged | Unique constraint on display id | Insert fails, transaction rolls back, exit 6; no id is reused | `crates/teamwire-store/tests/corrupt.rs::counter_damage_never_reuses_id` |
| The host cannot pass environment to the agent (herdr) | Capability records: no `session_env_via_config` and no `sets_env_at_spawn` | Launch refuses that member with `no_identity_channel` before creating anything | `crates/teamwire-launch/tests/identity.rs::no_identity_channel_refused` |
| A member started without its identity variables | `fake-herdr` clears the environment of every agent it starts | Identity arrives through the per-member `settings.json` `env`; the member's `send`, `done` and Stop hook all work | `crates/teamwire-launch/tests/identity.rs::identity_written_to_settings_env_mode_0600_outside_worktree`; `tests/e2e/tests/identity.rs::member_under_env_clearing_host_can_send_done_and_report` |
| A lead and a member use different harnesses | Adapters looked up per party in the registry | Lead proven and located with the lead's harness; member reached with its own | `crates/teamwire-deliver/tests/mixed.rs::lead_and_member_on_different_harnesses_route_per_party` (two `FakeHarness` instances registered as `fake-a` and `fake-b`) |
| A recorded adapter id is not in this binary | Registry lookup fails | Delivery `undelivered` with `unknown_adapter`; launch refused | `crates/teamwire-core/tests/registry.rs::unknown_and_duplicate_ids_refused`; `crates/teamwire-deliver/tests/mixed.rs::unknown_member_harness_is_undelivered` |
| DEL-15: the recipient shows a permission dialog, on a host that does not guard dialogs (tmux) | `Harness::screen_state` on the screen tail says `Dialog` | Nothing pasted, no Enter; `undelivered` with `not_at_input_prompt` | `crates/teamwire-deliver/tests/dialog.rs::no_enter_into_a_dialog_on_unguarded_host` |
| DEL-15: the host or harness reports the recipient `Blocked` | State check before submit | `undelivered` with `session_blocked`; zero submit calls | `crates/teamwire-deliver/tests/dialog.rs::blocked_session_refused_before_submit` |
| A burst of sends to one recipient | Rate limit | Later sends refused with `rate_limited` and logged | `crates/teamwire-msg/tests/send.rs::burst_refused_and_logged` |
