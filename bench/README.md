# bench: tools for the gains study

Python 3.9, standard library only. Every script has `--help`, reads real data
read-only, and writes only under `bench/results/` (git ignores it) or the folder
a flag names. Run all commands from the repository root.

Check the tools: `python3 -m unittest discover -s bench`

The scripts in the order the lead uses them:

1. **Mine the archive** (plan section 4). One row per team, to `bench/results/mine.csv`.

   ```bash
   python3 bench/mine.py
   ```

2. **List candidate commits** (plan 5.1), to `bench/results/candidates.csv`.

   ```bash
   python3 bench/candidates.py
   ```

3. **Make a leak-free clone and the run's settings** (plan 5.2). Give the task's
   commits (oldest first); the clone ends at the first one's parent, and the tool
   refuses when a task commit is already in that history. Each run gets a random
   folder `bn<random>` under `--root`. Prints the clone path. Pass `settings.json`
   to `claude --settings`. `run.json` holds the commits and the start time.

   ```bash
   python3 bench/clone.py --commit <sha> [--commit <sha> ...] --task <task id> --setup <setup name> --root <runs folder>
   ```

   **Range mode** for a task given as a start and an end commit (merges allowed). The
   clone ends at the start commit itself; the tool refuses when the start is not an
   ancestor of the end. `run.json` holds `base`, `target` and `parent` (= base).
   Use `--base/--target` or `--commit`, never both.

   ```bash
   python3 bench/clone.py --base <start> --target <end> --task <task id> --setup <setup name> --root <runs folder>
   ```

4. **Grade.** First once per task: the reference applies the task commits to the
   first one's parent with `git cherry-pick`, in the order given, and runs the
   hidden tests there. It fails, naming the file, when a hidden file adds no
   passing test, a crate does not build, or `bun install` fails.

   ```bash
   python3 bench/grade.py reference --commit <sha> [--commit <sha> ...] --out bench/results/<task>/reference
   ```

   Then after each run. `score` reads the run's clone and never writes to it: the
   gate and the tests run in a throwaway copy that has the parent's `scripts/` and
   `xtask/`. Use `--ref <branch or sha>` when the result is not the clone's working
   tree (the teamwire lead's `afk/<team>` branch). Set `CARGO_TARGET_DIR` to a shared
   folder to avoid a full rebuild per score.

   ```bash
   python3 bench/grade.py score --commit <sha> [--commit <sha> ...] --run-dir <runs folder>/bn<random> \
     --expected bench/results/<task>/reference/expected.json --out bench/results/<task>/<setup> [--ref <branch>]
   ```

   Range mode: `reference` checks out the end commit (no cherry-picks). The hidden files
   are the test-only files added or changed between start and end; a file deleted in
   the range is not hidden. `score` compares with the start commit, and refuses when
   `expected.json` or `run.json` name another start or end.

   ```bash
   python3 bench/grade.py reference --base <start> --target <end> --out bench/results/<task>/reference
   python3 bench/grade.py score --base <start> --target <end> --run-dir <runs folder>/bn<random> \
     --expected bench/results/<task>/reference/expected.json --out bench/results/<task>/<setup> [--ref <branch>]
   ```

   Then once, for the blind grader (writes `diffs/`, `rubric.md`, `order.txt`; the key
   goes to `--key-out`, which must be outside the folder the grader reads):

   ```bash
   python3 bench/grade.py blind --runs bench/results/<task>/<setup> ... --brief <task>=<brief file> --seed 1 \
     --out bench/results/blind --key-out bench/results/blind-key.json
   ```

5. **Scan for leaks.** Lists every tool call in the run's transcripts that names the
   source checkout, `~/.teamwire/archive`, `~/.claude/projects` or the runs folder.
   Exit 1 on any hit: report that run and throw it out. `--allow` takes expected
   paths, such as the run's own `TEAMWIRE_HOME`.

   ```bash
   python3 bench/leaks.py --clone <clone> --source /Users/apoorvagarwal/Projects/teamwire --runs-root <runs folder> [--allow <path>]
   ```

6. **Price a run** (plan 5.5), from every transcript under the clone path. A transcript
   that began before `run.json`'s `started_at` is left out. `--session <id>` adds a
   member whose cwd is elsewhere (a closer's cache clone).

   ```bash
   python3 bench/cost.py --clone <clone> [--claude-json out.json] [--session <id>]
   ```

Prices live in `bench/prices.json` with the date they were read. A model id that
is not in the table (or its alias list) stops the script with the id named.

## Rules the tools follow

- Tokens are counted once per `message.id` across all files of a run, from the
  last line of the id (subagent files hold a placeholder `output_tokens` on early
  lines). A line of model `<synthetic>` is skipped.
- A cache write without the 5m/1h split is priced at the 1h rate and shown as
  `cache_creation_unsplit`.
- The grade is the set of hidden tests that pass in the reference clone. A test
  missing from a run's output, or in a binary that did not compile, fails.
  Each hidden integration-test binary runs as its own `cargo test --test <name>`
  (`raw-cargo-<crate>-<name>.txt`), so one that does not compile fails only its own
  tests. Its `error` lines go to `compile_errors` in `score.json` and `expected.json`.
- `grade.py score` takes the outside-files list and the diff from the run's clone
  untouched, then copies it; the hidden tests go into the copy only. Running it
  twice gives the same `diff.patch`.
- `mine.csv` has a `lead_shared_with` column: teams whose time windows overlap on
  one lead session carry the same lead tokens, so do not add them up.
- Bun's per-test results come from its JUnit reporter, because bun prints no
  `(pass)` lines to a pipe.
