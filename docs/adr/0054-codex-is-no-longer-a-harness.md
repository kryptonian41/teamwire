# ADR 0054: Codex is no longer a harness

Status: accepted, 2026-11-19.

Supersedes ADR 0023, except the points below. Each of them still holds, as
far as it describes what Claude Code and pi do:

- Point 1: a member with no `harness` runs on the lead's harness
  (`crates/teamwire-launch/src/plan.rs`, `harness_id`).
- Point 4: a member's token is read from `TEAMWIRE_TOKEN`, else from the file
  that `TEAMWIRE_TOKEN_FILE` names, which a pi member uses
  (`crates/teamwire-msg/src/token.rs`, `caller_token`).
- Point 6: a member hook parses its input with the harness that the member
  row names (`crates/teamwire-cli/src/cmd/hook.rs`, `member_hook_in`).
- Point 10: the contract text names no harness
  (`crates/teamwire-launch/contracts/`).
- Point 9, two of its bullets only. The wiring picks the lead harness from
  `TEAMWIRE_LEAD_HARNESS`, else from the session the command runs in
  (`crates/teamwire-cli/src/wiring.rs`, `lead_harness`). `teamwire install`
  shows a numbered selector of the harnesses on PATH in a terminal, and
  installs `--harness` or `claude-code` without one
  (`crates/teamwire-cli/src/cmd/install.rs`).

Also supersedes each sentence in the table at the end of this file, as far
as the sentence gives a rule for a Codex lead or a Codex member. A sentence
that lists "Claude Code, Codex and pi" now reads "Claude Code and pi".

## Context

ADR 0023 added the Codex CLI as a harness on 2026-10-18. The adapter was
written from Codex's source code, its docs and its `--help` text. ADR 0023
says that no fact in it was seen in a live session. Codex is still not
installed on the user's machine, and the user has no ChatGPT subscription,
so nobody can check those facts.

These numbers are from commit `e427e22d` (v0.28.1):

| Fact | Number |
| --- | --- |
| Source lines in `crates/teamwire-harness-codex/src` | 2,742 |
| Test lines in `crates/teamwire-harness-codex/tests` | 1,734 |
| Source files in other crates that mention Codex | 27 |
| Commits that touched the Codex crate | 42 |

Every lead feature since ADR 0023 was built for three harnesses. The Codex
part of each feature was tested only against fixture files that were written
from the same reading of Codex's source. A passing Codex test proves that the
code matches that reading. The test proves nothing about a real Codex session.

One fact changed on 2026-11-18. The user added OpenRouter credentials to pi,
and `teamwire models --harness pi` then listed 324 models from several
vendors (ADR 0053, Context). The team `xvtrial` ran a pi member on
`openrouter/openai/gpt-6-astra` from launch to close. So pi reaches the
models that Codex was kept for.

## Decision

The user decided on 2026-11-19 to remove support for the Codex harness.

1. **The Codex adapter is deleted.** The crate `teamwire-harness-codex` goes,
   with its tests and fixtures. The wiring registers two harnesses:
   `claude-code` and `pi`. Every code path that is chosen by the id `codex`
   goes with it, such as the `codex` arm of `hook_name` in
   `crates/teamwire-report/src/turn_end.rs`.
2. **The id `codex` is an unknown harness.** A spec that names
   `"harness": "codex"`, and `teamwire install --harness codex`, get the same
   refusal as any other unknown id. teamwire adds no special message for it.
3. **teamwire reads no Codex setting.** The wiring stops reading
   `CODEX_HOME`, `CODEX_THREAD_ID` and `TEAMWIRE_CODEX_HOME`. A `teamwire`
   command that runs inside a Codex session is treated like a command in a
   plain shell.
4. **A lead or member feature ships for Claude Code and pi.** The rule in
   `.specify/memory/constitution.md` names those two.
5. **A model of another family runs on pi.** Decisions 1 to 4 of ADR 0053
   stand. The bullet "A model pi does not list" loses its Codex case. The
   lead cannot use a model that neither harness lists.
6. **The spec-kit checks drop Codex.** `teamwire doctor` no longer warns for
   `.codex/config.toml` or `.codex/hooks.json`, and the `speckit` preset no
   longer names the Codex integration.
7. **Shared code that only Codex used is removed in a second step.** Each
   piece is removed in its own commit, after the adapter is gone, and only
   when a search shows that no harness and no host route still needs it.
   The candidates are:
   - `HarnessCaps::reads_watch_lines` and the launch check that refuses a
     lead who cannot be located (ADR 0023 point 9). Claude Code and pi both
     set it to `true`.
   - `HarnessCaps::choose_session_id` and the code path for a harness that
     picks its own session id. Claude Code and pi both set it to `true`.
   - `WatchStart::BackgroundCommand`, the watch start that only a Codex lead
     used.
   - The `MCP_META_<key>` lookup in `Harness::caller_session`.
   - `MemberLaunch::with_run_context`, kept for a harness that carries its
     hook and MCP commands in argv.
   - `MemberLaunch::accept_trust`, the copy of the spec's `accept_trust` that
     launch writes into each member's start. Launch still writes it, and no
     adapter reads it. The spec's own `accept_trust`, which decides whether
     launch answers the folder-trust prompt, is not a candidate.
   - The `false` value of `HarnessCaps::reads_input_box` and the two branches
     for it in `crates/teamwire-deliver/src/deliver.rs`, which skip the
     input-box check on a host that guards dialogs and read the screen
     without style codes on another host. Claude Code and pi both set it to
     `true`.
   - `CarryRules::keyed` in `crates/teamwire-core/src/lead_start.rs`, which
     carries an old lead's flag only with a value that starts as given.
     Claude Code and pi both pass an empty list.
   - The hook `before_read` of the lead-setup contract suite in
     `tests/fakes/src/contract/lead_setup.rs`, which queues what a harness's
     program answers when asked what is installed. Every caller passes a hook
     that does nothing.
