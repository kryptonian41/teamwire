# ADR 0048: `teamwire speckit draft` drops the bare file-name rule

Status: accepted, 2026-11-14. Supersedes ADR 0044 decision 4 in part: the
rule that a file's full file name, as a whole token, is a mention. The rest
of ADR 0044 decision 4, and its other decisions, still hold. ADR 0044 keeps
its words as history.

## Context

ADR 0044 decision 4 counted a tracked file as a mention of a task path when
it held the path's full file name as a whole token, such as `tools.py` for
`apps/ai/harness/tools.py`. In a real repository a file name is often
shared. This repository tracks 20 files named `lib.rs`, 12 named `SKILL.md`
and 6 named `README.md`. So a task path that ends in `lib.rs` added every
file that names any `lib.rs`, and the lead sorted the noise out by hand.

On `specs/002-amend-member-check/tasks.md` in this repository, with
`--repo .` and `--exclude 'bench/**'`, the output fell from 339 lines to
296 when the rule was dropped. On deckh's `tasks.md` in its own repository
it fell from 1988 lines to 1767. The part of the output before the DRAFT
lists did not change in either run. The user decided on 2026-11-14.

## Decisions

1. **A bare file name is not a mention.** A file that holds only a task
   path's file name, such as `lib.rs` or `tools.py`, no longer joins a
   draft. The lead greps the repository for a changed file's name when the
   name is distinctive, and adds what it finds by hand.
2. **These rules stay, from ADR 0044 decision 4:** the exact path, also
   right after a `/` or `./`; a partial path that holds a `/` and is the end
   of the task path, when the task path's file name has an extension, such
   as `harness/tools.py` for `apps/ai/harness/tools.py`, never
   `other/tools.py`, and never `x/Makefile` for `crates/x/Makefile`; the
   dotted module name; a CI workflow's `on.<event>.paths` glob that matches
   the path.
3. **A partial path prints as a path.** Its reason is `names path
   <partial path>`, such as `names path harness/tools.py`, with a leading
   `./` left off. The reason `names file <name>` is gone, so no line tells
   the lead that the dropped rule still runs.

## Alternatives rejected

- Keeping the rule for names that few tracked files share. The count
  changes as the repository grows, so the same `tasks.md` would give a
  different draft over time, and the lead could not predict which names
  count.
- Keeping the rule with a list of common names to skip, such as `lib.rs`
  and `mod.rs`. Each repository has its own common names, and a list the
  lead does not see hides files.

## Consequences

- A file that names a task path only by its file name, such as a test that
  lists `"tools.py"`, is missing from the draft. The lead skill tells the
  lead to grep for a distinctive file name.
- On deckh's `tasks.md`, the three files ADR 0044 was written to find,
  `apps/ai/.importlinter`, `apps/ai/tests/test_harness_targets.py` and
  `.github/workflows/evals-harness-replay.yml`, are still in the drafts, by
  the dotted module, path and CI glob rules.
- `docs/api/cli.md`, the lead skill and its reference describe the rules as
  they now are.
