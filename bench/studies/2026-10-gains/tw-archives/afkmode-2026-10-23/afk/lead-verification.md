# Lead's verification of the final joined branch

Branch afk/afkmode at e85c711 (main fb1fd8f + start, endings, stophold, skill merges + lead commit 9e45487 (D26) + a second merge of afkmode/skill after the closer's F2-F4 fixes). Run by the lead in a git worktree of the repository, on 2026-10-24.

| Command | Where | Exit |
| --- | --- | --- |
| scripts/gate.sh (full) | afk/afkmode e85c711 | 1: steps 1 fmt, 2 tidy, 3 clippy ok; step 4 test fails only at herdr_env_wins_over_tmux; gate stops there (log: afk/gate-joined.log) |
| TEAMWIRE_FAKES_PREBUILT=1 cargo test --workspace --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux | e85c711 | 0 (171 test binaries ok, 0 failed; afk/test-joined-skip1.log) |
| RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked (gate step 5) | e85c711 | 0 |
| scripts/check-docs.sh (gate step 6) | e85c711 | 0 |
| bun test (ui/) | e85c711 | 0 |
| bun run typecheck (ui/) | e85c711 | 0 |
| cargo test --locked -p teamwire-cli --test tmux_wiring | main fb1fd8f (repo checkout) | herdr_env_wins_over_tmux FAILED, panic at crates/teamwire-cli/tests/tmux_wiring.rs:347, the same test as on the joined branch: a pre-existing failure on this machine (HERDR_ENV and TMUX both set) |

teamwire verify: all four member branches PASS at their final heads (flags read and waived: standard lint headers of new test files; the D25 Codex files for skill).
