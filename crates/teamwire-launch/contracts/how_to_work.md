
HOW TO WORK
- Your environment was prepared before you started. Do not run package
  installs or environment setup (pnpm install, npm install, make setup,
  pip install) unless your brief tells you to. In a worktree they
  replace shared symlinks with duplicates and break the checkout.
- Run only your check, the tests and checks your brief names, the
  tests you write yourself, and, as a closer, the checks your
  teammates claimed. Do not run the whole suite or a
  repo-wide type gate unless the brief says so; the integrator or the
  closer runs those once, on the finished branches. Minutes-long runs
  with nothing committed are the most common way a child wastes its
  budget.
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
- Run each shell command directly, never inside `sh -c` or `bash -c`.
  Your harness cannot check a wrapped script, and when one also runs
  `rm` it can stop at a permission dialog even with approvals turned off.
  Nobody answers that dialog: the command is denied after about two
  minutes, and your run stalls until then.
- Never run `git worktree add`, `git worktree remove`, `git worktree move`
  or `git worktree prune`, and never
  write into another member's directory. Every member's checkout is shared
  state: one member that removes a worktree can delete a teammate's
  checkout mid-run. If you need a second worktree, ask the parent as
  HOW THIS TEAM WORKS TOGETHER says, and carry on with what does not
  need it. If it is still missing at the finish, report blocked to the
  lead: set status 'blocked' in your completion.
- Put temporary files, such as scripts and sample inputs, in your own
  scratch folder, the path in `$TEAMWIRE_SCRATCH`, or in the scratch
  folder your harness gives this session. Never put them in the
  repository or in a shared /tmp path: other members share /tmp and
  can overwrite them. The `$TEAMWIRE_SCRATCH` folder is deleted when
  the team closes.
- Do not end a turn while a background job you still need is running.
  Closing a member ends its background jobs with its tab.
