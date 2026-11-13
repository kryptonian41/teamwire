# ADR 0045: A waiver for a file holds until that file changes

Status: accepted, 2026-11-13. Supersedes ADR 0009 point 2 in part: for the
verify flags that name one file, a waiver now carries over to a later HEAD
while that file is unchanged. Every other flag keeps the old rule. ADR 0009
keeps its words as history.

## Context

ADR 0009 point 2 binds a waiver to the HEAD it was recorded at: "A waiver
does not carry over to a new HEAD." `verify_member` reads only the waivers
stored at the HEAD it verifies, so every commit a member makes after a
waiver raises the same flags again.

Team `deckh`'s spec member changed `.specify/memory/constitution.md`, a file
outside its `owns` that verify also counts as check config. The lead waived
`config-changed:.specify/memory/constitution.md` and
`outside-owns:.specify/memory/constitution.md` with a reason. The member's
next three commits changed only files under `specs/`, and
`git diff --stat` between the HEADs printed nothing for `.specify`. Each
commit still raised both flags, and the lead waived the same two flags four
times, with reasons that said the later commits did not touch the file. A
waiver the lead must repeat with nothing new to judge teaches the lead to
waive without reading. The user agreed the rule below on 2026-11-13.

## Decisions

1. **A file flag's waiver holds while the file is unchanged.** The flags
   that name one file are `test-deleted`, `config-changed`, `outside-owns`,
   `weakened`, `lint-removed`, `test-removed` and `asserts-removed`. A
   waiver for one of them, recorded at an earlier HEAD `w`, also counts at
   HEAD `h` when that file's content at `h` is the content at `w`.
   - verify compares the file's blob id at both commits, from
     `git ls-tree -z --full-tree <rev> -- <path>`. A file absent at both
     commits, as for a deleted test, counts as unchanged.
   - Only the two end states are compared. A file changed and then changed
     back holds the waived content, so the waiver holds.
   - Any change to the file ends the waiver, whatever made it: a commit, a
     merge or a rebase.
   - A waiver whose recorded HEAD git does not know, after a rebase or a
     garbage collection, does not carry over. An error from git on either
     side means the waiver does not count, never that it does.
2. **Every other waiver keeps ADR 0009 point 2.** A flag that names no file,
   such as `no-owns` or `bad-owns`, needs a waiver at the HEAD verify
   checks. `head-moved` is never waivable, as before. So does `uncommitted:<path>`: its change lives in the
   worktree, not in a commit, so "the file's content at the waived HEAD"
   does not describe what the lead waived.
3. **Recording does not change.** `teamwire verify --waive` still records
   the waiver at the member's HEAD now, for a flag that HEAD raises, and
   still needs a reason. The waiver table and its schema do not change: the
   HEAD stored with each waiver already names the blob to compare.
4. **A verify record is still needed at HEAD now.** VER-5's rule that
   removing a member, and `close`, accept only a record at the member's
   HEAD now stays. A carried waiver lets that new run pass without a new
   waiver; it does not replace the run.
5. **The flag carries its path.** verify's raw flag gets an optional path,
   set only by the file flags above. The path is not parsed back out of the
   flag id, because a path may hold `:` and `weakened` and `lint-removed`
   add `:<line>` after it. The `Flag` that a verify record stores keeps its
   fields, so nothing that reads a record changes.

## Rejected

- Storing the file's blob id in the waiver row. That needs a schema
  migration, and the HEAD stored today already gives the blob.
- Checking every commit between the two HEADs for a change to the file.
  That costs one git call per commit. The end states give the same answer,
  except for a change that was reverted, where the waived content is what
  is there.

## Consequences

- The deckh replay passes after one waiver per flag.
- A lead who waives a file flag has judged that file's content. A later
  change to the file raises the flag again, so the lead judges the new
  content.
- Tests in `crates/teamwire-verify/tests/verify.rs` hold both halves of the
  rule: `a_file_flag_waiver_holds_while_that_file_is_unchanged` and
  `a_waiver_for_a_deleted_file_holds_while_the_file_stays_deleted` for the
  carry-over, and `a_file_flag_waiver_ends_when_that_file_changes`,
  `a_file_flag_waiver_ends_when_a_merge_changes_that_file`,
  `a_waiver_at_a_commit_git_does_not_know_does_not_carry_over`,
  `a_waiver_for_a_flag_with_no_file_ends_at_the_next_commit` and
  `an_uncommitted_path_waiver_ends_at_the_next_commit` for the old rule.
