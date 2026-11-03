<!-- teamwire:generated completion=01M5YXPPJ686VWKA93BRTWM2WR -->
# Result from skill (done)

- Team: afkmode
- Status: done
- Generation: 3
- Completion: 3
- Time: 2026-10-27T14:54:49.926Z
- Final message from: hook
- Member report: /private/tmp/claude-501/bnruns/bn4tu2e1us/twhome/teams/afkmode/reports/skill.md

## Summary

Closer findings 1-4 fixed (5778a70): second-word command test seen red, refusal order, Claude-Code-only wording, README; check passed (440 tests, docs ok, gate fast ok)

## Task (first 300 characters)

# Brief: skill, the AFK skill, its install, and the docs ## 1. Who you are You are `skill`, an implementer in team `afkmode`. Nobody will answer a question from the user during this run; the lead answers questions, and every open detail below is already decided. The other members of wave 1, each on…

## Final message

All four closer findings held against the code, and I fixed each one. I committed the fixes on `afkmode/skill` and recorded completion as `done`. The check passes on the last commit: 440 tests passed with none failed, `docs check: ok`, and `scripts/gate.sh fast` passed fmt, tidy and clippy.

1. **Command test:** `the_afk_skill_names_only_commands_and_tools_that_exist` now checks the word after the command whenever that command has subcommands. I ran it red first. With `teamwire afk finish --team ...` in the skill it failed with "`teamwire afk finish` is not a command". After I put the skill back, all 11 preset tests passed. The main skill's test still checks only the first word, and I did not touch it.
2. **Refusal order:** `docs/api/cli.md` now lists `unknown_harness` first. `unknown_host` has its own row after the task-file row, which matches the order in `afk_check`.
3. **Codex lead:** `cli.md` now says AFK runs are "supported only for a Claude Code lead; no check refuses another harness (D38)". ADR 0025 point 10 says the same, and adds that a Codex lead can start a run nothing supports.
4. **README:** the close bullet now ends "unless the run never added a member".

I left findings 5 and 6 alone, since they belong to the lead and to stophold. `reports/skill.md` has the evidence for each finding in a new "Closer round" section.
