# ADR 0060: Metrics show the money a harness recorded, and teamwire holds no price table

Status: accepted, 2026-11-20. Extends OBS-6 from tokens to money. ADR 0033
stands: the repository names no model.

## Context

A member of a set (ADR 0057) on a GPT or a Gemini model runs on pi
through OpenRouter (ADR 0053), and OpenRouter bills each token. `teamwire metrics`
showed tokens and no money, so a lead could not see what a set cost.

pi already writes the money of each reply into its session file. Each
assistant message line carries `message.usage.cost.total`, a float in US
dollars for that one reply. pi sets `cost.totalFromProvider` to true when
the provider billed the figure. Otherwise the figure is pi's own estimate
from its model catalog. pi records 0 for a model that a subscription pays
for, such as a Claude model through the Claude bridge.

Claude Code writes no money into its transcript.

teamwire could work money out from token counts and a table of prices.
That table would name models and prices, and both change often.

## Decision

1. **teamwire reads the money a harness recorded and never works money out
   from prices.** No price table exists in code, config or docs. That keeps
   ADR 0033's rule that the repository names no model.
2. **Money rides with the token counts.** `TokenCounts` gains
   `cost_micro_usd`, whole millionths of a US dollar. A float would lose the
   type's `Eq` and `Copy`. The pi reader rounds each reply's figure to the
   nearest millionth before it adds the figure, over the same lines whose
   tokens it counts. pi's `"type":"usage"` lines repeat a reply's figures
   and are skipped.
3. **The JSON keeps its old shape when there is no money.** The key is left
   out when it is 0, so a harness that records none writes the four token
   keys it wrote before. An event written before this change reads as no
   money.
4. **A new counter, `teamwire.member.cost`**, unit `usd`, with the labels
   `team`, `role`, `kind`, `model`, `effort` and `task_class`. Its value is
   dollars. Each `result.delivering` event counts what it adds to the
   previous event of the same session, as `teamwire.member.tokens` does.
5. **No money gives no line, and a total of 0 gives no line.** A missing
   line means "not recorded". teamwire never prints 0 for it, because a
   line of 0 dollars would read as "free" for a subscription model.
6. **`teamwire metrics` prints dollars to exactly 4 places**, through a
   format of its own. The shared format prints 3, which would show a reply
   of $0.0004 as 0.000.
7. **A billed figure and an estimate print as one line.** teamwire shows
   pi's figure either way, and `docs/metrics.md` says so.

## Consequences

- A Claude Code member gets no money line. Its cost stays unknown to
  teamwire until Claude Code records one.
- A team whose lead route is the watch records no tokens today, so it
  records no money either.
- The OpenTelemetry export carries the new counter with no extra work.
- When pi's figure is an estimate, the line can differ from the bill. The
  line says nothing about which kind it is.
