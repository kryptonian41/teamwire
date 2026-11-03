# ADR 0033: The lead picks each member's model from what its harness reaches

Status: accepted, 2026-11-03. Supersedes no decision of an earlier ADR. The
fact in ADR 0023 point 1 still holds: a member that keeps a Claude model
name needs `"harness": "claude-code"`. This ADR changes the lead
instructions, so they no longer advise that move. ADR 0025 point 8, the
model table for AFK runs, is unchanged.

## Context

A member runs on a harness (Claude Code, Codex or pi), on a model, at an
effort (thinking) level. Until this ADR, the lead instructions pointed every
lead at Claude Code and at Claude model names:

- the presets in `examples/presets/*.json` set `"model": "sonnet"` or
  `"opus"`, an effort, and a `plugins` map of Claude Code plugins on members;
- the role table in `examples/skills/teamwire/SKILL.md` gave one model and
  one effort per member kind, and said that no measured source backed them;
- `examples/skills/pi-lead.md` and `codex-lead.md` said that a member that
  keeps a Claude model needs `"harness": "claude-code"`.

In the `hsurvey` team, a pi lead followed those instructions. The presets
named Claude models, so the lead started five Claude Code members, and the
user had wanted pi members.

A second fact came up on 2026-11-03. `pi --list-models` lists a provider
whenever pi finds any credential for it. On the user's machine,
`AWS_PROFILE` is set, so pi lists about 200 `amazon-bedrock` models. The
user's AWS policy denies Bedrock, so every Bedrock call fails with
`AccessDeniedException`. pi resolves a model pattern without a provider,
such as `opus`, to whichever provider it prefers. It picked Bedrock, and
the first launch of the team that built this ADR failed that way.

## Decision

The user made five decisions on 2026-11-03:

1. **The lead instructions name no harness and no model.** The presets and
   the lead skill describe what a task needs. The lead picks the model.
2. **The lead picks from the models the member's harness reaches on this
   machine.** Picking a harness and picking a model are separate decisions.
   A member runs on the lead's harness unless the user asks for another.
3. **The lead picks with evidence**: public benchmarks and user reports, and
   the outcomes of the user's own past teams, per task class.
4. **Each member gets a task class** (`MemberSpec.task_class`, one of the
   seven `TaskClass` values in `crates/teamwire-core/src/models.rs`). Launch
   records the class in the member's `member.creating` event. Outcomes can
   then be read per class, and a task put in the wrong class shows up in
   that data. A member's `kind` still sets its role, and its class says how
   hard its task is.
5. **`teamwire models --for <class>` shows the ranked evidence and its
   sources.** It never names one best model. The choice stays with the
   lead.

These follow from them:

- Every preset member sets `task_class` and no `model`, `effort` or
  `plugins`. Each rule that named a model now says what the task needs and
  points at `teamwire models --for <class>`. `teamwire preset` prints
  `task_class` right after `mode`.
- The skill's role table has no model or effort column. A section on
  choosing a member's model replaces it. That section says to keep the
  lead's harness, run `teamwire models`, set `task_class`, run
  `teamwire models --for <class>`, then pick `model` and `effort` and give
  the reason in one line of the plan.
- The skill's plugin advice names Claude Code plugins, so the skill says it
  applies to Claude Code members. The `plugins` map holds plugin ids of the
  member's own harness.
- **A pi member's `model` names its provider**, as `provider/id` from
  `teamwire models`. A pattern without a provider can resolve to a provider
  the account cannot call.
- **The user can leave a pi provider out.** `TEAMWIRE_PI_EXCLUDE_PROVIDERS`
  is a comma-separated list of pi provider names. teamwire reads it from the
  environment first, then from a `KEY = "value"` line in
  `$TEAMWIRE_HOME/config.toml`, as it reads the OTLP settings
  (`crates/teamwire-otel/src/config.rs`). An example line:
  `TEAMWIRE_PI_EXCLUDE_PROVIDERS = "amazon-bedrock"`.

## Rejected

- **A fixed model table per kind.** That was the old role table. A table
  names one vendor's models, so it steers a lead on another harness toward
  that vendor. Models also change faster than the table is updated, and no
  measured source backed the old values.
- **teamwire naming one best model per class.** Evidence for a class comes
  from several sources, each with its own date and method, and the user's
  own outcomes may disagree with them. One answer would hide that. The lead
  sees the ranked evidence with its sources and decides.

## Out of scope

- **The AFK model table** in `crates/teamwire-launch/src/afk.rs` and the AFK
  skill. An AFK run needs a Claude Code lead (ADR 0025,
  `HarnessCaps::runs_afk`), so its table of Claude model names steers no
  other harness.

## Consequences

- A lead on any harness reads the presets and the skill without being
  pointed at one harness or one vendor's models.
- A lead must run `teamwire models` before it can fill in `model`. A spec
  with no `model` still launches, on the harness's default model.
- A preset's skeleton no longer switches plugins off for read-only members.
  A Claude Code lead adds that map from the skill's plugin section.
- The test `every_member_names_a_task_class_and_no_model` in
  `crates/teamwire-cli/tests/preset.rs` fails when a preset names a model,
  an effort or plugins, or when a member's class does not fit its kind.
