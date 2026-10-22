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

3. **Make a leak-free clone and the run's settings** (plan 5.2). Prints the clone
   path. Pass `settings.json` to `claude --settings`. `run.json` holds the start time.

   ```bash
   python3 bench/clone.py --parent <parent sha> --task <task id> --setup <setup name> --dir /tmp/bench-<task>-<setup>
   ```

4. **Grade.** First once per task, in a clone at the original commit (several
   `--commit` flags for a task of several commits, oldest first):

   ```bash
   python3 bench/grade.py reference --commit <sha> --out bench/results/<task>/reference
   ```

   Then after each run:

   ```bash
   python3 bench/grade.py score --commit <sha> --run-dir /tmp/bench-<task>-<setup> \
     --expected bench/results/<task>/reference/expected.json --out bench/results/<task>/<setup>
   ```

   Then once, for the blind grader (writes `diffs/`, `rubric.md`, `order.txt`, `key.json`):

   ```bash
   python3 bench/grade.py blind --runs bench/results/<task>/<setup> ... --brief <task>=<brief file> --seed 1
   ```

5. **Price a run** (plan 5.5), from every transcript under the clone path.
   `--session <id>` adds a member whose cwd is elsewhere (a closer's cache clone).

   ```bash
   python3 bench/cost.py --clone /tmp/bench-<task>-<setup>/clone [--claude-json out.json] [--session <id>]
   ```

Prices live in `bench/prices.json` with the date they were read. A model id that
is not in the table (or its alias list) stops the script with the id named.

## Rules the tools follow

- Tokens are counted once per `message.id` across all files of a run. A line of
  model `<synthetic>` is skipped.
- A cache write without the 5m/1h split is priced at the 1h rate and shown as
  `cache_creation_unsplit`.
- The grade is the set of hidden tests that pass in the reference clone. A test
  missing from a run's output, or in a crate that did not compile, fails.
- `grade.py score` takes the outside-files list, the diff and the gate result
  before it copies the hidden tests in, so those three see only the agent's work.
