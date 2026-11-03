# Planner report, team fixes3

1. All plan files are written in `afk/`: `understanding.md`, `plan.md`, `decisions.md` (D3-D13 appended), `briefs/report.md`, `briefs/msg.md`, `briefs/ui.md`, and `spec-wave1.json`.
2. Each member's setup and check ran green on a fresh clone of `bench` (e78c32be): report exit 0 in 36 s, msg exit 0 in 34 s, ui exit 0 in 5 s.
3. I prototyped all three fixes on those clones. Each new test failed on `bench` and passed with the fix, and the fast gate passed.
4. `teamwire launch --add --dry-run --spec afk/spec-wave1.json` exits 0. The plain `--dry-run` refuses with `team_exists`, because fixes3 already exists. The same spec under an unused team name passes the plain form.
5. `herdr_env_wins_over_tmux` in teamwire-cli fails on unchanged `bench` in this environment. I sent the lead finding m001.

## Progress

- Read the brief, the task, AGENTS.md, ui/AGENTS.md, docs/conventions.md and the code of all three areas.
- Ran `afk/tmp/fresh_check.sh` for each member on fresh clones, setup as `sh -c <setup> teamwire-setup <path>` and check as `sh -c <check>`. Results are in `afk/tmp/<role>.result` and `.log`.
- Prototyped each fix on its clone. The diffs are in `afk/tmp/*-prototype.diff`.
- Ran the dependents of both Rust prototypes with `--no-fail-fast`. msg: 63 binaries, 502 passed, 1 failed. report: 60 binaries, 495 passed, 1 failed. The failure was `herdr_env_wins_over_tmux` both times.
- Confirmed that the same test fails on code identical to `bench` (`afk/tmp/bench-tmux-wiring.log`). Sent the lead finding m001.
- Searched promptwire (react, rust, bun, sqlite, typescript). No skill fits (D10).
- Wrote the plan files and validated the spec. Deleted the three temporary clones. The repository is unchanged: `git status` is clean and there is no extra worktree.

## What I verified and what I inferred

Verified by running:
- On `bench`, a named-pipe transcript with no final message blocks `on_turn_end`. The probe's 10 s `recv_timeout` failed.
- On `bench`, a 64 MiB + 1 transcript is read: `Final message from: transcript`.
- On `bench`, a resend to closing or closed beta reports `(Undelivered, Some(TeamClosed), Undelivered, 0)` for a handed-off, read or pending copy.
- On `bench`, the sort test in teams.test.ts breaks once `hasProblem` counts the new reasons: it receives `["Alpha","Zed","Zzz"]`.
- Each prototype fix is green with its crate or ui tests and the fast gate.
- `bun test` can render `Overview` with `renderToStaticMarkup` from a `.test.ts` file.
- `cargo test -p teamwire-cli --locked --test preset` passes (10 tests), so the docs edit for msg has a quick test.

Inferred, not run:
- The `unconfirmed` and `undelivered` rows in the receiving team (D4) behave as the brief says. The mapping follows `mirror`'s rules, but I did not prototype those two cases.
- The tidy `WEAKENED` flag would also appear in `teamwire verify`. I saw it in the gate only.

## Ruled out

- A ui schema change, a ui lint step, a new dependency for `mkfifo`, a doc change for fix 1, other readers of `hasProblem`, and file-size limits. The evidence is in plan.md under "Ruled out".

## Decisions I made

- D3 to D13 in `afk/decisions.md`. They cover the shared transcript reader in usage.rs, the outcomes for `unconfirmed` and `undelivered` rows in the receiving team, the change living in `deliver_to_team`, the literal `<n> question waiting` label, the exact `lead` party with `state === "open"`, the reasons as `.flag` spans tested by `Overview.test.ts`, the sort-test input fix, no promptwire skills, the check commands, no `test-changed.sh` for members, and msg owning the two resend docs.
- I validated the spec with `--add --dry-run` and a renamed copy, because the brief's plain `--dry-run` refuses an existing team. I did not change the spec to get around that.
- I gave msg ownership of `crates/teamwire-cli/tests/preset.rs`, read only. The brief requires every file that reads what a member changes to be in exactly one `owns`, and that test reads `docs/api/mcp.md`.
- I used whole-crate `owns` for report and msg (`crates/teamwire-report/`, `crates/teamwire-msg/`). Every file that reads or pins their change sits in that crate.
- Each Rust check adds the fast gate to the crate tests, because the task requires the gate and `teamwire verify` then catches fmt, tidy and clippy problems.

## Rework after the plan review (lead message m002)

1. Finding 1 holds as a reading question, and the lead settled it as D-L2 (keep D4). In `briefs/msg.md` I added D-L2 under "Decisions already made" and marked the `undelivered` table row with it. "Done when" now names the test `a_copy_that_surely_never_arrived_stays_undelivered_when_its_team_closes` and requires a comment on that match arm in remote.rs saying why. Unit 2 does the same.
2. Finding 2 holds. A whole-page check passes when the reasons sit only in the heading or on the wrong card. I tested a per-card test on a fresh clone with the ui prototype. It passed on the correct card, failed with payments' reasons also on the docs card, and failed with the reasons only in the page heading. `bun test src/web` gave 46 pass, 0 fail, and typecheck exited 0. Splitting the page on `<li>` fails, because the `lost` card lists its errors as `<li>` items. `briefs/ui.md` ("Done when", the seams sentence, D15, "What already exists", unit 3) and `plan.md` (the fix 3 tests) now require each reason on its own card and on no other card, with at least two teams. The tested file is `afk/tmp/Overview.per-card.test.ts`. I deleted the clone afterwards.
3. The reviewer's question is the D4 question. I answered it as D14: I agree with D-L2, and it needs no brief change beyond item 1.
4. The reviewer's note on D5 is correct, and I verified it. `send` runs `rules::check` first (crates/teamwire-msg/src/send.rs:64). That runs `foreign::check` (rules.rs:77), which refuses a recipient team that is not open (rules/foreign.rs:41-42). So a `send` retried with the same client key does not reach the new branch, except when the team starts closing between that check and the delivery. D5's choice stands, and no brief repeats the wrong reason. The wrong sentence is still in D5 and in understanding.md, because the lead said to change nothing else.
5. I did not touch spec-wave1.json, so I did not rerun the dry run.

Decisions I made in the rework:
- D14 and D15 in afk/decisions.md.
- I left D5's wrong reason in place and reported it here, following the lead's "change nothing else".
