# ADR 0035: The lead's tool hook notes a member's work

Status: accepted, 2026-11-04. Adds a hook to the lead's setup on every
harness that can lead. Changes no decision of an earlier ADR.

## Context

The lead plans the work with the user, writes every brief and judges every
result. Members edit the code, run the builds and tests, merge the branches
and open the pull requests. The lead skill says so in its first section.

A skill is text, and a lead follows the text it has. On 2026-11-04 a scan of
this machine's Claude Code transcripts found 139 sessions that ran
`teamwire launch`. The scan counted what each of those sessions ran itself:

| The lead itself ran | Sessions |
| --- | --- |
| builds, tests or gate scripts | 102 |
| `git commit` | 101 |
| `git merge` | 88 |
| `git worktree add` | 82 |
| the Edit tool | 53 |

Most of those calls followed the skill text of the time, which told the lead
to run each check, merge the branches and run the full gate. The scan also
counted sessions in which the user asked the session itself to build
teamwire, so the numbers are an upper bound for drift.

A member has three things that hold it to its role: its seat's rules in its
system prompt, its `owns` list, and `teamwire verify`. The lead has its skill
and the Stop hook, which checks only for a missing watch and an AFK team
nothing will wake (ADR 0018 point 5). `verify` sees a lead's commit on a
member's branch only after the fact. In teamwire's own runs, it recorded 426
outside-owns flags in 21 teams, and those runs' closers traced them to the
lead's own merges and commits
(`docs/research/2026-10-18-lead-field-reports.md`, O9; nobody re-checked the
tracing).

## Decision

1. **A lead tool hook, `teamwire hook lead-tool --harness <id>`, runs after
   each of the lead's tool calls**, on every harness that can lead. The user
   decided on 2026-11-04 to treat every harness like Claude Code.
   - Claude Code: the plugin's `PostToolUse` hook, for the tools `Edit`,
     `Write`, `MultiEdit`, `NotebookEdit` and `Bash`.
   - Codex: the plugin's `PostToolUse` hook, with no tool filter, as for a
     member's hooks.
   - pi: the lead extension runs the hook when a call starts, and adds the
     note to that call's result.
2. **`Harness::parse_tool_call` turns the hook input into a neutral
   `ToolCall`**: the session, whether a subagent made the call, the folder,
   and what the call does (`Edit` with its paths, `Shell` with its command,
   or `Other`). The default is `Unsupported`, and the hook then says
   nothing.
3. **The note goes out when the session leads an open team and the call is
   a member's work.** That work is an edit to a file under a member's `repo`
   or `cwd` and outside `$TEAMWIRE_HOME`; a build or test run; a git command
   that commits, merges, rebases, cherry-picks, reverts, applies a patch,
   pulls, pushes or adds a worktree; or `gh pr create` or `merge`. The note
   names the team and tells the lead to reprompt the member that owns the
   work, or to brief a new one.
4. **The note never blocks the call.** It is the in-turn reply every
   harness's tool hook reads (ADR 0023 point 6). The user may ask the lead
   for a change directly, and the note ends with "If the user asked you to
   do it yourself, carry on."
5. **The hook writes nothing and notes every such call.** It keeps no
   record of earlier notes. It reads the call before it opens any store,
   and gives up after about 5 s, as the Stop hook does.

## Rejected

- **Refuse the first such call, then allow the retry.** A refusal needs a
  record of which calls were refused, so the hook would write to the store.
  The Stop hook writes nothing, and the tool hook runs far more often. A
  refusal also costs a turn each time the user did ask for the work.
- **Note only the first such call in a session.** It needs the same record.
  One note early in a long session leaves the rest of the session unwatched.
- **Change only the skill.** The skill change matters most, because the scan
  shows leads doing what the skill told them. But nothing would notice a
  lead that drifts later, such as one that edits code when the user asks it
  mid-run.
- **Claude Code only.** The user chose parity across harnesses.

## Consequences

- A lead doing a member's work sees a note beside each such call. A lead
  the user asked to do it carries on, and the note repeats.
- On Codex and pi the hook starts on every tool call of the lead's session,
  and on Claude Code on every edit and shell call. Each run is one short
  process that reads its input and, for most calls, stops there.
- The command check is a fixed list of tools and git commands, read word by
  word. A build tool missing from the list, or a command hidden in a script
  without `test`, `gate`, `build` or `check` in its name, gets no note.
- Codex's tool names come from the hook input teamwire's tests record, not
  from a live Codex lead. A patch counts as an edit wherever its text
  appears in the tool's input.
- pi keeps one pending note. When pi runs a turn's calls side by side, a
  note can land beside another call of the same turn.
- An installed plugin or pi package is out of date until `teamwire install`
  runs again, and `teamwire doctor` says so.
- Reading code stays the lead's own work, because the lead needs it to
  design the plan and write the briefs. The hook never notes a read.
