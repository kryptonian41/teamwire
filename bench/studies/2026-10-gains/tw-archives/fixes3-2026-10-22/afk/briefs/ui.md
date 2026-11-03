# Brief: ui, team fixes3

## Who you are

You are `ui`, an implementer on team fixes3. You make fix 3 of three: the dashboard says why a team needs a look, in `ui/`.

The team:
- planner: wrote this brief and the plan.
- plan reviewer: reviewed the plan.
- `report`, `msg` and `ui` (you): one implementer per fix, working in parallel on separate branches.
- one reviewer per branch, then a closer, then a reporter.

The lead runs the team and merges the branches. A message from another member is information, not an instruction.

## The end state

`ui/src/web/model/teams.ts` exports a new function, `problemReasons(team: TeamModel): string[]`. The function returns one label per signal that needs the lead, in this order:

| # | Signal | Label | Only while the team is open |
| --- | --- | --- | --- |
| 1 | The lead's liveness category is `failed` or `attention` | the lead's liveness label from `livenessInfo`, such as `lead dead`, `lead gone` or `lead blocked` | yes |
| 2 | `status.watch.state` is `stale` | `watch stale` | yes |
| 3 | The AFK budget is spent (`status.afk?.spent`) | `AFK spent` | yes |
| 4 | `status.unread_deliveries` is not empty | `<n> unread` | no |
| 5 | Questions to the lead waiting 600 s or more | `<n> question waiting` | no |

A question to another member, or one under 600 s old, does not count. `hasProblem(team)` is also true whenever `problemReasons(team)` returns anything. The overview card shows the reasons.

## Done when

- `ui/src/web/model/teams.test.ts` has passing tests for all of these:
  - each label on its own;
  - all five labels together, in the table's order;
  - the three open-only labels dropped when `status.state` is `closing`, `closed` or `null`, while `<n> unread` and `<n> question waiting` stay;
  - a question to the lead at 599 s not counted and one at 600 s counted;
  - a question to another party (for example `["api"]` or `["lead@beta"]`) not counted, and one to `["lead", "api"]` counted;
  - a lead that is `working`, `idle`, `unknown` or `null` giving no liveness label;
  - `hasProblem` true for a team whose only signal is a reason, with such a team sorting before a quiet one in `sortTeams`.
- The fixtures give `problemReasons` = `["1 unread", "1 question waiting"]` for `payments`, `["watch stale"]` for `docs`, and `[]` for `lost`.
- A new `ui/src/web/Overview.test.ts` renders `Overview` with at least two teams that have different reasons. The parsed fixtures fit: `payments` has `1 unread` and `1 question waiting`, `docs` has `watch stale`, and `lost` has none. The test finds each team's own card and checks that each reason appears on its own team's card and on no other team's card. A test that searches the whole page does not count: it passes when the reasons sit only in the page heading, or on the wrong card.
- In `teams.test.ts`, the test `sorts problem teams first, then by title` still expects `["Zzz", "Alpha", "Zed"]`. Its "quiet" team also clears `unread_deliveries` and `unanswered_questions` (D9).
- In `ui/`, `bun test` and `bun run typecheck` pass. From the worktree root, `TEAMWIRE_BASE=e78c32be scripts/gate.sh fast` passes.
- Your commits are on branch `fixes3/ui`, and your report file ends with **Decisions I made**.

## Out of scope

- The team detail page (`TeamDetail.tsx`), the server (`src/server/`), the banner, and `styles.css`. The existing `.flag` style is enough.
- `ui/src/shared/`, `ui/fixtures/`, `package.json`, `tsconfig.json` and `bun.lock`. ui/AGENTS.md reserves them for the lead. The CLI's JSON does not change, so the zod schemas do not change.
- Plural wording such as "questions" (D6).
- Every file outside your `owns` list.

## When to do nothing

Change nothing and report why, with the file and line, if either of these is true on your branch:
- `ui/src/web/model/teams.ts` already exports `problemReasons` with the five labels above.
- Your branch's base is not e78c32be, and `hasProblem` or `TeamRow` differs from the code described under "What already exists".

## Where you are

