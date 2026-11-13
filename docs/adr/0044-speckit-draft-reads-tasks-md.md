# ADR 0044: `teamwire speckit draft` reads `tasks.md`

Status: accepted, 2026-11-13. Supersedes ADR 0042 decision 1 in part: the
`tasks.md` reader that decision deferred now exists. The rest of ADR 0042
decision 1 still holds. ADR 0042 keeps its words as history.

## Context

ADR 0042 deferred a reader for spec-kit's `tasks.md`. Until one existed, the
lead read the task paths by hand to decide which phases and stories could
run in parallel, and grepped the repository for the files each member
needed. Team `deckh`'s lead missed two tasks that could have run in
parallel, because no step listed the tasks that touch no file or touch only
their own files. Its `owns` lists also missed three files: a CI workflow
whose `paths:` glob matched a task path, a lint config that named a module
by its dotted name, and a test that named a file in a list. The user agreed
the reader and its matching rule on 2026-11-10 and 2026-11-13.

## Decisions

1. **The command.** `teamwire speckit draft <tasks.md> [--repo <dir>]`
   reads one `tasks.md` and prints text. It writes no file and no store
   row, and has no `--json`. `--repo` defaults to the git top level of the
   folder that holds `tasks.md`.
2. **What it reads.**
   - A phase starts at a `## Phase <n>: <title>` heading.
   - A task line is `- [ ]`, `- [x]` or `- [X]`, then `T<digits>`, an
     optional `[P]`, an optional `[US<n>]`, and the description. A task
     with no `[US<n>]` label belongs to the "setup and polish" group.
   - A path is a token in backticks, or a bare token, with the
     punctuation around it removed (FR-013). A token with a `/` is a path
     when it does not start with `/`, is not inside a markdown link's
     `](...)`, and names a tracked file, a tracked folder, or ends in
     `.<ext>`. A token with no `/` is a path only when it names a tracked
     file at the repository's root, such as `README.md`. So a route such as
     `"/agui/deck_editor"`, an event name such as `knit.progress`, an
     absolute path and a URL are not paths.
   - A folder token, such as `apps/ai`, counts in the overlap table as a
     prefix of the files under it, and adds no file to the mention search.
   - A brace set such as `src/{a,b}.rs` names each of its files.
   - A `:N` or `:N-M` suffix is stripped, so `send.rs:297-321` and
     `send.rs` are one file.
   - An indented line under a task line belongs to that task, until the
     next task line, heading or blank line.
   - A token that holds `*` is a glob. It matches the repository's tracked
     files (`git ls-files`) with the same matcher as verify's `owns`
     (`teamwire_verify::Owns`, where `*` stays inside one path segment), and
     each match counts as a task path. A glob that matches no tracked file
     counts as itself, like a path that does not exist yet.
3. **What it prints.** The files each pair of phases shares, the stories
   that share files, the tasks that name no file, the tasks whose files no
   other task names, every workflow it could not read, and one DRAFT `owns`
   list per story plus one for setup and polish.
4. **What counts as a mention.** A tracked file joins a draft when it holds
   one of these for a task path of that story:
   - the exact path text, also right after a `/` or `./`, as in
     `./apps/ai/harness/tools.py` or `$ROOT/apps/ai/harness/tools.py`;
   - a dotted module name of the path, any dotted suffix of the path
     without its extension with at least two parts, such as `harness.tools`
     for `apps/ai/harness/tools.py`;
   - the full file name with its extension as a whole token, such as
     `tools.py`, or right after a `/` when the words before it end the path
     too, so `harness/tools.py` counts for `apps/ai/harness/tools.py` and
     `other/tools.py` does not;
   - a `.github/workflows/*.yml` or `*.yaml` file whose `on.<event>.paths`
     glob matches the path.

   A bare stem such as `tools` never counts: it matches too many files.
   Each added file carries its reason, such as `names module harness.tools`
   or `CI paths glob apps/ai/**`.
5. **What it leaves out.** The feature folder that holds the `tasks.md`
   names every path, so its files are never mentions.
6. **A workflow it cannot read is named.** The YAML reader refuses anchors,
   aliases and tags. Such a workflow is printed under `Unread workflows`
   with the reason, never counted as matching nothing.
7. **Errors.** A missing or unreadable `tasks.md`, or one with no task
   lines, exits 1 and names the file and the problem. Exit 3 stays for
   usage errors.
8. **The lists are drafts.** The output says `DRAFT` before the lists. The
   lead checks each list and keeps the manual search for names the reader
   cannot see, such as a name a file builds at run time.

## Alternatives rejected

- A preset step that has the lead do the overlap by hand. The `deckh` lead
  already had that step, and it missed the tasks.
- Matching a bare stem. The user excluded it: `tools` is in too many files.
- A new glob dependency in `teamwire-cli`. That changes `Cargo.lock`;
  `teamwire_verify::Owns` already matches globs, so the command reuses it.
- A new crate. The layering rules do not call for one.

## Consequences

- `teamwire_verify::Owns` is public, with the same matching.
- The lead skill, its reference, the `speckit` preset, the AFK skill and
  `docs/api/cli.md` tell the lead to run the command, check its drafts and
  show the table to the user before launch.