8. **These parts stay.**
   - The list of agent kinds in `crates/teamwire-host-herdr/src/spawn.rs`.
     The list holds herdr's own kind names, and `codex` is one of them.
   - `teamwire watch --deliver` and `teamwire pr-watch --deliver`, unless
     the second step proves that no lead route uses them.
   - Entries in `examples/models/evidence.json` that were measured in the
     Codex CLI. Each entry is evidence about a model, and its text already
     says where it was measured.
   - A stored row with harness `codex` in a migration test. The row is old
     data, and a migration must still read old data.
9. **History is not edited.** `bench/studies/`, `docs/research/`,
   `docs/design/`, `specs/` and every accepted ADR file stay as they are.
10. **The store does not change.** No schema version is added.

## Rejected

- **Keep the crate and stop adding features to it.** The docs and the lead
  skill would still say that Codex works. Each new feature would make that
  statement less true, and no test would show it.
- **Keep Codex for members only.** The member path holds most of the facts
  that nobody checked: the `-c` values, the hooks, the thread lock and the
  rollout file.
- **Keep the code behind a cargo feature that is off by default.** Code that
  is not compiled in the gate stops building within a few changes to the
  `Harness` trait.

## Consequences

- A user who leads from Codex, or runs Codex members, cannot upgrade past
  v0.28.1. The repository is private and the user's machine has no Codex, so
  no known user is affected. The release notes say it.
- teamwire adds no cleanup for an old Codex install. A user removes the
  Codex plugin with `teamwire uninstall --harness codex` on v0.28.1 before
  the upgrade.
- A team with a Codex member must be closed before the upgrade. No such team
  exists on the user's machine: `teamwire teams` printed `no teams` on
  2026-11-19.
- pi through OpenRouter is the only route to a model of another family. A
  Claude model keeps two routes (ADR 0053).
- `tidy --base` prints `WEAKENED` for each deleted Codex test and
  `PROTECTED-CHANGED` for the root `Cargo.toml`, `Cargo.lock`, `xtask/`,
  `scripts/test-changed.sh` and `tests/fakes/`. This ADR is the reason for
  each of those lines. A deleted test that is not a Codex test is still a
  weakened test.
- The `Harness` trait keeps two real adapters. Claude Code works through
  hooks and a plugin. pi works through extensions. ADR 0004's promise about
  a new harness is still tested by two different designs.
- The Codex adapter can be read at the tag `v0.28.1`. Bringing it back means
  porting it to the trait as it is then.

## Superseded sentences

| ADR | Lines | What the sentence said |
| --- | --- | --- |
| 0004 | 8, 38, 107 | Codex CLI is a planned adapter |
| 0007 | 19-20 | A fallback for Codex with no turn-end hook |
| 0010, 0026 | 0010 line 21; 0026 lines 16, 57, 59-60, 63, 77, 79, 118-119 | Codex CLI ships, and the corrections that follow from it |
| 0025 | 132-133 | A Codex lead keeps the behaviour without AFK |
| 0027 | 36-39, 85-86, 135-136, 145-146, 164-167 | How teamwire reads a Codex member's context fill and session id |
| 0029 | 117 | A pi member's token travels as for Codex. The rest of the sentence still holds for pi |
| 0030 | 52-55 | The Codex session variable is checked before pi's marker |
| 0031 | 75-76, 95-97, 107-109 | Doctor's Codex lines and the order `codex`, `pi` |
| 0033 | 11, 19-20 | A member runs on Claude Code, Codex or pi |
| 0035 | 46-47, 88-89, 94-96 | The Codex plugin's `PostToolUse` hook for the lead |
| 0037 | 6-10, 12 | A correction to ADR 0023 decision 9, and "Every other sentence of ADR 0023 stands" |
| 0039 | 17, 21, 54-56, 92-94, 96-98 | A Codex lead starts `pr-watch --deliver` in the background |
| 0041 | 30-31, 125-126, 174, 182-184, 200-201 | Codex members in a spec-kit team, and the `.codex/` files launch refuses |
| 0042 | 78 | A Codex member is out of scope for the step after the review |
| 0043 | 39-40, 58-59 | The lead's tool hook runs on Claude Code, Codex and pi |
| 0050 | 16-17, 61, 68, 109-110 | Two table rows about Codex, and the three-harness rule |
| 0051 | 21-22, 98-100, 113, 170, 181-182, 189-190 | A new Codex lead adopts each team itself |
| 0052 | 70-71, 92 | Codex names the dialog kind from the screen |
| 0053 | 78-81, 85-88, 125-127, 128-129 | A machine with Codex and no pi, the Codex lead note, and what a Codex lead sets |
