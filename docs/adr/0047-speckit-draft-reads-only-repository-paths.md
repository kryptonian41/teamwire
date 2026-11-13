# ADR 0047: `teamwire speckit draft` reads only repository paths

Status: accepted, 2026-11-14. Supersedes ADR 0044 decision 2 in part: which
words of a task line count as paths. The rest of ADR 0044 decision 2, and
its other decisions, still hold. ADR 0044 keeps its words as history.

## Context

The lead ran `teamwire speckit draft` on `specs/002-amend-member-check/tasks.md`
in this repository. The US1 DRAFT `owns` list held words that are not files
of the repository:

- code in backticks: `"z/c/**".to_owned(` and
  `Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/skills/teamwire/SKILL.md`;
- example globs from a test's description: `crates/b/**`, `crates/c/**`;
- partial paths: `cmd/send.rs`, `cmd/verify.rs`, `skills/teamwire/SKILL.md`,
  `teamwire/skills/teamwire/SKILL.md`, `contracts/amend-cli.md`;
- a template path: `specs/NNN-name/tasks.md`.

ADR 0044 counted a word with a `/` and an extension as a path, so each of
these became a task path. The mention search then added every file that
named one of them, such as each spec-kit skill that names `tasks.md`. It also
added hundreds of archived study files under `bench/`, which name paths but
are records, not live files. The lead set the rules below on 2026-11-14.

## Decisions

1. **Code is not a path.** A word that holds `"`, `'`, `(`, `)`, `,`, `;`,
   `!` or `::` is not a path. The rule runs on each word after the
   punctuation around it is taken off and a brace set is expanded, so
   `src/{a,b}.rs` and `(see src/a.rs)` still name paths. The reader splits
   a line at white space, so a word never holds white space.
2. **A path must resolve.** After ADR 0044's rules, a word is a path when
   it names a tracked file or folder, or when its parent folder is tracked,
   as a new file in an existing folder is. A glob counts when it matches at
   least one tracked file, and then each match is a task path. A glob that
   matches none is not a path.
3. **A partial path resolves when it is unique.** A word that is the end of
   exactly one tracked path, at a `/`, is that path: `cmd/send.rs` is
   `crates/teamwire-cli/src/cmd/send.rs` when no other tracked file ends in
   `/cmd/send.rs`. A word that ends several tracked paths, or none, is not a
   path. Rule 2 runs first, so a new file in a tracked folder stays as
   written.
4. **Nothing is dropped silently.** Each word that rules 1 to 3 turn away,
   and that holds a `/`, is printed under `Names that are not repository
   paths:`, with its task id. A new file in a new folder lands there, and
   the lead adds it to an `owns` list by hand. These names go in no DRAFT
   list. The words ADR 0044 already turned away, such as a URL, an absolute
   path, a placeholder or a word with no extension, stay unprinted.
5. **`--exclude PATTERN`.** The flag, repeated, takes one glob each, as
   `amend --owns` does, matched as verify matches `owns`. A tracked file
   that matches one is left out of the mention search. Paths still resolve
   against every tracked file. There is no default exclusion: the command
   cannot know which folders of a repository hold records. The lead skill
   tells the lead to pass `--exclude 'bench/**'` in teamwire's repository,
   and in general to exclude folders of archived records.

## Alternatives rejected

- Resolving a partial path against the feature folder first. spec-kit
  writes `contracts/x.md` relative to the feature folder, but rule 3
  already finds such a file when it is unique, and a second lookup order
  makes the result harder to predict.
- Excluding `bench/` by default. Another repository may keep live files
  under that name, and a default the lead does not see hides files.
- Dropping the turned-away words without a line. A new file in a new folder
  looks like any other word that does not resolve, so the lead must see
  them.

## Consequences

- A task that names only words the rules turn away is listed under
  `Tasks that change no file`, and its words are printed under `Names that
  are not repository paths`.
- `docs/api/cli.md`, the lead skill and its reference describe the rules and
  `--exclude`.