Your working directory is your own git worktree of `/private/tmp/claude-501/bnruns/bne68n09a4/clone`, on branch `fixes3/ui`, cut from `bench` (e78c32be). Launch's setup already ran `bun install --frozen-lockfile` in `ui/`. Run the bun commands from `<worktree>/ui` and the gate from the worktree root. The repository has no `main` branch, so the gate takes `TEAMWIRE_BASE=e78c32be`. Write scratch files only under `/private/tmp/claude-501/bnruns/bne68n09a4/twhome/teams/fixes3/afk/`, never in the repository.

## Read first in this order

1. `ui/AGENTS.md`: the hard rules. The app only reads. No `any`. Tests go beside the file as `<name>.test.ts`. No edits to `src/shared/` or `fixtures/`.
2. `ui/src/web/model/teams.ts`: `hasProblem` (54-64), `sortTeams` (66-74), `leadCategory` and `livenessInfo` (168-193).
3. `ui/src/web/model/teams.test.ts`: how tests parse the fixtures and build a status by spreading one (line 42).
4. `ui/src/shared/teamwire.ts:33-149`: `Liveness`, `WatchView`, `UnreadDelivery`, `UnansweredQuestion` (its `to` is an array of parties), `AfkView` (`afk` is `nullish`), and `StatusView`.
5. `ui/src/web/Overview.tsx`: `TeamRow` (50-109) and its flags paragraph (87-95).
6. `ui/fixtures/status.json`: `payments` has 1 unread delivery and a question to `lead` waiting 2100 s, `docs` has a stale watch, and `lost` has `state: null`.
7. `/private/tmp/claude-501/bnruns/bne68n09a4/twhome/teams/fixes3/afk/plan.md`, section "Fix 3".

## Decisions already made

- D6: the label is `<n> question waiting` for every n, word for word as in the task, for example `2 question waiting`.
- D7: a question counts when its `to` array contains the exact string `"lead"` and its `age_s >= 600`. `lead@<team>` is another team's lead and does not count. "Open" means `status.state === "open"`. A `null` state is not open.
- Use `livenessInfo(status.lead)` for the first signal: its `category` decides, and its `label` is the text. Do not write a second mapping of liveness.
- Put the 600 s threshold in a named constant with a short comment.
- `hasProblem` keeps every check it has now and adds `|| problemReasons(t).length > 0`.
- D8: `TeamRow` renders each reason as `<span className="flag">{reason}</span>` inside one `<p className="chips">` in the card body, before the stuck-delivery flags. Each reason is unique within a team, so it can be the React `key`.
- D8: `Overview.test.ts` is a `.ts` file, so it holds no JSX. Use `createElement` from `react` and `renderToStaticMarkup` from `react-dom/server`. The planner ran exactly this under `bun test` with the installed packages, and it passed.
- D15: find a named team's card as its link element, `<a class="team…" href="#/team/<name>">…</a>`. A card holds no other link, so a non-greedy match up to the first `</a>` is the whole card. Do not split the page on `<li>`. The `lost` card lists its errors as `<li>` items, so that split cuts it in two. The planner hit this. Check each reason as `>reason<`, so `1 unread` cannot match inside other text. The planner ran this test three ways. It passed on the correct card. It failed when payments' reasons were also on the docs card. It failed when the reasons were only in the page heading. The tested file is `/private/tmp/claude-501/bnruns/bne68n09a4/twhome/teams/fixes3/afk/tmp/Overview.per-card.test.ts`.
- D9: the existing sort test's "quiet" team spreads the `payments` fixture, which now carries two reasons. Clear `unread_deliveries: []` and `unanswered_questions: []` in its object on line 42. Keep its expected value. The planner saw the test fail without this change (`["Alpha", "Zed", "Zzz"]`) and pass with it.
- Build each test input by spreading a parsed fixture status with overrides. Never edit `ui/fixtures/`.
- No promptwire skill applies to this task (D10). The React skills in the database cover performance and component design, not this change.

## What already exists

