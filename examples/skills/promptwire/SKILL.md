---
name: promptwire
description: "Give teamwire members vetted domain skills from the promptwire database. Use as a teamwire lead while you write briefs, once `command -v promptwire` has found the tool."
---

# Domain skills from promptwire

promptwire is a command-line tool that carries a curated database of agent
skills and prompts. Each entry has public evidence of use, a person read
every file before it went in, and its `evidence.notes` list its known
defects. You use it for one thing: handing a member a **domain skill**, the
vendor's own guide to a technology its task touches, such as Stripe,
Cloudflare Workers, Terraform, PostgreSQL schemas, SwiftUI or Playwright.

## Why a lead picks few, and only domain skills

SkillsBench (NeurIPS 2026, arXiv 2602.12670) ran agents on 84 tasks with and
without skills:

- Curated skills raised the pass rate by about 16 points on average, but by
  only 4.5 points for software engineering against 52 for healthcare. The
  gain is domain knowledge the model lacks.
- 16 of the 84 tasks got worse with a skill. A skill on every member hurts.
- Focused skills of 2 to 3 modules beat long documentation. Give a member
  at most two.
- Skills the model wrote for itself gave no gain. Whoever writes the briefs
  picks the skills, and a member works from the skills its brief names.

## Steps

1. **Name the domains.** For each member, list the technologies its task
   touches that have a vendor or a body of practice of their own: an API, a
   framework, a cloud service, a file format, a schema. General coding,
   refactoring, testing, debugging and review are not domains; a member whose
   task has only those gets no skill.
2. **Search each domain**, skills only:

   ```bash
   promptwire search <technology> --kind skill
   ```

   Keep a match whose summary covers what the member builds or checks.
3. **Read each match** with `promptwire show <id>`. Drop it when the table
   below names it, or when the member already loads a skill of the same name
   from a plugin. Note each known defect in `evidence.notes` that touches the
   task.
4. **Install it with the team**, once per team, before the brief goes out:

   ```bash
   promptwire install <id> --dir "$TEAMWIRE_HOME/teams/<team>/skills"
   ```

   The files land in `skills/<install_name>/`, where `install_name` comes
   from `promptwire show <id> --json`. A second member that needs the same
   skill reads the same folder; a second install refuses. This folder works
   before launch, because launch looks for the team's database, not its
   folder. `teamwire close` archives it with the team, which records what
   each member read.
5. **Write it into the brief.** Under "Read first", give the full path to
   `SKILL.md` and say which of its parts apply. Put each known defect that
   touches the task under "Decisions already made", with the reading you
   chose. A reviewer of that branch gets the same path as its checklist of
   what correct means in that domain, and its seat still limits it to
   correctness, security and data-loss findings.

The steps are done when every brief names its skills by full path, or the
member's task touched no domain that promptwire covers.

Keep the install in the team folder. The default target,
`~/.claude/skills/`, changes the user's own setup for every later session,
and `--project` puts untracked files in the member's worktree, which
`teamwire verify` flags as `uncommitted`. A path in the brief also serves a
member on another harness, which loads no Claude skills.

## Skills that fight a teamwire seat

| Entries | Why they do not fit |
| --- | --- |
| `mattpocock-grilling`, `mattpocock-teach` | They wait for a human to answer. |
| `superpowers-requesting-code-review`, `anthropic-doc-coauthoring`, `vercel-find-skills` | They start sub-agents or install more skills. |
| `sentry-iterate-pr`, `openai-gh-fix-ci` | They work from a pull request and its CI checks. A member's branch has no pull request, and members never push. |
| `sentry-find-bugs` | It diffs against the repository's default branch, not the member's `base`. |
| `mattpocock-handoff`, `superpowers-using-git-worktrees` | teamwire already hands off context and makes the worktrees. |
| `juliusbrussee-caveman`, `blader-humanizer`, `awesome-copilot-git-commit` | They change output or commit style, which the repository's own rules set. |
