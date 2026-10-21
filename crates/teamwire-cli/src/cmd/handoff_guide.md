# Handoff guide for a teamwire member

Your context window is filling. The lead will give you a fresh session in
the same seat, with the same brief, worktree, branch, report file and token.
The new session starts empty and reads your brief and the document you write.

Before you write: finish the step you are on, commit your work (a handoff
refuses uncommitted changes), and add each commit's progress line to your
report file.

Write the document to the path the note named, `reports/<your role>.handoff.md`
in your team directory, never `$TMPDIR` or the repository. Its sections:

- **Done**: what is finished, with the commit ids.
- **Next**: the next steps, in order.
- **Open questions**: what you asked and have no answer to, with message ids.
- **Files and commands**: the files that matter now, and the commands that
  build and check them.
- **Decisions so far**: each decision and its reason.
- **Skills to load**: the skills the next session should load first.

Point at the brief, the report, commits and diffs by path or commit id
instead of copying them. Redact secrets: tokens, keys, passwords and
personal data.

After you write, record your completion with a summary that starts with
`handoff:`, then end your turn. The lead runs the handoff.

    teamwire done --status done --summary "handoff: <what is next>"

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
