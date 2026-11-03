# Task: close deletes the closer's cache folder

A team's closer works in a clone kept at `$TEAMWIRE_HOME/cache/closer/<team>/`, which can hold a large `target/` folder. `teamwire close` deletes the team's state directory but leaves that cache folder behind forever.

End state, in crate `teamwire-close`:

- A whole-team close that deletes the team state (`close(ctx, CloseTarget::Team, CloseOpts::default())`) also deletes `<home>/cache/closer/<team>`, and the close still reports no stopped step.
- The plan's `delete_state` step (from `plan_close`) names the cache folder's path in its detail when the folder exists.
- With `CloseOpts { keep_state: true, .. }`, the cache folder and its contents stay.
- When the cache folder is a symbolic link, close removes the link only. Nothing in the folder it points to is touched.

## How to work and finish

- The repository is teamwire, a Rust tool, with a Bun and TypeScript dashboard in `ui/`. Read `AGENTS.md` first, and `ui/AGENTS.md` for the dashboard.
- Add a test for each change, in the crate's `tests/` folder for Rust and beside the file as `<name>.test.ts` for the dashboard.
- Check your work with the tests of each crate you changed (`cargo test -p <crate> --locked`), `bun test` in `ui/` if you changed it, and `TEAMWIRE_BASE=f1ac22cb scripts/gate.sh fast` from the repository root. This checkout has no `main` branch, so the gate needs that variable.
- Commit the finished work in this repository. Do not push; there is no remote. End your final message with the name of the branch that holds the finished work.
