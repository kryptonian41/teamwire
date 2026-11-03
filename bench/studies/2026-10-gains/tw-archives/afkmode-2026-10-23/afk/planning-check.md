# Planning check: spec-wave1.json

Run by planner2 on 2026-10-23. Logs are in
/private/tmp/claude-501/-private-tmp-claude-501-bnruns-bnei8xdqkf-clone/da8b3353-9081-4949-a4e4-fa33e5b509ab/scratchpad/.

## 1. Each check is runnable and was run once on the base

Yes. planner2 ran all four checks at fb1fd8f in a fresh clone (item 3). The
first planner's earlier run left no saved evidence, so planner2 ran them again.

## 2. Each check against what CI covers

There is no .github. The gate is scripts/gate.sh: 1 fmt, 2 tidy
(`cargo xtask tidy --base main`), 3 clippy over the workspace, 4 the whole
workspace test, 5 `cargo doc` with `-D warnings`, 6 scripts/check-docs.sh.
`fast` runs steps 1 to 3.

| Gate step | Member check covers it? | Who covers the gap |
| --- | --- | --- |
| 1 to 3 (fmt, tidy, clippy) | The brief tells each member to run `scripts/gate.sh fast` (13 s warm on the base). | closer, full gate |
| 4 (workspace tests) | Partly: each member tests its own crates. `scripts/test-changed.sh` in each brief adds the crates that depend on them. | closer, full gate |
| 5 (`cargo doc -D warnings`) | No member runs it. New `pub` items with broken doc links fail here. | closer, full gate |
| 6 (check-docs.sh) | `skill` runs it. | closer |
| ui `bun test`, `bun run typecheck` | `endings` runs them. scripts/gate.sh does not run them at all. | closer must run them in ui/ on the joined head |
| `herdr_env_wins_over_tmux` | Skipped in every check (D12): it fails in a tmux pane at the base. | closer reports its result separately |

## 3. Each check passes in a fresh clone after setup

`git clone /private/tmp/claude-501/bnruns/bnei8xdqkf/clone <scratchpad>/clone`,
`git checkout main` (fb1fd8f), then setup `cd "$1/ui" && bun install --frozen-lockfile`,
then each check in order, then the clone was deleted.

| Step | Exit | Time |
| --- | --- | --- |
| setup | 0 | under 1 s (14 packages) |
| start check | 0 | 90 s (cold build included) |
| stophold check | 0 | 59 s |
| endings check (with `bun test`: 159 pass, 0 fail; typecheck) | 0 | 84 s |
| skill check (with `docs check: ok`) | 0 | 60 s |
| `scripts/gate.sh fast` | 0 | 13 s (fmt 1 s, tidy 3 s, clippy 9 s) |

A cold build costs about 90 s per worktree, so each worktree keeps its own
target/ (D11). `scripts/test-changed.sh` was not timed; it runs several
crates' tests and should take a few minutes per member.

## 4. No two members of one wave have overlapping owns

`jq -r '.children[] | .owns[]' spec-wave1.json | sort | uniq -d` prints
nothing. `teamwire launch --add --dry-run --spec spec-wave1.json` exits 0 and
prints only the four expected AFK model warnings (sonnet below opus). Without
`--add` it refuses `team_exists`, because team afkmode already exists, so the
lead must launch with `--add`. Every owned path exists at the base except the
new files the units create.

## 5. Every file the work must change is owned

- The one caller of `create_afk_team`, crates/teamwire-store/src/team/tests.rs:48: `start`.
- `StatusView` constructors: crates/teamwire-watch/src/status.rs:299 and crates/teamwire-cli/src/cmd/status.rs:300: `endings`. No other literal.
- `TeamEntry` literal: crates/teamwire-msg/src/directory.rs:44 only: `endings`. MCP serialises it (crates/teamwire-mcp/src/reads.rs); its tests index keys and pin no full object.
- ui fixtures ui/fixtures/status.json and teams.json: added to `endings` (were missing).
- Plugin file-list test crates/teamwire-harness-claude/src/lead_setup/plugin.rs:278: `skill`.
- crates/teamwire-cli/tests/preset.rs:331 checks only the main skill's commands; it needs no change (skill brief says why).
- No CLI help snapshot or command-count test exists (`grep insta` and `subcommands` in crates/teamwire-cli/tests found only preset.rs's live `--help` read). Tidy has no list of commands or requirement ids.
- Layers: every new call goes downward (cli to launch, store, report, watch, close; report to store and deliver). No crate needs a new dependency; close, watch and report already depend on serde_json (needed for D21).
- Protected files: only docs/adr/0025-afk-runs.md (new), given to `skill` (D15).

## 6. Content rules with a worked example

The note texts (D6), status line and teams mark (D7), warning text (D8),
refusal texts (D5, D18, D20) and heading rule (D5) are written out word for
word in the briefs. D7 and D8 match the installed teamwire binary's output
on this machine.

## 7. Conventions the harness also sets

The briefs fix no commit trailer and no commit message format.

## 8. Commands live once in the repository's instruction files

AGENTS.md already lists the build and test commands. The briefs repeat only
each member's own check and the gate, because this brief asks for exact
verification commands. No new repository rule was needed.
