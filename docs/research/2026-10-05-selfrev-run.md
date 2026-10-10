# teamwire ran a team on itself: the selfrev run (2026-10-05)

A real smoke test of the Rust teamwire and a real piece of work: an adversarial review of every crate, then a fix run. One team, `selfrev`, eleven members over three waves, led from one Claude Code session. Archive with every brief, message, report and verify log: `~/.teamwire/archive/selfrev-2026-10-05/` (the lead's plan, smoke notes, specs and briefs are under `lead/`).

## Result

| | |
| --- | --- |
| Review findings | 44 (0 critical, 4 high, 19 medium, 21 low) across 5 crate groups |
| Fixed | 45 (44 findings minus 5 deferred, plus 6 lead additions during the run), on 5 branches `selfrev/fx_*`, 73 commits |
| Verify | every branch PASS; 25 waived flags, all `#[expect(...)]` suppressions plus 2 approved cross-crate edits |
| Closer | confirmed all 5 branches with its own runs; 17 findings, 3 worth a fix round; all fixed in round 2 |
| Full gate on the integrated tree | 6/6 ok (tests 289 s) |
| Integration | `selfrev/integration` → `develop` → `main` (fast-forward), tag `v0.1.0`, GitHub release |
| Wall time | about 2 h 20 min from first launch to close |

## How the run went

1. **Wave 1, review.** One pilot reviewer (opus) on core, store and platform, then four more with `launch --add`. Each read its crates against the invariants in `docs/requirements.md` and the failure-modes table, confirmed its serious findings with throwaway programs outside the repository, and wrote a report in a fixed format. Each report took 6 to 9 minutes.
2. **Wave 2, fix.** The lead triaged every finding and split them by owning crate into one brief per implementer (sonnet), each in its own worktree with `check` and `owns`. A pilot implementer first, then four more. Five questions came back through `teamwire send --type question`; each was answered within minutes and the member carried on. Every branch passed `teamwire verify` before any merge.
3. **Wave 3, closer.** Ran every check and the fast gate in every worktree itself, read every diff, compared the branches, and graded 17 findings. Three fix rounds went back as reprompts in one turn; all three members fixed them within 15 minutes.
4. **Integration.** Five branches merged with no conflicts into an integration worktree, three lead commits (two docs sentences the fixes made true, one to unify a rule two branches had written twice), full gate, then `teamwire close`: 92 files archived and hash-verified, 5 worktrees removed, 11 tabs closed.

## What the review found (the worst per group)

- **flow:** after a branch is merged into its base, `verify` compared the member's HEAD with itself and passed everything (high). Fixed.
- **adapt:** a numbered list in a reply read as a dialog under tmux, so delivery to an idle session was refused (high). Fixed.
- **cli:** a member could run `close --role`, `watch` and `launch --add`; no lead command checked the caller (high). Fixed, with the rule now in `docs/api/cli.md`.
- **comms:** a member's summary could forge a TEAM-STALLED event inside a watch line (medium). Fixed with one shared helper.
- **store:** the store itself was sound; the sleep-hold release reported "already gone" when `ps` failed, so close forgot a running hold (medium). Fixed.

## What the smoke test found about teamwire itself

1. **Launch accepted a background lead.** Claude Code parked the lead's turn into a background job; launch read the session kind from an environment variable Claude Code does not set, and recorded the job as lead. Fixed in this run (the session file's `kind` now decides). Still open: a parked job carries a different session id, so every lead proof needs `CLAUDE_CODE_SESSION_ID=<interactive id>` in front of it, and a sleep hold started for the job's pid was not released at close.
2. **Members were told to run a bare `teamwire`,** which is not on their PATH. Fixed in the contract text (full binary path); typed lines and reminders still say `teamwire`, follow-up.
3. **`launch --add` demanded a team title** the skill text says may be left out. Fixed.
4. **Two CLI close tests flake under machine load.** Follow-up.
5. Everything else worked first time: dispatch, trust, Stop-hook delivery (including into a parked lead), questions and answers, notes, reprompts with generations, verify with HEAD-bound waivers, the closer, and close.

## Deferred, each needing an API first

| Follow-up | Needs |
| --- | --- |
| Reused pid for the close lock; session files trusted on pid alone | a process start time from `Platform` |
| OTLP export of an open 60 s bucket (sent twice) | `claim_metric_export(max_seq)`; the exact API and test are in fx_adapt's report |
| `--key` resume of an old message reads as stale | `deliveries.attempt_started_at` column |
| `find_existing` reads a failed `ps` as "no hold" | `SleepHold::find_existing -> Result` (core trait, fakes, launch) |
| Lead commands from a parked job; the leaked hold | map a job to its parent interactive session (`parkedJobId`) |
| Typed lines still name a bare `teamwire` | a `teamwire_bin` config field filled from `current_exe` |

## Costs and shape, for the next run

- Reviewers on opus at 40 minutes were done in under 10. Implementers on sonnet fixed 3 to 14 findings each in 10 to 25 minutes, with clean clippy on the first gate run in every case.
- The pilot-first rule paid for itself twice: the pilot reviewer showed the bare-`teamwire` problem before four more members hit it, and the pilot implementer showed that cross-crate trait changes need a lead decision before the other four started.
- Giving one implementer a second crate (fx_cli got the store crate) avoided two dependency chains. Two cross-branch test interactions were caught by the members themselves and routed to the right branch through questions.
