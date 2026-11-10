# ADR 0042: Spec-kit after the review

Status: accepted, 2026-11-10. Supersedes the parts of ADR 0041 named
below; the rest of ADR 0041 still holds. ADR 0041 keeps its words as
history.

## Context

A review of teamwire's spec-kit support, as shipped in v0.23.0, found 30
defects, after the pilot feature had run. Some were in the guidance: the
question wording, the amendment merge, the checklist, AFK, and claims that
the pilot had tested parts it never ran. Some were in teamwire: doctor read
the disk where a worktree reads a commit, the watch ended at every
question round, and pi members had no scratch folder. The user agreed every
fix on 2026-11-10. This ADR records what changed from ADR 0041.

## Decisions

1. **Build order (supersedes ADR 0041 decision 7).** Step A shipped: the
   lead guidance and the `speckit` preset. The pilot ran. Step B shipped
   as doctor's spec-kit lines (decision 4) and a verify rule: spec-kit's
   constitution, `.specify/memory/constitution.md`, is check config, so a
   change to it outside `owns` is flagged. Step C, a `tasks.md` reader
   (`teamwire speckit draft`), is deferred. Until it exists, the lead
   writes the phase list, the task ids per commit, the check's grep and the
   disjoint-files test for a split by hand, from the file paths in the
   task lines.
2. **One wording for a question round (supersedes the wording in ADR 0041
   decision 2).** A spec member stops at a spec-kit question with
   `teamwire done --status blocked --summary "N questions for the parent. ..."`,
   or `1 question for the parent.` for one, and writes its questions under
   the heading `Questions for the parent`. This is the wording reviewers,
   closers and handoffs already use. The rule: the result's status is
   `blocked`, and its summary, after an optional leading `handoff: `,
   matches `^[1-9][0-9]* questions? for the parent\.`. A stop with no
   questions records no stop: the member goes on to the next command. The
   spec member's seat rules say to decide and finish when no answer comes,
   so its brief carries an override line, given word for word in the
   reference. The seat rules stay unchanged.
3. **The watch waits through a question round.** The watch counts a member
   in a question round as waiting for the lead, not as finished, so
   `TEAM-COMPLETE` cannot print while it waits, and the same watch follows
   the member after the lead's reprompt.
4. **Doctor reads the commit members start from (supersedes the "seven
   lines" consequence of ADR 0041).** `teamwire doctor --base REF`
   (default `HEAD`) prints nine spec-kit lines, each `ok` or `warn`:
   `spec-kit version`, `spec-kit committed`, `spec-kit constitution`,
   `spec-kit integrations`, `spec-kit git extension`, `spec-kit hooks`,
   `spec-kit events`, `spec-kit feature pointer` and `spec-kit workflows`.
   Each line names the ref and its commit, and reads files from that
   commit, except `committed`, which compares the working tree with it,
   and `feature pointer`, which asks git what it ignores. `spec-kit hooks`
   warns about any other extension's enabled hook with `optional: false`,
   because every spec-kit command tells the member to run such a hook.
   `teamwire afk start` runs doctor with `HEAD`.
5. **Amendment merges use `--no-ff` (supersedes the merge command in ADR
   0041 decision 8).** Both `SPEC AMENDMENT:` and `CONTRACT AMENDMENT:`
   notices say `git merge --no-ff --no-edit <sha>`. Without `--no-ff`, a
   merge before the member's first commit fast-forwards. The sender's
   commit then sits on the member's own line, so verify flags its files
   and `done` asks for a progress line for it. With `--no-ff` the merge is
   a merge commit, and verify skips it.
6. **Every member has a scratch folder.** Launch makes
   `$TEAMWIRE_HOME/scratch/<team>/<role>/` for every member, on every
   harness, and passes its path as `TEAMWIRE_SCRATCH`. The folder is
   outside the team folder, so the archive never copies it. Closing the
   team deletes it. The seat rules say to put temporary files there.
7. **AFK runs spec-kit only from a reviewed spec.** An AFK lead may run the
   plan reviewer, the build, the closer and the fix rounds from a
   `spec.md`, `plan.md` and `tasks.md` that the user already reviewed and
   committed. The spec stage needs the user at both question stops, at the
   review of `spec.md` and `plan.md`, and for the `checklist` command's
   ticks, and an AFK lead would answer all of them itself.
8. **What no run has tested (supersedes ADR 0041's "The pilot is their
   first test").** The pilot ran one Claude Code spec member, a Claude Code
   plan reviewer, one pi implementer and a Claude Code closer. No run has
   tested a split per user story (the merge of tick lines, the fix round
   on the integrator's branch, the per-member waivers) or a Codex member.
   The reference says so, and the lead tells the user before picking
   either.

## Deferred, recorded here so nobody looks for them

- The `tasks.md` reader, `teamwire speckit draft` (decision 1).
- Team shapes for spec-kit's `bug` extension (an investigator, then an
  implementer, then the closer) and its `assess` extension (a survey).
  Neither has a preset.

## Consequences

- `examples/skills/teamwire/reference.md` ("Spec-kit features"), the
  `speckit` preset, `examples/skills/teamwire/SKILL.md` and
  `examples/skills/afk/SKILL.md` state these decisions. Every lead harness
  reads them.
- The preset tests run the `speckit` preset's `setup` and `check` in
  scratch git repositories, so a command that stops working fails a test.
- Every preset's `setup` is `true CHANGEME worktree setup command`, which
  runs as printed, because a named script that a new repository lacks
  failed only at the real launch.
