<!-- teamwire:generated completion=01M5MYDD6THZGPVDEWXAXYNA1B -->
# Result from rev_skill (done)

- Team: afkmode
- Status: done
- Generation: 1
- Completion: 1
- Time: 2026-10-23T17:54:49.690Z
- Final message from: hook
- Member report: /private/tmp/claude-501/bnruns/bnei8xdqkf/twhome/teams/afkmode/reports/rev_skill.md

## Summary

afkmode/skill is sound: skill text matches design 6, plugin render and tests correct; 4 low text findings (Codex gets the pointer line, 2 mis-cited test files, 2 rules without AFK rows). Report: reports/rev_skill.md

## Task (first 300 characters)

# Brief: reviewer of branch afkmode/skill (AFK run "afkmode") You are a reviewer: you read, you never edit, commit or message anyone. The lead runs this team while the user is away. - The user's task, word for word: /private/tmp/claude-501/bnruns/bnei8xdqkf/twhome/teams/afkmode/afk/task.md. Its "Na…

## Final message

The `afkmode/skill` branch is sound. I found no correctness, security or data-loss problem in its Rust change. The full review is in `/private/tmp/claude-501/bnruns/bnei8xdqkf/twhome/teams/afkmode/reports/rev_skill.md`.

- **Skill text:** the new AFK skill matches design section 6 word for word. The only difference is one re-wrapped line, and no rule is missing.
- **Install:** `teamwire install` now writes the skill into the plugin as `teamwire/skills/afk/SKILL.md`. The file counts toward the plugin's version, and doctor's plugin check covers it with no code change. Each of the three new tests fails if its fix is removed.
- **Names:** the command names, refusal codes and messages in the docs match the code on the other three branches.
- **Tests run:** I ran the tests for `teamwire-harness-claude`, `teamwire-harness-codex` and `teamwire-cli` on the branch head. All passed, with `herdr_env_wins_over_tmux` skipped as the brief says. The builds went to my scratchpad, so nothing was written into the implementer's worktree.

I found four problems, all low and all in text:

1. **The pointer line also reaches Codex leads.** Codex builds its own copy of the main skill from the same text, so a Codex lead is now told to use `/teamwire:afk`. The Codex plugin has no such skill. The design asks for the pointer line and also says a Codex lead keeps today's behaviour. Those two statements conflict, so the parent has to choose.
2. **AFK-8 in `docs/requirements.md` names the wrong test file.** It cites `crates/teamwire-cli/tests/afk.rs`, which has no `status` or `teams` test. The real tests are on the endings branch: `tests/status.rs`, `tests/teams.rs` and the MCP teams test.
3. **One row in `docs/failure-modes.md` claims tests that do not exist.** The row says reopen, verify, metrics and a quiet watch work on a team with no members. None of the three test files it cites checks any of those four.
4. **Two rules have no `AFK-` requirement row.** One is that `launch --add` accepts a team with no members. The other is that the Stop hook skips an AFK team whose team row it cannot read. Both rules are tested on other branches.

I did not run `scripts/gate.sh`, as the brief says.
