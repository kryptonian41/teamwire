# Quickstart: check the three fixes by hand

These steps replay the cases of the spec on a throwaway team. They use a
temporary `TEAMWIRE_HOME`, so no real team is touched. The automated tests
named in [research.md](research.md) cover the same cases on the fakes; this
guide is for a person who wants to see it run.

## Prerequisites

- A build of this branch: `cargo build -p teamwire-cli --locked`.
- A scratch repository with one commit.
- `export TEAMWIRE_HOME=$(mktemp -d)`.
- A host (herdr or tmux). Step 1 of US1 starts a real agent session in it,
  so close it with `teamwire close demo` when done. The automated tests
  below need neither.

## US1: amend a check (F13)

1. Launch one implementer `api` with `check: "false"` and `owns: ["src/"]`.
2. Run `teamwire verify demo --role api`. Expected: `FAIL`.
3. Run `teamwire amend demo --role api --check true --reason "the old check always fails"`.
   Expected: exit 0, the output names the old and new check and says to
   verify again.
4. Run `teamwire log demo --role api`. Expected: a `note` from `lead` whose
   excerpt starts `CHECK AMENDED: the old check always fails`.
5. Run `teamwire close demo --role api --dry-run`. Expected: the refusal
   "api's verify at HEAD ... was recorded before the lead amended its check
   or owns".
6. Run `teamwire verify demo --role api`. Expected: `PASS`, and the first
   line of `$TEAMWIRE_HOME/teams/demo/verify/api.log` is `$ true   # in ...`.
7. Run `teamwire close demo --role api --dry-run`. Expected: no refusal.
8. Read `$TEAMWIRE_HOME/teams/demo/contract/api.md`. Expected: its check
   line holds `true`, not `false`.
9. Run `teamwire amend demo --role api --owns src/ --owns 'docs/{a,b}/**' --reason "docs too"`,
   then `teamwire teams --json`. Expected: `owns` is exactly
   `["src/", "docs/{a,b}/**"]`.
10. From a shell with `TEAMWIRE_ROLE=api` set, run any amend. Expected:
    exit 1, `refused.identity`.

## US2: the spec-kit removal rule (F14)

1. Run `teamwire preset speckit`. Expected: a `REMOVE SPEC-KIT.` rule that
   says the removal commit goes on the pull request branch, cut from the
   implementer's last verified head, never on the implementer's own
   branch, and names `teamwire amend` for a removal already on that branch.

## US3: the role table (F15)

1. Open `examples/skills/teamwire/SKILL.md`, section "Roles". Expected: every
   row's "May not" names `git worktree add`, `git worktree remove`,
   `git worktree move` and `git worktree prune`, and the implementer row
   also names `git stash`, `git reset --hard` and `git checkout -- .`.
2. Run `cargo test -p teamwire-launch --locked --test role_table_git_bans`.
   Expected: pass. Delete `git worktree move` from one row and run it
   again. Expected: a failure that names the kind and `git worktree move`.

## Automated checks

```bash
cargo test -p teamwire-store -p teamwire-msg -p teamwire-verify -p teamwire-close -p teamwire-launch --locked
cargo test -p teamwire-cli --locked --test amend --test preset --test close
cargo test -p teamwire-harness-codex -p teamwire-harness-pi -p teamwire-mcp --locked
scripts/test-changed.sh
```
