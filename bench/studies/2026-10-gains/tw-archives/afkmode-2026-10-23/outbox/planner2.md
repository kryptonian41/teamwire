<!-- teamwire:generated completion=01M5MKTRPKHN8X1W5YA28BHZ81 -->
# Result from planner2 (done)

- Team: afkmode
- Status: done
- Generation: 2
- Completion: 1
- Time: 2026-10-23T14:49:53.107Z
- Final message from: hook
- Member report: /private/tmp/claude-501/bnruns/bnei8xdqkf/twhome/teams/afkmode/reports/planner2.md

## Summary

Plan ready: 4 sonnet implementers in one wave (start, stophold, endings, skill), D3-D23, briefs, spec-wave1.json (launch with --add; dry run exit 0), base checks pass on a fresh clone. See reports/planner2.md.

## Task (first 300 characters)

# Brief: planner2 for the AFK run "afkmode" ## 1. Who you are You are planner2, an investigator (read-only for the repository). The lead runs this team while the user is away, so nobody will answer questions from the user. Other members, added later by the lead: a plan reviewer (reads your plan), i…

## Final message

The plan is ready: four implementers in one wave, all on sonnet at high effort, all based on `main` at fb1fd8f. Every check passed when I ran it on a fresh clone of that commit.

| Implementer | What it builds |
| --- | --- |
| `start` | The `teamwire afk start` command, launch's budget refusal and model warning, and the fourth parameter on the store's `create_afk_team` |
| `stophold` | The AFK hold in the lead's Stop hook, through `afk_hold_notes` |
| `endings` | Close's check for a complete REPORT.md, the AFK line in `status` and the mark in `teams`, and the ui zod schemas |
| `skill` | The AFK skill, install adding it to the plugin, ADR 0025, and every doc file |

- **Launch with `--add`.** Team afkmode already exists. The plain `--dry-run` refuses with `team_exists`. The dry run with `--add` exits 0 and prints only the expected warnings that sonnet is below the AFK table.
- **Base checks.** I ran each implementer's check and `scripts/gate.sh fast` on a fresh clone, because the first planner saved no proof of its run. All exited 0. The first check took 90 s, including the cold build. The others took 59 to 84 s, and the fast gate took 13 s.
- **Gaps the closer must cover.** `scripts/gate.sh` never runs `bun test` in ui/, so the closer must run it on the joined branch. Every implementer check skips one test, `herdr_env_wins_over_tmux`, because it fails inside tmux on the base commit.
- **Merge order.** Merge start, then endings, then stophold, then skill. After `start` merges, the test helper that creates AFK teams in endings and in stophold needs one more argument, `&serde_json::Value::Null`. That is a one-line fix in each branch (decision D21).
- **The installed teamwire binary already has AFK mode.** Its status line and its `(open, afk)` mark in `teams` match what I chose. I changed the launch warning text to match it too.

I settled 21 open details of the design as D3 to D23 in `decisions.md`. The first planner's `understanding.md` was correct, so I kept it and only updated the build time. I added the two ui fixture files to what `endings` owns, because nobody owned them.

I did not time `scripts/test-changed.sh`. I left the D-entry dates in local time, while status timestamps are in UTC. I stay open to answer implementers' questions.

Files are in /private/tmp/claude-501/bnruns/bnei8xdqkf/twhome/teams/afkmode/afk:
- plan.md
- decisions.md
- planning-check.md
- spec-wave1.json
- briefs/start.md, briefs/stophold.md, briefs/endings.md, briefs/skill.md

My report is /private/tmp/claude-501/bnruns/bnei8xdqkf/twhome/teams/afkmode/reports/planner2.md
