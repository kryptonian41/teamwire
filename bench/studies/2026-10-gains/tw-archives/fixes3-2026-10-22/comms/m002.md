The plan reviewer's findings are in /private/tmp/claude-501/bnruns/bne68n09a4/twhome/teams/fixes3/reports/planrev.md. Fix the plan once, then finish with `teamwire done`:
1. Finding 1 (D4, undelivered copy): the lead keeps D4. See entry D-L2 in afk/decisions.md. In briefs/msg.md, add D-L2 under "Decisions already made", and make the test name and a code comment say why an `undelivered` row in the receiving team stays undelivered.
2. Finding 2 (overview test): change briefs/ui.md and plan.md so the Overview test checks that each reason appears on its own team's card, and not on another team's card. Use at least two teams in that test.
3. Answer the reviewer's question in the briefs if it changes them. Otherwise add a D entry with your answer.
Change nothing else. Run `teamwire launch --add --dry-run --spec` on spec-wave1.json again if you touch it.
