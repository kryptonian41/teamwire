# ADR 0061: A lead may run a set of readers on several model families, as one review round

Status: accepted, 2026-11-20. Supersedes no record. Adds to ADR 0034, which
added the plan reviewer, and to ADR 0053, which has a model's family set a
member's harness. ADR 0056, ADR 0057 and ADR 0058 built the parts this
record uses: the family of a model, the `reconciler` kind with the `set`
label, and the `quiet` setting.

## Context

A teamwire review has had one reader: one reviewer per finished diff, and
one plan reviewer per plan. The skill and the presets said so in several
places, such as "ONE independent reviewer", "Never review the same plan
twice" and "never a second review of the same diff". Those rules exist
because a reader asked again finds something new each time, and the fixes
then grow without end.

One reader also misses defects that a model of another family would find.
The scouting of 2026-11-18 and 2026-11-19 is in
`docs/research/2026-11-19-model-family-sets.md`. Its findings, in short:

- One controlled study found that a fresh Claude review plus a GPT review
  matched 56.7% of 150 planted errors, against 42.7% for two fresh Claude
  reviews. The study could not tell the family apart from the reviewers'
  strength.
- One reviewer on another family was not significantly better than one
  fresh reviewer on the author's family.
- No source measured a third reader, by any family mix.
- The one study on point scored joined parallel plans worst of 12 setups.
  No source built and tested a joined plan.
- In the user's own runs, 12 extra Claude readers on ten teams added 37
  findings that no other reader had.

The user decided on 2026-11-18 and 2026-11-19 to let a lead run a **set**:
two or three members that do the same judging job on the same brief, each
on a model of a different family, that never talk to each other. A
`reconciler` then joins their reports into the one report the next step
acts on. Step 1 covers reviewer sets and plan-reviewer sets. A planner set
is a later step: a planner there is an `investigator` with `"quiet": true`,
as ADR 0058 says, and it gets its own plan once step 1 has shipped.

## Decision

1. **A set is one review round, not a second review.** The rules against a
   second review keep their meaning: they forbid a review after a review of
   the same diff or plan. A set is several reviews side by side, read once,
   then joined once. So each such rule now says "one review round, which
   may be one reader or one set, and never a second round".
2. **Two preset steps carry a set.** `teamwire preset plan-review-set` holds
   three `plan_reviewer` readers, `plana`, `planb` and `planc`, and the
   reconciler `planjoin`. `teamwire preset review-set` holds three
   `reviewer` readers, `reva`, `revb` and `revc`, and the reconciler
   `revjoin`. The two steps share no role, because one team may run both
   and launch never uses a role twice in one team. The readers of a step
   share one `set` label and one task text, word for word. The reviewer
   set's task lists every implementer's branch, base and brief, and every
   reader reads every diff in the list.
3. **The lead follows twelve rules for every set.** The teamwire skill's
   section "Sets: several model families on one judging job" and the `_rules`
   of both presets state them:
   1. A set is opt-in. With the user present, the lead asks which set to
      use when it settles the plan's questions. The lead recommends a set
      when the team has three or more implementers, or when any member's
      class is `cross_cutting` or `concurrency_data_loss`. In an
      away-from-keyboard run, the lead uses a set only when the user's task
      text asks for one.
   2. The lead picks one model per family: the newest top-tier model of
      each family that `teamwire models --harness claude-code` and
      `teamwire models --harness pi` list. A set uses at least two families,
      and three when a third is reachable. A set has at most three readers.
      With more than three families reachable, the lead keeps the three
      whose top models have the strongest evidence in
      `teamwire models --for judgement`, and says why in the plan.
   3. The plan says when a pick has no measured result for its seat. The
      user confirms or swaps each pick.
   4. Each reader's `harness` follows its model's family, as ADR 0053 says,
      and every reader has an `effort`. A model called with no effort
      setting can run below its best level.
   5. Before the launch, the lead shows the user the estimated money for
      the readers that run through a provider that bills each token, and
      says where each tier, release date and price came from. teamwire has
      no command that gives them.
   6. Every reader gets the same brief, and no reader a special focus. A
      reviewer with a special focus is a separate member outside the set.
   7. The lead launches the reconciler only after every reader has
      reported. Launch warns when it does not (`set_reconciler_early`).
   8. The reconciler reads the reports under role names. The lead does not
      tell it which model wrote which report.
   9. The lead reads the reconciler's summary and its "Questions for the
      parent". For a reviewer set, the lead sends each owner one reprompt
      that names the reconciler's report and the owner's section. For a
      plan-reviewer set, the lead is the owner: it compares each paraphrase
      with what it meant and fixes the briefs once. The lead does not read
      the readers' reports.
   10. A set reads once. Nobody runs a set, or a single reviewer, a second
       time on the same diff or the same plan.
   11. When a reader cannot reach its model, the set carries on if two
       families remain. Otherwise the lead replaces that reader once, under
       a new role name, with another model of the same family. When the
       replacement fails too and one family is left, no set runs. The lead
       tells the user, treats the one report as a single review, and adds
       no reconciler.
   12. A reader on Claude Code gets the plugins map that turns off memory
       plugins. A pi reader ignores that map, so the lead checks that the
       user's pi setup has no memory feature that carries one reader's
       findings to another.
4. **The repository names no model for a set.** No preset, skill text or
   code names a model for a reader, and no file under `examples/` names a
   real model for a set. The worked example in the skill writes each model as a
   placeholder, such as `<newest Claude model>`. This follows ADR 0033:
   models change faster than any table, and the lead picks from what
   `teamwire models` lists on the day. A named set would also go out of date
   the week a vendor ships a model. The `family` column of
   `teamwire models` labels each model, and launch warns about a set from
   it, but neither picks anything (ADR 0056).

## Consequences

- A lead on Claude Code or on pi can run a reviewer set or a plan-reviewer
  set from the skill and a preset alone. A pi lead follows the same rules.
  Its notes on the watch and on a member's harness already cover a set.
- `teamwire preset` lists six shapes and five steps.
- A set costs more than one reader: about $8 to $20 for a reviewer set in
  the scouting's estimates, against about $4.50 for one reviewer. The user
  sees the estimate before each set runs.
- The reconciler's table records which reader raised each finding and its
  verdict. teamwire stores no such record itself, so a later study reads
  the tables to see what each family added.
- The evidence for a set is thin. The gain of a mixed pair comes from one
  study that could not separate family from strength. A third reader and
  joined plans have no measurement at all. A set stays opt-in for that
  reason, and the planner set waits for its own plan.

## Alternatives rejected

- **A set by default.** The evidence does not show that a set pays for its
  cost on every team. The user decided that a set is opt-in.
- **Act on a finding only when two readers agree.** No source tested that
  rule on code review, and models that are wrong agree on the same wrong
  answer about 60% of the time. The reconciler checks each finding in the
  code instead, and one confirmed finding counts in full.
- **A different focus per reader.** A reader with its own focus no longer
  reads the same thing as the others, so the set cannot show what a family
  adds. A focused reviewer stays possible as a separate member.
- **Name a model per family in the presets.** ADR 0033 forbids it, and a
  test fails a preset that sets `model`, `effort` or `plugins`.
