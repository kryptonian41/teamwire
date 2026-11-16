# ADR 0049: `teamwire speckit draft` skips imports and bare root names

Status: accepted, 2026-11-16. Supersedes in part:

- ADR 0044 decision 4: the dotted module name (rule A below), and the exact
  path for a file at the repository root (rule B).
- ADR 0047 rule 1: which words reach the code rule (rule E).
- ADR 0047 rules 2 and 3: the order of the new-file rule and the
  partial-path rule (rule D).
- ADR 0047 rule 4: the lines printed for the lead, which gain a section
  (rule C).

The rest of those ADRs, and ADR 0048, still hold. ADR 0044 and ADR 0047
keep their words as history.

## Context

On real task lists the drafts were too noisy to use, and two readings were
wrong. Team `drfix`'s investigator measured four problems on 2026-11-16,
with teamwire's `specs/001-deckh-fixes/tasks.md` and
`specs/002-amend-member-check/tasks.md` (`--repo .`, `--exclude 'bench/**'`)
and deckh's `tasks.md` in its own repository:

- **Module names.** In deckh's run, 1,352 of the 1,767 output lines came
  from the dotted module rule. 878 lines said `names module
  shared.db.enums`, and 869 of those files named the module only in an
  import. Over all 31 modules, 1,323 files named one only in imports and
  29 named one elsewhere. The rule also built a module name for a file
  that is not Python: `agents/deck_editor_agent/prompts/system.md` gave
  `prompts.system`.
- **Root files.** A root file's path is its bare name, so the exact-path
  rule matched `AGENTS.md` as a bare word, as `@AGENTS.md`, and right after
  any `/`, as in `apps/ai/AGENTS.md`. Of the root-file lines, 21 of 118
  named the root file in deckh's run, 0 of 15 in spec 001's and 5 of 43 in
  spec 002's.
- **Bare task words.** deckh's T004 names `AGENTS.md` and `CLAUDE.md` as new
  files in `apps/ai/gateway/decks/`. Both resolved to the root files,
  which made a false share between Phase 1 and Phase 9. No rule tells T004's
  meaning from T065's, which means the root `AGENTS.md`, by the word alone.
- **Two readings.** The new-file rule ran before the partial-path rule. So
  in teamwire's repository `tests/preset.rs` became a new file in the root
  `tests/` folder, though `crates/teamwire-cli/tests/preset.rs` is the one
  tracked file that ends in `/tests/preset.rs`. No word in the three real
  lists hit this, but 127 possible words in teamwire's repository would.
- **Line lists and possessives.** `crates/teamwire-msg/tests/send.rs:718,852`
  and `contracts/amend-cli.md's` held a `,` or a `'`, so the code rule
  turned them away. Two real words in spec 002 hit this.

Before these rules, the runs printed 1,767 lines for deckh, 613 for spec 001
and 300 for spec 002, at `b8d10cbb`. The user picked the rules below from
the measured options on 2026-11-16.

## Decisions

A. **A module name counts only outside import lines.** Only a task path
   that ends in `.py` has dotted module names. A module name counts as a
   mention only when one of its whole-token matches sits on a line that is
   not an import of it. An import line, after leading white space, is
   `import M`, `import M as m`, `import a, M` or `from M import X`. A match
   in `from M.sub import Y` is no whole token. Any other line counts, such
   as a comment, a doc, a string or a line of `.importlinter`.
B. **A root file is mentioned only by a relative link to the root.** For a
   task path with no `/`, an occurrence counts only when one or more `./` or
   `../` segments come right before the name, and that relative path,
   resolved from the folder of the file that holds it, is the repository
   root: `../../AGENTS.md` in `apps/ai/CLAUDE.md`, or `./AGENTS.md` in
   `README.md`. A bare word never counts, even in a file at the root, and
   neither does `@AGENTS.md`, `../AGENTS.md` in `apps/ai/CLAUDE.md` or
   `apps/ai/AGENTS.md`. The reason prints the link as written, such as
   `names path ../../AGENTS.md`. A CI `paths:` glob that matches a root file
   still counts.
C. **A bare task word read as a root file is printed.** A task word with no
   `/` still resolves to the root file of that name, and still counts in
   the overlap and the drafts. Each such word is also printed, right after
   `Names that are not repository paths:`, under
   `Bare names read as root files:`, one line per task, such as
   `T004: AGENTS.md, CLAUDE.md`. An empty section says `none`.
D. **A partial path with two readings is no path.** When a word is not
   itself tracked, the new-file rule would accept it, and one or more
   tracked paths end in `/<word>`, the word is no path. It is printed under
   `Names that are not repository paths:` with the tracked path it also
   ends, as `tests/preset.rs (also ends crates/teamwire-cli/tests/preset.rs)`,
   or `(also ends a/x.rs and 2 more)` when it ends several, naming the first
   in sorted order. It counts in the overlap as a word that does not
   resolve does, and goes in no draft. A word that only the new-file rule
   accepts stays a new file, and a word that only the partial-path rule
   resolves stays that tracked path.
E. **A line list and a possessive come off before the code rule.** A list
   of `:N` and `:N-M` suffixes joined by commas, such as `:718,852` or
   `:13-48,84-115`, comes off as a single suffix does. A trailing `'s` or
   `’s` comes off too. Both happen before ADR 0047 rule 1, which otherwise
   holds: `"z/c/**".to_owned(` is still code.

## What each rule loses

- Rule A loses a file that only imports a changed module. When a task
  renames or removes a name, the lead's grep for that name finds those
  files, and the lead skill already tells the lead to grep for changed
  names. It keeps attribute access that looks like a module, such as
  `harness.events` in `assert harness.events == [...]`.
- Rule B loses prose mentions such as "root AGENTS.md": 21 lines in deckh's
  run and 5 in spec 002's. The lead skill tells the lead to grep for a
  changed root file's name.
- Rule C adds a section the lead reads. It drops nothing.
- Rule D makes the lead add a real new root file by hand when its name also
  ends a tracked path.
- Rule E reads the files a task names but does not change as task paths,
  such as spec 002's T005, which names the callers it leaves alone.

## Alternatives rejected

- Dropping the module rule. deckh's run fell to 444 lines, but
  `apps/ai/.importlinter`, which the deckh implementer needed, left the US2
  draft.
- Skipping the module rule in every Python file that holds the text.
  deckh's run fell to 449 lines, but the rule also dropped real mentions
  inside Python files, such as a module path in a test's mock string.
- Collapsing a module with many lines into one line. deckh's run stayed
  above 500 lines.
- A root file matched after `./` or `../` with no check of where the link
  leads. deckh's `../AGENTS.md` links in nested files point at nested
  files.
- Making a bare task word never resolve. That fixed T004's two words and
  lost six right ones, such as deckh's T061 `sonar-project.properties`.
- Running the partial-path rule before the new-file rule. A task that
  creates a root `tests/x.rs` beside a crate's `tests/x.rs` would move onto
  the old file without a line.

## Consequences

- With these rules, from the `drfix/impl` branch, deckh's run prints 370
  lines. It keeps T051 under `Tasks whose files no other task names`,
  `Tasks that change no file: T058, T067`, the share of
  `apps/ai/gateway/decks/tools.py` by T035 and T043, and
  `apps/ai/.importlinter`, `.github/workflows/evals-harness-replay.yml` and
  `apps/ai/tests/test_harness_targets.py` in the drafts. Spec 001's run
  prints 607 lines and spec 002's prints 273.
- `docs/api/cli.md`, the lead skill and its reference describe the rules,
  the new section and what the lead must grep for.
