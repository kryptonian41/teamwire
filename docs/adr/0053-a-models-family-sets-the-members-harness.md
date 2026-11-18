# ADR 0053: The family of a member's model sets its harness

Status: accepted, 2026-11-18. Supersedes these sentences of ADR 0033, and no
others:

- The status line (lines 5-6), "This ADR changes the lead instructions, so
  they no longer advise that move". The lead skill again tells a lead to put
  a member on a Claude model on Claude Code.
- Decision 1 (line 38), "The lead instructions name no harness and no
  model", for the words "no harness". The instructions still name no model.
- Decision 2 (lines 41-42), "Picking a harness and picking a model are
  separate decisions. A member runs on the lead's harness unless the user
  asks for another", as a rule the lead follows. As a fact about a spec that
  names no `harness`, the second sentence still holds (ADR 0023 point 1).
- "These follow from them", second bullet (lines 62-63), "That section says
  to keep the lead's harness".
- Consequences, first bullet (lines 99-100), "without being pointed at one
  harness or one vendor's models", for the skill. The presets still name no
  harness and no model.

The rest of ADR 0033 stands: the lead picks each model with evidence, each
member has a task class, and `teamwire models --for <class>` names no best
model.

## Context

ADR 0033 made the lead pick each member's model from what the member's
harness reaches. The lead skill turned decision 2 into a ban. Step 1 of
"Choose each member's model" said to set `harness` only when the user names
one, and to "never change a member's harness to reach a model".

The ban came from the `hsurvey` team. Its presets named Claude models, so a
pi lead started Claude Code members, and the user had wanted pi members.

Two facts changed on 2026-11-18.

- The user added OpenRouter credentials to pi. `teamwire models --harness pi`
  then listed 324 models, 310 of them through OpenRouter, from several
  vendors. `teamwire models` for Claude Code lists 15 models, all Claude
  models. So a model of another vendor runs only on pi, and a Claude model
  runs on both harnesses.
- The team `xvtrial` ran two reviewers on one diff under a Claude Code lead.
  One ran on Claude Code. The other ran on pi with
  `openrouter/openai/gpt-6-astra`. Launch, the result, the token counts and
  close all worked with no change to the program. The lead had to act
  against step 1 of the skill to write that spec.

The harness has belonged to each member since ADR 0004. The ban stopped a
lead from using a setup the program supports. With the ban gone, a Claude
model has two harnesses that reach it, so the lead needs a rule for which
one to use.

## Decision

The user decided on 2026-11-18:

1. **A member on a Claude model runs on Claude Code.** The lead sets
   `"harness": "claude-code"` for it, whatever harness the lead runs on.
2. **pi is the fallback for a Claude model.** When Claude Code is not
   working, the lead puts the member on pi with the same Claude model from
   pi's list.
3. **A member on a model of any other family runs on pi.** A GPT or a
   Gemini model is such a model. The lead sets `"harness": "pi"` for it.
4. **The lead may change a member's harness to reach a model.** The skill's
   ban is removed.

These follow from them:

- **The model comes first, then the harness.** The skill's steps now list
  what Claude Code and pi reach, set the task class, read the evidence, pick
  the model and effort, and set the harness last, from the model's family.
- **What "not working" means for the lead.** The skill names the signs
  teamwire or the harness shows: launch refuses with
  `harness claude-code is not on PATH`, a Claude Code member fails to start
  or stops at a login dialog, or the Claude account is at its usage limit.
- **A running member cannot change its harness.** When Claude Code stops
  working mid-run, the lead replaces the member under a new role name.
- **A model pi does not list.** The lead uses a harness whose list holds the
  model. Without this, a machine with Codex and no pi could launch no member
  on a GPT model, because launch refuses a spec whose harness is not on
  `PATH`.
- **The user's word on the harness still wins.** When the user names a
  harness for a member or for the team, the lead sets it. This keeps what
  ADR 0033 fixed for `hsurvey`.
- **The rule is the same for every lead.** It is in the shared skill, so a
  Claude Code, Codex and pi lead all read it. The pi and Codex notes
  (`examples/skills/pi-lead.md`, `codex-lead.md`) repeat it beside the
  sentence that says where a member with no `harness` runs.

No program behaviour changes. Launch already accepts a `harness` on each
member, and `teamwire models --harness <id>` already lists any harness.

## Rejected

- **teamwire picks the harness from the model id.** A model id does not
  name one harness. The same Claude model is `claude-opus-5-5` on Claude
  Code, and `claude-bridge/claude-opus-5-5` and
  `openrouter/anthropic/claude-opus-5.5` on pi. A listed model can also be
  out of reach for the account (ADR 0033, Context). The lead reads the lists
  and writes the spec, as it does for the model.
- **Keep the ban, and have the user name the harness for each member.** The
  harness follows from the model the lead already picked with evidence. The
  user would have to repeat that choice for every member.
- **Every member stays on the lead's harness when that harness reaches the
  model.** A pi lead reaches Claude models through pi's own providers, so
  this rule would keep its Claude members on pi. The user wants a Claude
  model on Claude Code first.

## Out of scope

- **The AFK model table** (ADR 0025 point 8,
  `crates/teamwire-launch/src/afk.rs`). The table judges only the names in
  its own order, so a member on a model outside it gets no warning.
- **Launch moving a member to pi by itself** when Claude Code fails. The
  lead sees the failure and writes the new spec.
- **One command that lists every harness.** `teamwire models` still lists
  one harness per call.

## Consequences

- A team with members on two harnesses is an ordinary team. A pi lead's
  members on Claude models run on Claude Code, and a Claude Code lead's
  members on other models run on pi.
- Each member follows its own harness's rules, which the skill already
  states: the `plugins` map holds that harness's plugin ids and a pi member
  ignores it, and teamwire sets no compaction for a Codex or pi member
  (ADR 0027).
- A Codex lead sets `harness` on every member, unless pi does not list the
  member's model and Codex does, or the user names Codex.
- A member's cost goes to the account behind its harness and provider. A pi
  member on an OpenRouter model is billed by OpenRouter.
- The test `the_models_family_sets_a_members_harness` in
  `crates/teamwire-cli/tests/preset.rs` fails when the skill forbids a
  harness change again, sets the harness before the model, or drops a part
  of the rule, and when a harness note drops it.
