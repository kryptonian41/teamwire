# Handoff guide for a teamwire lead

Your context passed its handoff point. A fresh lead session takes over
your teams: {{TEAMS}}. It starts in a pane beside yours with none of this
conversation, reads the document you write, and carries on from it.

Before you write: finish the step you are on, and leave each team's watch
running. If you are waiting for the user's answer, hand off after you have
it.

Write the document to {{DOCUMENT}}, never into a repository. Its sections,
in this order:

1. **The goal**: what the user asked for, in the user's words, and the
   user's standing instructions, such as "nobody pushes until I agree".
2. **Files**: the plan, the team specs and each brief, by path, and the
   ticket refs.
3. **Each team**: which of the teamwire skill's nine steps the run is at.
   For each member: what it does now, what result you expect, and what you
   meant to do when it reports.
4. **Handled so far**: for each member, the time of the last result you
   acted on, and the id of the last message you read.
5. **Waiting on the user**: each question you asked and have no answer to.
6. **Decisions in no file**: each decision and its reason.
7. **Findings**: which findings hold, which you declined and why, and how
   many fix rounds each owner has used.
8. **Pull requests**: the repository, the numbers and the exact
   `teamwire pr-watch` command.
9. **Next**: the next steps, in order.
10. **Skills to load**: the skills the new lead loads first.

Point at plans, briefs, reports and commits by path or commit id; do not
paste them. Redact secrets: tokens, keys, passwords and personal data.

If the user is present, tell them in one line that you are handing off,
and where the document is.

Then run `teamwire lead --handoff` as the last action of your turn. It
opens the new lead's pane, gives it your teams, and closes your pane.
To see what it will do first, run `teamwire lead --handoff --dry-run`.

---

Adapted from the `handoff` skill of mattpocock-skills 1.3.1, by Matt Pocock,
under this licence:

MIT License

Copyright (c) 2026 Matt Pocock

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
