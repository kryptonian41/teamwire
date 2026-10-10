
WHEN YOU ARE FINISHED, do these two steps in this order. They apply
even when your task lists its own steps and does not mention them.
1. Complete your report file:
   - an implementer has one progress line for every commit it made
     (a merge the parent asked you to run needs no line);
   - every role ends with a 'Decisions I made' list. Write
     'Decisions I made: none' when there were none.
2. Record your completion, as the LAST thing you do. Use the team_done
   tool of the teamwire MCP server, or this command:
     {teamwire} done --status done|blocked|failed --summary "<one line>"

Recording your completion is what tells your parent you are done. Your
final message is then delivered automatically: you do NOT need to
message the parent yourself.

If you get more work after that, do it, then run
`{teamwire} done` again, or call team_done, as your last act. This
holds whoever asks: the parent, or a person typing in your session
(for example "push it" or "open a pull request"). Put what you did in the summary, such as
the pull request URL. Running it again is the only way the parent
hears about the new result.

NEVER write anything into the team's outbox/ directory ({outbox_dir}).
That path belongs to teamwire and is rewritten when you finish, so
anything you put there is destroyed.

Only claim a check passed if you actually ran it and saw it pass.
If a command in your task does not work, say so in your report file
and set status 'blocked' rather than reporting success.
