# revui report: review of fixes3/ui (fix 3, the dashboard says why a team needs a look)

Summary:
1. Verdict: the branch is sound. I found no correctness, security or data-loss defect, and no requirement missing or built wrong.
2. The new function `problemReasons` in `ui/src/web/model/teams.ts:71-84` matches the task's table: five labels, in order, with the first three only while `state === "open"`.
3. `hasProblem` keeps every old check and adds the reasons (`teams.ts:63`). The overview card renders each reason as a `.flag` span (`ui/src/web/Overview.tsx:88-96`).
4. I broke the fix seven ways in a temporary clone. A new test failed every time, so no new test passes without its fix.
5. In my clone, `bun test` passed (170 pass, 3 skip, 0 fail), `bun run typecheck` exited 0, and fmt and tidy passed with no WEAKENED or PROTECTED-CHANGED line. I did not run clippy (see below).

## Progress

- Read the brief, the task, the member brief, decisions.md, the member report, and the diff `e78c32be...fixes3/ui` (3 commits: 17fceae, df8fd9b, 5a002b0).
- Read the connected code: `teams.ts` in full, `Overview.tsx` in full, the schemas in `ui/src/shared/teamwire.ts:14-152`, `ui/src/web/model/route.ts`, and both fixtures.
- Cloned the repository to `/private/tmp/claude-501/bnruns/bne68n09a4/twhome/teams/fixes3/afk/tmp/revui-clone`, checked out `fixes3/ui` (5a002b06), and copied `ui/node_modules` from the ui worktree. I ran no install.
- Ran the tests, the typecheck, seven mutations, fmt and tidy in that clone.
- Wrote this report.

## Findings

None.

## What I checked, and the result

| Requirement (quoted) | Where | Result |
| --- | --- | --- |
| "The lead's liveness category is failed or attention" gives "the lead's liveness label from `livenessInfo`" | `teams.ts:75-76` | Correct. `livenessInfo` decides, so `dead` and `gone` give `failed`, and `blocked` gives `attention`. A `null` lead, `"unknown"` and `{alive: "unknown"}` give `unknown` and no label. |
| "The watch state is `stale`" gives `watch stale` | `teams.ts:77` | Correct. |
| "The AFK budget is spent (`afk.spent`)" gives `AFK spent` | `teams.ts:78` | Correct. `afk` is nullish in the schema, and `s.afk?.spent === true` handles a missing key. |
| "Only while the team is open" for the first three | `teams.ts:74` | Correct. A `null` state counts as not open (D7). |
| "Unread deliveries exist" gives `<n> unread`, open or not | `teams.ts:80` | Correct. |
| "Questions to the lead waiting 600 s or more" gives `<n> question waiting`, open or not | `teams.ts:81-82` | Correct. The check is `to.includes("lead")` and `age_s >= 600`, so `lead@beta` does not count (D7). The label stays singular for every n (D6). |
| "`hasProblem` is also true whenever `problemReasons` returns anything" | `teams.ts:63` | Correct. Every old check is kept. |
| "The overview card shows the reasons" | `Overview.tsx:55, 88-96` | Correct. Each reason is one `<span className="flag">` inside one `<p className="chips">`, before the stuck-delivery flags (D8). A team with no name still shows its reasons inside its `<div>` card. React keys cannot collide, because the five labels always differ. |
| Labels are not parsed as HTML | `Overview.tsx:90-94` | Safe. Each label is fixed text plus a number or a schema enum value, rendered as a React text child. |
| Fixtures give `["1 unread", "1 question waiting"]`, `["watch stale"]` and `[]` | `teams.test.ts:127-133` | Tested, and passes. |
| Per-card overview test, not a whole-page search (D15) | `Overview.test.ts:11-15, 17-31` | Built as asked. The test takes each card as its `<a class="team…" href="#/team/<name>">` element and checks every reason against every card as `>reason<`. |
| Old sort test still expects `["Zzz", "Alpha", "Zed"]`, with D9 applied | `teams.test.ts:42, 45` | Correct. Only `unread_deliveries: []` and `unanswered_questions: []` were added. The expected value is unchanged. |
| No change outside the owns list | `git diff --stat` | Only the four owned files changed. `ui/src/shared/`, `ui/fixtures/`, `package.json`, `tsconfig.json` and `bun.lock` are untouched. |

## Mutation check: does each new test fail without its fix?

I changed the code in my clone, ran `bun test src/web`, and restored the file after each run.

| Mutation | Test that failed |
| --- | --- |
| The open check removed (`if (true)`) | lists all problem reasons in a fixed order |
| Threshold changed to `age_s > 600` | counts a question only to the lead and only after 600 s |
| `lead@beta` counted (`startsWith("lead")`) | counts a question only to the lead and only after 600 s |
| `hasProblem` ignores the reasons | a team whose only signal is a reason is a problem and sorts first |
| The `attention` category dropped | gives each problem reason its own label |
| The card renders no reasons | each card shows its own team's problem reasons and no other team's |
| Every card renders all three fixture reasons | each card shows its own team's problem reasons and no other team's |

Each mutation failed exactly one test, and the other 50 tests in `src/web` passed.

## Commands I ran, in the clone at 5a002b06

```
cd ui && bun test               -> 170 pass, 3 skip, 0 fail
cd ui && bun run typecheck      -> tsc --noEmit, exit 0
cargo fmt --all -- --check      -> exit 0
cargo xtask tidy --base e78c32be -> "tidy: ok", exit 0, 0 WEAKENED and 0 PROTECTED-CHANGED lines
```

The 3 skipped tests come from `describe.if(process.env.TW_UI_LIVE === "1")` in `ui/src/shared/teamwire.test.ts:61`. They are skipped on the base too, and the branch does not touch them.

I did not run the clippy step of `scripts/gate.sh fast`. The branch changes no Rust file, so clippy sees the same code as on the base. A fresh clone would also need a full workspace build first. The member reports that the full fast gate passed in its worktree.

## Decisions I made

- I ran the tests in a fresh clone with `ui/node_modules` copied from the ui worktree, instead of running `bun install`. My seat rules forbid installs, and a copy writes nothing into the member's directory.
- I ran fmt and tidy from the fast gate and skipped its clippy step, because the diff holds no Rust code. Tidy is the step that reports WEAKENED and PROTECTED-CHANGED lines, and it showed none.
- I left the clone at `/private/tmp/claude-501/bnruns/bne68n09a4/twhome/teams/fixes3/afk/tmp/revui-clone` in place, so the closer can reuse it. The mutations were restored there, and `git status` in it is clean apart from the copied `ui/node_modules`, which git ignores.