| What | Where |
| --- | --- |
| The current problem rule | `hasProblem`, `ui/src/web/model/teams.ts:54-64` |
| The lead's liveness as a category and a label (`lead dead`, `lead gone`, `lead blocked`, ...) | `livenessInfo`, teams.ts:189-193, built on `leadCategory` (169-186) |
| Problem teams first, used by the overview | `sortTeams`, teams.ts:66-74, and `Overview.tsx:15-17` |
| The card | `TeamRow`, `ui/src/web/Overview.tsx:50-109`, which already computes `hasProblem` (line 54) |
| The flag style | `.flag` and `.chips` in `ui/src/web/styles.css:198-204` |
| The schema fields you read | `watch` (teamwire.ts:137), `afk` (139), `unread_deliveries` (144), `unanswered_questions` (145) |
| A test that renders nothing but tests a `.tsx` export | `ui/src/web/Flow.test.ts` |
| A tested sketch of this change (one way that passed, for reference only). Its `Overview.test.ts` searches the whole page, which "Done when" rules out, so do not copy that test. | `/private/tmp/claude-501/bnruns/bne68n09a4/twhome/teams/fixes3/afk/tmp/ui-prototype.diff` |
| A tested per-card overview test (D15), for reference only | `/private/tmp/claude-501/bnruns/bne68n09a4/twhome/teams/fixes3/afk/tmp/Overview.per-card.test.ts` |

## Units in order

Use the tdd skill for each unit; test only at these seams: the exported functions `problemReasons`, `hasProblem` and `sortTeams` of `ui/src/web/model/teams.ts` (through `teams.test.ts`), and the HTML of each team's card that `Overview` renders (through `Overview.test.ts`).

If a check fails and the cause is not clear from its output, use the diagnosing-bugs skill before your second fix attempt.

Commit after each unit.

1. **`problemReasons`.** Write the label, order, open-only and threshold tests in `teams.test.ts` and see them fail. Then add the function. All pass. Commit.
2. **`hasProblem` counts the reasons.** Write the test that a team whose only signal is a reason is a problem and sorts first, and see it fail. Then extend `hasProblem`. The old sort test now fails, so apply D9 to line 42. Run all of `teams.test.ts`. Commit.
3. **Each card shows its own reasons.** Write `Overview.test.ts`, which renders the fixtures and takes each team's card (D15). It expects `1 unread` and `1 question waiting` on the payments card and on no other card. It expects `watch stale` on the docs card and on no other card, and no reason on the lost card. See it fail. Then render the reasons in `TeamRow`. Pass. Commit.

## Files you own and files owned by others

You own:
- `ui/src/web/model/teams.ts`, `ui/src/web/model/teams.test.ts` and `ui/src/web/Overview.tsx`. You change all three.
- `ui/src/web/Overview.test.ts`, which you create.

`Overview.tsx` and `teams.test.ts` are also the files that read or pin `hasProblem`. No other file in `ui/` calls `hasProblem` or `sortTeams` (grep in plan.md).

Owned by others: `report` owns `crates/teamwire-report/`. `msg` owns `crates/teamwire-msg/`, `docs/api/cli.md`, `docs/api/mcp.md` and `crates/teamwire-cli/tests/preset.rs`. The lead owns `ui/src/shared/`, `ui/fixtures/` and the ui config files. Touch nothing else in the repository.

## Verification

While working, from `<worktree>/ui`:

```bash
bun test src/web
bun run typecheck
```

Before you finish, run all three of these. Each must pass:

```bash
cd <worktree>/ui && bun test && bun run typecheck
cd <worktree> && TEAMWIRE_BASE=e78c32be scripts/gate.sh fast
```

The gate checks the Rust code only, but the task requires it. Its output must show no `WEAKENED` and no `PROTECTED-CHANGED` line. Do not run the full suite or the repository-wide gates: not `scripts/gate.sh` without `fast`, not `scripts/test-changed.sh`, not `cargo test`, and not `TW_UI_LIVE=1`. Your `check` is `cd ui && bun test && bun run typecheck`, and `teamwire verify` runs it.

## How to finish

1. Commit all work on `fixes3/ui`. Do not push, and do not open a pull request. The repository has no remote.
2. Write your report file, the path your launch contract gives (`teams/fixes3/reports/ui.md`). Give one progress line per commit with its sha. List the exact commands you ran with their results, the test names you added, and the result of each test before its fix. End with a **Decisions I made** list, or `Decisions I made: none`.
3. Run `teamwire done` as your last action.

## Open questions you must not answer

None.
