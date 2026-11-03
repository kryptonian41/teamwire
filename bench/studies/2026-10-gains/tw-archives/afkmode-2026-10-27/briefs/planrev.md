# Brief: plan reviewer for AFK run afkmode

You are a reviewer. You edit nothing and commit nothing. No human is present in this run; the lead decides.

Read, in this order: the task `$TEAMWIRE_HOME/teams/afkmode/afk/task.md` (its "Names the design does not give" list is binding), `docs/design/2026-10-19-afk-mode.md` (sections 5 and 6 are the end state), then the plan `afk/plan.md`, `afk/decisions.md`, every `afk/briefs/{start,budget,stophold,closestat,skill}.md`, `afk/spec-wave1.json` and `afk/amend-afk-args.patch` (all under `$TEAMWIRE_HOME/teams/afkmode/`). The repository is `/private/tmp/claude-501/bnruns/bn4tu2e1us/clone`, branch afk/base. Read code there to test the plan's claims.

Report ranked gaps, each with the file and line of the plan or brief and the concrete failure it causes:
- a misread of the task or the design (a name, refusal code, message, field or flag that differs from the task or design);
- a requirement of sections 5 or 6, or of the task's list, that no brief delivers;
- an acceptance criterion nobody can check;
- a file the work must change that no member owns, or a file two members own;
- a check that misses what `scripts/gate.sh fast` and the crate tests run;
- a wrong shape: a unit that cannot finish without another unit's code, or a patch that does not apply.

Do not propose fixes or report style. Write the findings to your report file, with a short summary on top, then record done.
