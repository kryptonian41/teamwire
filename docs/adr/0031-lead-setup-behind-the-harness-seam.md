# ADR 0031: The lead setup sits behind the harness seam

Status: accepted, 2026-10-30. Supersedes the `Harness` row of ADR 0004's
trait table for the lead setup: a harness now also knows how to install,
remove and check its lead's setup. Points 1 to 10 of ADR 0004 still hold,
and point 9 now covers a fourth suite. ADR 0023 point 9 still holds: the CLI
passes the shared skill text in.

## Context

ADR 0018, ADR 0023 and ADR 0030 each gave one harness a lead setup: a
Claude Code plugin, a Codex plugin, a pi package. Each was built as the
previous one's copy, beside it, with no interface in `teamwire-core`:

- each adapter crate had its own `LeadSetupError` (pi's a different shape),
  its own `ApplyError`, the same 14-line apply loop, the same file-writing
  loop and the same "run the program, say what went wrong" code;
- Codex and pi each defined `LeadCommand { bin, home }`, and doctor parsed
  `--home` back out of Claude's MCP entry to build them;
- the CLI switched on the harness id in `install` (three id constants, a
  `Plan` enum with one variant per harness, three-way apply and note code,
  uninstall written three times) and in `doctor` (three check functions,
  the literal names `codex` and `pi` looked up on PATH while install used
  each adapter's configured program, and no PATH or version line for pi).

ADR 0004 point 1 says adding a harness is a new crate plus one
`add_harness` call. For a harness that leads, it was also edits in four
CLI files.

## Decision

1. **One trait.** `teamwire_core::LeadSetup` is what install, uninstall
   and doctor ask of a harness: its program and a label, whether it is the
   default lead harness, `status` (`Missing`, `Stale(problems)`,
   `Current`), `plan_install`, `plan_uninstall`, `apply`, its doctor checks,
   and checks of its program beyond the version. `Harness::lead_setup()`
   hands it out, `None` by default. `Registry::lead_setups()` lists every
   harness that has one, sorted by id, and the CLI loops over that list.
   The signatures are in `docs/api/core.md`.

2. **The change list is opaque to the CLI.** A plan is a `LeadPlan`: a list
   of `LeadChange` and a note to print after it is made. The CLI prints a
   change and passes it back to the harness that planned it; it never looks
   inside. A change carries a `LeadStep`: write files, delete a directory,
   run the harness's own program, or a step only the adapter knows (Claude
   Code's removal of the ADR 0012 skill, which must not follow a symlinked
   folder). Each adapter keeps its own typed change list for its own tests
   and maps it to steps.

3. **Shared code once, in core.** `apply_in_order` is the one apply loop,
   `make_step` makes a shared step through `Platform`, `run_program` runs
   the harness's program under `LEAD_SETUP_TIMEOUT` (30 s) and words its
   failure, and `LeadSetupError`, `ApplyError` and `LeadCommand` exist once.
   Each adapter renders its own entry from `LeadCommand`, so doctor no
   longer reads Claude's MCP arguments.

4. **Writes go through the platform.** `Platform` gains `create_dir_all`
   and `remove_dir_all`, so every write a lead setup makes passes the
   platform, and a `FakePlatform` with `fail_writes` stops an apply partway.
   Both default to `Unsupported`, so the many test platforms that never
   write a lead setup need no change.

5. **The default lead harness.** `LeadSetup::default_lead()` is true for
   Claude Code only. Doctor fails when its program is missing; install
   without a terminal or `--harness` installs it; uninstall always plans it
   and stops at its refusal. Every other harness is optional: doctor names
   it only when its program is on PATH, uninstall plans it only when its
   program runs and skips it with a line on stderr when it refuses.

6. **Doctor iterates the harnesses.** For each, in this order: the default
   lead's program (on PATH, version, its own checks, such as Claude Code's
   launch flags), then the host's program, then every other harness's
   program that is on PATH. pi now has the `pi on PATH` and `pi version`
   lines that Codex had. Then each harness's lead-setup checks. The
   program is the one each adapter's configuration names, as install uses.
   The check names, details and exit codes are otherwise unchanged.

7. **A fourth contract suite.** `teamwire_fakes::contract::lead_setup_contract`
   runs against `FakeLeadSetup` and the three adapters, on a `FakePlatform`
   that never starts the program: a fresh status is missing; planning
   writes nothing; files sit in the state directory and only the harness's
   own program runs; a write failure stops apply at the first change; a
   failing program stops apply there, with every earlier change made and
   named; a second plan skips the files already written; uninstall deletes
   them. `FakeHarness::with_lead_setup` lets a test give the fake harness a
   lead setup.

## Consequences

- Adding a harness that can lead is one crate that implements `Harness`
  and `LeadSetup` and passes both suites, plus its `add_harness` call. The
  CLI's install, uninstall and doctor need no edit.
- The order of install's plans follows the selection, as before; uninstall
  and doctor follow the registry's id order, which for `claude-code`,
  `codex` and `pi` is the order they had.
- `LeadPlan::note` is decided when the plan is made, from the changes it
  holds, so a note never names a change that was not planned.
- The contract cannot check that a registration took effect, because the
  fake platform never runs the harness's program. Each adapter's own tests
  still cover that with the files the program would write.
