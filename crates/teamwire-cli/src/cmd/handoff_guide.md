# Handoff guide for a teamwire member

Your context window is filling. The lead will give you a fresh session in
the same seat, with the same brief, report file and token, and the same
worktree and branch if you have them. The new session starts empty and
reads your brief and the document you write.

Before you write: finish the step you are on. If your role writes code,
commit your work (a handoff refuses uncommitted changes in your worktree)
and add each commit's progress line to your report file.
If your role does not write code, commit nothing.
Your directory may be shared, and the changes in it may not be yours.
Your report needs its 'Decisions I made' list ('none' if there were none),
or the turn-end check holds your stop once.

Write the document to the path the note named, `reports/<your role>.handoff.md`
in your team directory, never `$TMPDIR` or the repository. Its sections:

- **Done**: what is finished, with the commit ids.
- **Next**: the next steps, in order.
- **Open questions**: what you asked and have no answer to, with message ids.
- **Files and commands**: the files that matter now, and how to build and check them.
- **Decisions so far**: each decision and its reason.
- **Skills to load**: the skills the next session should load first.

Point at the brief, the report, commits and diffs by path or commit id; do
not copy them. Redact secrets: tokens, keys, passwords and personal data.

After you write, record your completion with status `done` and a summary that
starts with `handoff:`, then end your turn. This one `done` does not mean your
check passed: the `handoff:` prefix tells the lead to run the handoff. Use the
team_done tool, or the `done` command your contract names, with `--status done`.

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
