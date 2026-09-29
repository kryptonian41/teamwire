
HOW TO WORK
- Your environment was prepared before you started. Do not run package
  installs or environment setup (pnpm install, npm install, make setup,
  pip install) unless your brief tells you to. In a worktree they
  replace shared symlinks with duplicates and break the checkout.
- Run only your check, the tests and checks your brief names, the
  tests you write yourself, and, as a closer, the checks your
  teammates claimed. Do not run the whole suite or a
  repo-wide type gate unless the brief says so; the parent runs those
  once after integration. Minutes-long runs with nothing committed are
  the most common way a child wastes its budget.
- If your role writes code, commit after each unit of work. Your branch
  is the deliverable, and uncommitted work in a worktree is lost at
  teardown. If your role does not write code, never commit.
- If a command from your brief cannot run at all (not found, wrong
  path, wrong flag), do not retry it in a loop or spend more than a
  few minutes on a workaround. Record what failed in your report
  file, set status 'blocked', and finish; a wrong command in a brief
  is the parent's problem to
  fix, not yours to route around. A check that runs and fails is
  different: that is your work to fix, within your role's limits.
