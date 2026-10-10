# Research: running a company of agents

Written 2026-09-30. This is research, not a decision. Nothing here is built,
and no ADR accepts it. It exists so the user can decide whether and how to
build it.

## The question

The user asked for three things under teamwire:

1. **One consistent config for a company of agents**, with a hierarchy:
   company, departments, teams, employees.
2. **A lasting record of every message** between agents that the hierarchy
   allows to talk to each other.
3. **A complete state record** of the company, down to each department, team
   and employee: todo lists, items waiting on a human, and any other state that
   helps someone see where things stand.

Four agents researched it in parallel on 2026-09-30. One read teamwire's own
code, and the other three read outside sources on config, messaging and
state. A fifth agent then checked 26 code citations and 16 web pages from
their reports, found where the reports contradicted each other, and wrote one
combined sketch. The lead re-checked the checker's key code claims. The full
reports are archived with the team `twco` (see "Where the full reports are").

The user settled three things before the research started:

- Compare both employee models: a long-lived employee that keeps memory
  across tasks, and a per-task seat filled by a fresh session each time.
- Treat "a company layer above today's teams" against "a redesign where a
  team is one level of a tree" as an open question.
- Produce findings plus a concrete sketch, with no code.

## The answer

1. **Build the company as a layer above today's teams.** Leave each team's
   database as it is. All four reports point this way or fit it.
2. **The deciding fact is SQLite's rule on two files.** A commit that spans two
   database files in WAL mode is not atomic: "some of those files might get the
   changes where others might not" ([sqlite.org/lang_attach.html]). So the
   company record must be built from the team records afterwards, never
   written in the same transaction.
3. **teamwire already reads many teams at once.** `teamwire status` with no
   team name opens every team's database read-only
   (`crates/teamwire-cli/src/cmd/status.rs:248`). A company roll-up extends a
   pattern that already works.
4. **A redesign would cut through three fixed points.** Those are the single
   lead per team (`crates/teamwire-store/src/schema/v1.sql:13-18`), the
   one-row `team` table (`v1.sql:8`), and the 32-character agent name
   `tw_<team>_<role>` (`crates/teamwire-core/src/names.rs:62-77`). No report
   found a need that only a redesign meets. The one exception: if a message
   between two teams must be all-or-nothing, only a shared database gives that.
5. **For employees, use a mix of both models.** Keep today's per-task seat: a
   fresh session for each task. Add an employee record that outlives the
   session, which the next session reads when it starts. teamwire already
   counts the sessions in one seat (`members.generation`, `v1.sql:43`). Claude
   Code's `/resume` does not bring teammates back ([agent-teams]). Temporal has
   to restart any history before it reaches 51,200 events ([temporal-event]).
   LangGraph keeps per-run state apart from long-term memory ([lg-persistence]).
   All four point away from one endless session per employee.
6. **The gap no existing system fills is a checker for the org file.** No
   system found refuses a bad org config before anything runs, such as a boss
   that does not exist or a reporting loop. Paperclip, the closest product,
   still lists "strict tree or multiple managers" as an open question in its
   spec ([paperclip-spec]).

## Decisions made after the research (2026-10-05)

The user answered two of the open questions. The sketch below has been
updated to match. Where a decision leaves a new gap, the gap is listed under
"Open questions for the user".

1. **The org chart is a reporting view only.** `reports_to` says who a seat
   reports to and whose work it rolls up into. It does not decide who may
   message whom. Paperclip makes the same split: its org chart "defines
   reporting and delegation lines, not access control" ([paperclip-spec]).
   So the rule for messages between teams needs a source other than the chart.
   That source is not decided yet (open question 12).
2. **A seat may report to two managers.** `reports_to` becomes a list. The
   reporting lines form a graph with no loops, not a tree. Every path still
   ends at `human`. Who acts when two managers disagree, or when an escalation
   could go to either, is not decided yet (open question 13).

## Idea 1: one config for the company

What teamwire has today:

- A team spec with a team block (`crates/teamwire-core/src/spec.rs:15-26`) and
  a member block (`spec.rs:28-65`). There is no parent, department or budget
  field.
- Inheritance is one short chain. The harness comes from the member, else the
  team, else the lead's harness. The host comes from the team, else the lead's
  host. Members have no host field (`crates/teamwire-launch/src/plan.rs:147-152`).
- One config file already exists. `$TEAMWIRE_HOME/config.toml` holds the
  OpenTelemetry settings (`crates/teamwire-otel/src/config.rs:17`). A company
  file needs a different name. The `toml` crate is already a dependency, so a
  TOML company file adds none.

What the outside sources show:

| Finding | Source | Checked |
| --- | --- | --- |
| Claude Code subagent files have no reports-to or budget field. A file with the same name in a higher location replaces the whole definition; nothing merges | [sub-agents] | yes |
| Claude Code's agent-teams config is runtime state. It must not be edited by hand, and "there is no project-level equivalent of the team config" | [agent-teams] | yes |
| In CUE, a child can only narrow its parent's constraints: "an instance may never violate the constraints of its parent". This is the model for a department budget that a team may only lower | [cue] | yes |
| Paperclip passes budgets down from each manager to its reports. Its spec says the org chart "defines reporting and delegation lines, not access control" | [paperclip-spec] | yes |
| AutoGen keeps config (a blueprint "stamped out many times") apart from state (message history) | [autogen-config] | yes |
| CrewAI only says a task's agent "should match" a defined agent. CODEOWNERS skips a bad line and carries on. Neither refuses a broken reference | [crewai-tasks] | yes |
| A config change reaches Claude Code agents at the next spawn. It does not reach a running one | [sub-agents], [agent-teams] | yes |

Not confirmed: MetaGPT's role config (its docs show only `name` and
`profile`), LangGraph's hierarchical-teams tutorial (the page redirects to
nothing), and ChatDev 2.0's YAML fields.

## Idea 2: a lasting message record governed by the hierarchy

What teamwire has today:

- A per-team record already exists: `messages` holds one row per message and
  `deliveries` holds one row per recipient (`v1.sql:50-81`). Rows are only
  added, never edited, apart from the delivery state. Close archives the whole
  database (`crates/teamwire-close/src/archive.rs:1-20`).
- Who may send what is decided by the member's kind
  (`crates/teamwire-msg/src/rules.rs:141-162`). A rule based on the hierarchy
  fits as one more check where recipients are checked (`rules.rs:175-219`).
- Two guards against message floods already exist. One sender may send at most
  5 messages a minute to one recipient. An identical body to the same recipient
  within 10 minutes is refused (`rules.rs:16-20`).

What the outside sources show:

| Finding | Source | Checked |
| --- | --- | --- |
| A central check on results contains errors: they spread 4.4 times with one, and 17.2 times with none. Multi-agent setups lost 39-70% on sequential work. This backs a central check on results, not a rule about who relays messages | [google-scaling] | yes |
| Claude Code agent teams let any teammate message any other. The mailboxes live in the team directory, which is removed when the session ends | [agent-teams] | yes |
| Claude Code's cross-session messages hold at most 100 messages and queue at most 50. Loops between sessions are slowed down | [cross-session] | yes |
| CrewAI stops delegation loops by making specialists unable to delegate (`allow_delegation=False`) under a clear hierarchy | [crewai-collab] | yes |
| In Erlang/OTP, a supervisor that sees more than MaxR restarts in MaxT seconds stops all its children and then itself. The failure then goes one level up | [erlang-sup] | yes |
| MetaGPT's shared message pool, where roles subscribe to message types, reports fewer revisions and fewer tokens per line of code | [metagpt] | no |
| Communication topology changes results (MacNet); pruning messages saves 28-73% of tokens (AgentPrune) | [macnet], [agentprune] | no |

## Idea 3: one state record for the company

What teamwire has today: per-team tables for members, messages, deliveries,
completions, verify records and events (`v1.sql`). There is no todo table, no
queue of items waiting on a human, and no cost column. teamwire has no data on
tokens or money at all.

What the outside sources show:

| Finding | Source | Checked |
| --- | --- | --- |
| A LangGraph interrupt saves state and "waits indefinitely". It resumes with `Command(resume=...)` on the same thread id. There is no timeout | [lg-interrupts] | yes |
| Paperclip's spec defines approvals but no approval timeout. Its budgets have a hard ceiling that pauses the agent. The README says "overspend pauses agents and cancels queued work automatically" | [paperclip-spec], [paperclip] | yes |
| Claude Code's shared task list has three states, and its docs warn that "task status can lag" | [agent-teams] | yes |
| Temporal keeps a complete event log and rebuilds a workflow by replaying it. A run ends at 51,200 events or 10,000 signals, and must be restarted as a new run | [temporal-history], [temporal-event] | the limits yes; the replay claim no |
| teamwire already reads its own event table with a saved read position, for the metrics export (`metric_export`, `v1.sql:167-173`). A company view can read team events the same way | code | yes |

What follows from these:

- The company view is built from each team's `events` table, with a saved read
  position per team. It is only as fresh as the last `status` or `watch` run.
- An item waiting on a human is its own record with a due time. None of the
  systems studied sets a timeout, so teamwire has to choose the policy.
- An agent's own progress notes go out of date. Show measured facts beside
  them: last event, last commit, last delivery.
- Every budget field depends on the harness reporting token counts. No report
  checked whether Claude Code exposes them.

## Combined sketch

Nothing here is code. All names are placeholders. Where the four reports
disagreed, the choice made here is noted.

### The company config

Path: `$TEAMWIRE_HOME/companies/<co>/company.toml`. The shape is flat tables
joined by name, so the checker has named references to check.

```toml
schema = 1

[company]
name = "acme"
human = "apoorv"               # end of every reporting path; never a message recipient
budget_usd_month = 500

[defaults]                     # everything below inherits these
harness = "claude-code"
host = "herdr"
model = "sonnet"
seat = "per-task"              # or "long-lived"
deadline_s = 3600
human_item_ttl = "48h"
human_item_on_timeout = "escalate"   # expire | escalate | wait

[dept.eng]
head = "cto"                   # a seat name
budget_usd_month = 300         # may only be lower than the company's
model = "opus"                 # override: the whole department runs opus

[dept.ops]
head = "opshead"
budget_usd_month = 100

[team.engcore]
dept = "eng"
lead = "cto"                   # the seat whose session is this team's teamwire lead
[team.engqa]
dept = "eng"
lead = "qalead"
[team.opsrun]
dept = "ops"
lead = "opshead"

[seat.cto]
team = "engcore"
kind = "lead"
reports_to = ["human"]
seat = "long-lived"
[seat.dev1]
team = "engcore"
kind = "implementer"
reports_to = ["cto"]
[seat.dev2]
team = "engcore"
kind = "implementer"
reports_to = ["cto", "qalead"]  # two managers: a reporting view only
model = "sonnet"               # one seat overrides its department
[seat.qalead]
team = "engqa"
kind = "lead"
reports_to = ["cto"]
[seat.qa1]
team = "engqa"
kind = "reviewer"
reports_to = ["qalead"]
[seat.opshead]
team = "opsrun"
kind = "lead"
reports_to = ["human"]
[seat.oncall]
team = "opsrun"
kind = "implementer"
reports_to = ["opshead"]

[channel.apischema]            # the one way across a team line without a manager
owner = "cto"
members = ["dev1", "qa1"]
types = ["notice", "finding", "question", "answer"]
expires = "2026-10-31"
```

In teamwire, a team's lead is the session that launched the team. The lead is
not a row in `members`, and the role name `lead` is reserved
(`names.rs:56-58`). So a seat with `kind = "lead"` becomes that team's lead
session, and it is left out of the generated member list.

A long-lived employee's files cannot live in the team directory, because close
removes it. They live at `companies/<co>/employees/<seat>/`, a fixed path the
config does not need to name.

What the checker refuses before anything starts:

1. An unknown key, or a value of the wrong type.
2. A `reports_to`, `head`, `lead`, `dept` or `team` that names nothing.
3. A reporting loop. `reports_to` is a list, so the lines form a graph, not a
   tree. The graph must have no loops, and every path must end at `human`.
4. A child budget above its parent's. A child may narrow a limit, never widen it.
5. A team `lead` that is not a `kind = "lead"` seat in that team, or a
   department `head` that is not a lead seat in that department.
6. A lead seat with a manager in `reports_to` that is not another lead seat
   or `human`. This keeps the managers of team leads made of team leads.
7. A team name outside `^[a-z][a-z0-9]{0,11}$`, a seat name outside
   `^[a-z][a-z0-9_]{0,15}$`, or a seat name used twice. These limits make
   `tw_<team>_<seat>` always a valid address.
8. A channel whose owner or members are not seats, whose types are not message
   types, or whose expiry has passed.

### Who may message whom

The org chart is a reporting view only (decided 2026-10-05). So the rule is
computed from `team`, `kind` and `channel`, and never from `reports_to`.
Today's rules by member kind stay as a second filter.

One row is still open. The research tied messages between two team leads to
the reporting lines. That row needs a new source, such as a channel, a rule
that any lead may reach any lead, or a list in the config (open question 12).

| Sender | Recipient | Allowed | How it travels |
| --- | --- | --- | --- |
| Member | Its own team's lead, or a member of its team | As today: the kind rules decide | One write to the team's file, unchanged |
| Team lead | Its own members | As today: `answer`, `reprompt`, `note` | Unchanged |
| Team lead | Another team's lead | **Open.** The research allowed it along reporting lines; that basis is gone | Relayed by the company layer |
| Member | Anyone outside its team | No. It asks its own lead, who forwards it | The hop count goes up by one |
| Channel member | Another member of the same channel | Yes, only the channel's types, until it expires | Relayed by the company layer |
| Anyone | The human | Only as a human item, never as a chat message | A `human_item` record |
| Reviewer or closer | Anyone | No, as today | Unchanged |
| Anyone | Anyone, after more than 3 hops | No. The thread goes to the sender's lead as one escalation | Refused |

### Where records live

Each team's `team.db` stays the record for that team. It changes in a new
schema version:

| Table | Change |
| --- | --- |
| `messages` | Add `thread` (the first message's id), `hop`, `via`, `rule` (`up`, `down`, `peer`, `relay`, `channel:<name>`) and `sender_pos` (a path such as `acme/eng/engcore/dev1`) |
| `todos` (new) | `ulid`, `role`, `generation`, `text`, `state`, `updated_at` |
| `events` | New types: `cost.recorded`, `todo.changed`, `human.asked`, `relay.received` |
| `team` | Add `company` and `department` |

A new `companies/<co>/company.db` holds copies, pointers, and records that
belong to no single team. Only the company layer writes it, one `Store::write`
per change.

| Table | Key fields | Written by |
| --- | --- | --- |
| `company`, `department` | name, budget cap, period | the checker, when a person applies the config |
| `team_ref` | team id, name, department, `db_path`, `events_pos` | the company layer, as it reads each team's events |
| `message_index` | team, `message_ulid` (unique), thread, `sender_pos`, recipients, `created_at` | the company layer, copied from team files. It answers "every message this employee saw" across teams |
| `relay` | id, `origin_team`, `origin_message_ulid` (unique), `target_team`, `target_role`, state (`pending`, `delivered`), `delivered_message_ulid` | the company layer |
| `human_item` | id, `origin_team`, `origin_role`, `origin_message_ulid` (unique), kind (`question`, `approval`), body, state (`open`, `answered`, `expired`, `withdrawn`), `due_at`, `on_timeout`, answer, `answered_by`, `answered_at` | an agent raises it, a person answers it, the clock expires it |
| `employee` | seat, `history_summary`, `todo`, `cost_total`, `last_event_at` | the agent writes the todo and the summary; teamwire measures cost and the last event |

How a relayed message or a human item moves, with no daemon and no
transaction that spans two files:

1. The sender writes to its own team's file in one transaction, as today.
2. The next `teamwire status` or `watch` run reads the new events. In one
   `company.db` transaction it inserts the `relay` or `human_item` row and
   moves the read position forward. The unique `origin_message_ulid` means a
   repeated run inserts nothing twice.
3. A second, separate transaction writes the delivered message into the
   target team's file. The layer then marks the relay `delivered`. If a run
   crashes between the two, the next run finds the `pending` relay and
   finishes it.

### What teamwire would have to change

- A way for a lead or a member to raise a human item. Today a lead may not
  send a `question` (`rules.rs:152-155`), and `human` is not a valid recipient
  (`names.rs:283-295`).
- A new sender for relayed and human-written messages. Today an `answer` must
  come from someone the question was asked of (`rules.rs:233-236`), and the
  lead is refused when it messages itself (`rules.rs:183-185`).
- Token or cost counts from the harness, before any budget field means
  anything.

These are changes to `teamwire-core` and `teamwire-msg`, plus a new schema
version. No existing table loses a column.

## Where the reports disagreed, and what the sketch chose

| Disagreement | Choice | Why |
| --- | --- | --- |
| The messaging report put original cross-team messages in a company file | Messages stay in team files; the company file holds an index and relay rows | Writing both files in one send is not atomic in SQLite. That would break message integrity, teamwire's second priority |
| Three config shapes: nested tables, flat tables joined by name, arrays of tables | Flat tables | The checker needs named references to check |
| Budget per day against per month | Per month, one period | Either works; teamwire has no cost data yet |
| The config report made a team lead an ordinary seat in the team | `kind = "lead"` marks the seat whose session leads the team | teamwire's lead is the launching session, not a member row |
| Where the person sits: top of every chain, open, or a message recipient | At the end of every chain, reached only through human items | teamwire has no `human` recipient, and a chat message to a person has nobody to deliver it |
| A memory path in the config against an `employee` row | An `employee` row, plus a fixed directory | Close removes the team directory |

## Idea 4: fixed workflow steps

Added 2026-10-05. The user asked whether fixed workflow steps would make the
flow of work solid. One researcher studied it (team `twflow`), and the lead
checked its key claims. The idea: code decides which step comes next and what
must be true to leave a step. The agent does the work inside the step. An
agent saying "done" is only an input; a program decides whether the step is
passed.

### What the sources show

| Finding | Source | Checked |
| --- | --- | --- |
| Anthropic names the split: "Workflows are systems where LLMs and tools are orchestrated through predefined code paths." Agents "dynamically direct their own processes". Prompt chaining can add "programmatic checks ... on any intermediate steps" | [anthropic-agents] | yes |
| Stripe runs this in production. "Blueprints are workflows defined in code that direct a minion run." Some steps "don't invoke an LLM at all—they just run code". "After the second push and CI run, we send the branch back to its human operator." Stripe says over 1,300 merged pull requests a week are agent-made. That number is from a company blog | [stripe-minions] | yes |
| AWS Step Functions has the most complete plain step format. A step has `Retry` (`MaxAttempts` 3, `IntervalSeconds` 1, `BackoffRate` 2.0 by default), `Catch` (a named next step per error) and `TimeoutSeconds`. Retries run first, then the matching catch. An error with neither fails the whole run | [sfn-errors] | yes |
| Claude Code hooks can refuse to let an agent stop. Exit code 2 from `Stop` "prevents Claude from stopping"; from `TeammateIdle` it keeps the teammate working; from `TaskCompleted` it "prevents the task from being marked as completed". The hooks page does not mention a loop guard such as `stop_hook_active`, so teamwire must count its own refusals | [cc-hooks] | yes |
| Temporal keeps the flow code apart from the work. Workflow code must make "the same Workflow API calls in the same sequence, given the same input". Calls to models and other programs go in activities, which are retried | [temporal-def] | no |
| CrewAI Flows wrap agent crews in code: `@start`, `@listen`, and `@router` for "conditional routing logic". Its `@human_feedback` step has an LLM read the person's words, which a program cannot check | [crewai-flows] | no |
| LangGraph routes with a function ("conditional edges"), and stops a run at a step limit of 1000 by default | [lg-graph] | no |
| MetaGPT writes its procedures into prompts, not into a checker. Its preprint reports 0.83 human revisions per task against ChatDev's 2.5, on the authors' own benchmark | [metagpt] | no |
| No measured study compares a fixed flow with a program-checked exit against a free agent loop. AFlow (preprint) compares workflow methods with each other. MAST (preprint) sorts 1600+ failure traces into 14 modes, one group of which is weak checking of results | [aflow], [mast] | no |

teamwire already has one fixed flow. A member's completion is stored, then
`teamwire verify` runs the member's `check` command and records the real exit
code, with a timeout recorded as exit 124
(`crates/teamwire-verify/src/check.rs:1-3`). The Python tooling holds delivery
once when a report is incomplete. It holds only once, so a child can never
loop (`~/.claude/skills/agent-team/SKILL.md:765-771`). With the hold in place,
two out of two children wrote the report. When only the prompt asked for it,
three out of three skipped it.

**What the evidence supports.** A step should end on a command whose exit code
a program reads. The evidence does not support long rigid chains of steps.
Every handoff between agents is a place for errors. Google measured
multi-agent setups losing 39-70% on sequential work ([google-scaling]).

### Sketch: a workflow in the company config

A team picks a workflow by name (`workflow = "ship"` in its `[team.*]` table).
A team may only narrow an inherited workflow: lower a limit, add a step, or
add a human approval. It may never remove one. The approvers are named in the
step itself, because the org chart is a reporting view only.

```toml
[workflow.ship]
start = "plan"
max_total_s = 28800              # whole run; on timeout the run goes to needs_human

[workflow.ship.step.plan]
kind = "agent"                   # an agent works; a program checks the exit
role = "planner"
exit_check = "file_exists:plan.md and sections:Goal,Steps,Risks"   # form only
retries = 1
timeout_s = 1800
on_pass = "implement"
on_fail = "needs_human"

[workflow.ship.step.implement]
kind = "agent"
role = "implementer"
exit_check = "completion:done"   # the agent finished and the commit exists
timeout_s = 7200
on_pass = "check"
on_fail = "needs_human"

[workflow.ship.step.check]
kind = "command"                 # no agent in this step
run = "scripts/gate.sh fast"
exit_check = "exit_code == 0"
timeout_s = 900                  # a timeout is exit 124
max_fix_rounds = 2               # fail, send back to implement, check again; twice
fix_step = "implement"
on_pass = "review"
on_fail = "needs_human"

[workflow.ship.step.review]
kind = "agent"
role = "reviewer"
exit_check = "completion:done and review_record_exists"
timeout_s = 3600
on_pass = "approve"
on_fail = "implement"
loop_limit = 2                   # review may send the work back at most twice

[workflow.ship.step.approve]
kind = "human"                   # only a person's command leaves this step
approvers = ["cto", "qalead"]    # any one of them; "all" is an option
due = "48h"
on_timeout = "escalate"
on_pass = "merge"
on_fail = "implement"

[workflow.ship.step.merge]
kind = "command"
run = "scripts/merge.sh"         # not read-only, so it must be safe to run twice
exit_check = "exit_code == 0"
timeout_s = 600
on_pass = "done"
on_fail = "needs_human"

[workflow.ship.step.needs_human]
kind = "human"
approvers = ["cto"]
due = "24h"
on_timeout = "wait"
actions = ["retry_step", "skip_step_with_reason", "abort_run"]   # the only ways out
```

An exit check is one of a short list of named tests: `exit_code == N`,
`file_exists:<path>`, `sections:<names>`, `completion:<status>`,
`review_record_exists`. The checker refuses any other string. The checker also
refuses a workflow where:

- an `on_pass`, `on_fail` or `fix_step` names no step,
- a cycle has no `loop_limit`,
- a command step is neither read-only nor marked safe to run twice,
- a team's copy removes a human step or raises a limit set above it.

Steps such as "the plan is good" have no exit code. Their check can only test
form: a file exists, a section is present. The judgement stays with an agent
or a person.

### Sketch: records in each team's `team.db`

A new schema version adds three tables. The run keeps a copy of the workflow
definition, so editing the config never changes a run already in progress.

| Table | Key fields |
| --- | --- |
| `workflow_run` | `ulid`, workflow name, `definition_json`, `definition_sha256`, `state` (`running`, `waiting_human`, `passed`, `failed`, `aborted`), `current_step`, `version` (raised on every move), `started_at`, `ended_at`, `deadline_at` |
| `step_run` | `run_ulid`, `step`, `attempt` (unique together), `state` (`pending`, `working`, `checking`, `passed`, `failed`, `timed_out`, `waiting_human`, `skipped`), `role`, `generation`, `completion_ulid`, `claimed_by`, `claim_expires_at`, `exit_code`, `check_log_sha256`, `started_at`, `deadline_at`, `ended_at`, `reason` |
| `step_decision` | `ulid`, `run_ulid`, `step`, `decided_by`, `action` (`approve`, `reject`, `retry_step`, `skip_step`, `abort_run`), `reason` (required for a skip), `created_at`. Rows are never edited |

New event types start with `flow.`: `started`, `step_started`,
`step_claimed`, `check_recorded`, `step_passed`, `step_failed`,
`step_timed_out`, `step_retried`, `waiting_human`, `decided`, `step_skipped`,
`passed`, `failed`, `aborted`. The company layer already reads each team's
events, so `flow.waiting_human` feeds the company's list of items waiting on
a human with no extra work.

### How a workflow moves with no daemon

One command, `teamwire flow advance <team>`, moves a workflow. Running it twice
changes nothing. Each call reads the rows, picks at most one move, and commits
it with `WHERE version = ?`. Three programs call it:

- the `done` command or the Stop hook, after an agent finishes a turn,
- a person's `teamwire flow approve` command,
- every `watch` poll, which catches timeouts and steps that nobody moved.

| Failure | Answer |
| --- | --- |
| A crash between two steps | Marking the old step `passed` and creating the next step happen in one `Store::write`. A crash leaves the old state or the new one |
| The process dies while a check runs | The step was claimed with an expiry before the check started. After the expiry, the next `advance` runs the check again. That is safe only for a command step that is read-only or safe to repeat |
| The agent never finishes | `watch` sees `deadline_at` pass and records `timed_out`. This works only while someone runs `watch` |
| Two commands move the same step | `BEGIN IMMEDIATE` puts one first. The second finds the version changed and stops with "already moved" |
| A long check would hold the database lock | The check never runs inside a transaction. One write claims the step, the check runs, and a second write records the result (ADR 0002, decision 2) |

### Problems the lead found in the sketch

1. **A long check cannot run inside the Stop hook.** The researcher's example
   runs a 15-minute check from the Stop hook. That holds the agent's turn open
   for 15 minutes, and teamwire's rule is that a hook never fails the agent's
   turn. The hook should only record that the agent finished. A separate
   caller has to run the check: the next `watch` poll, or a command the lead
   runs. Which one is open question 17.
2. **teamwire cannot yet tell which person is approving.** The approve command
   must refuse anyone not on the approver list. teamwire proves which agent
   sent a message with a token (ADR 0008), but it has no way to identify a
   person at the command line.
3. **The merge step changes things, so a crash can run it twice.** The rule
   "a command step must be read-only" cannot hold for a merge. The sketch marks
   it "safe to run twice". A merge script has to make that true, for example by
   stopping with success when the branch is already merged.

### A run from start to finish

The workflow is `ship`, the team is `engcore`, and the task is "add retry to
the sender".

| # | What happens | Moved by |
| --- | --- | --- |
| 1 | `teamwire flow start engcore ship` creates the run and a `plan` step | a person or the lead |
| 2 | The planner works and runs `done`. The program checks that `plan.md` has its three sections, and moves to `implement` | the Stop hook calls `advance` |
| 3 | The implementer works and runs `done`. The step passes, and `check` becomes pending | the Stop hook |
| 4 | The check is claimed, runs, and exits 101. This is fix round 1 of 2. The implementer gets the failure log path and starts again | the next `watch` poll (see problem 1) |
| 5 | The implementer fixes it and runs `done`. The check runs again and exits 0 | Stop hook, then `watch` |
| 6 | The reviewer works, runs `done`, and its review record exists. The step passes | the Stop hook |
| 7 | `approve` waits with a 48-hour due time. The run shows as waiting on a human, and the company view lists it | `advance` |
| 8 | The person runs `teamwire flow approve engcore <run> --reason "ok"`. A `step_decision` row is written | the person |
| 9 | `merge` runs and exits 0. The run is `passed` | the same command |

Had the check failed a second fix round at step 5, the run would go to
`needs_human` instead.

## Not covered by this research

- **Span of control.** No source gave a measured limit on how many reports one
  agent manager can handle.
- **Whether Claude Code reports token counts** that a budget could use.

## Claims not yet checked

The checker did not re-check these. Each rests on one researcher's fetch or
search summary:

- MetaGPT's measured numbers, MacNet's per-topology scores, AgentPrune's
  28-73%, A2A's field list, and the Conway quote.
- OpenHands issues #4532, #5092 and #4893, the MAST failure percentages,
  Linear's sub-issue rules, and Fowler's event-sourcing page.
- Kustomize, Terraform modules, CrewAI's agent fields, OpenAI Agents SDK
  handoffs, Magentic-One's ledgers, and ChatDev 2.0's YAML directory.
- "No framework checks an org config for cycles before running." This is a
  negative. Nothing contradicted it, but nobody searched for a counter-example.

## Open questions for the user

1. Should this be built at all, and does it belong in teamwire or in a
   separate tool?
2. What are the spending limits, and which actions must always wait for a
   person?
3. **Answered 2026-10-05:** a seat may report to two managers.
4. May a team override a department setting freely, or only narrow it? The
   sketch assumes narrow only.
5. **Answered 2026-10-05:** the org chart is a reporting view only. It is
   not a permission boundary.
6. Is the person a node in the tree, or outside it and reached only through
   human items?
7. Must a message between two teams be all-or-nothing? If yes, a layer above
   is not enough, and only the redesign with a shared database gives it.
8. Should a department head also be a working member of the level above?
9. When a human item times out, should it expire with a safe default,
   escalate, or wait forever?
10. Which employees should be long-lived, how long may they keep memory, and
    who decides what is forgotten?
11. Is a company view that is only as fresh as the last `status` run good
    enough? A live view needs a process running all the time, which ADR 0001
    rules out today.
12. Now that the org chart does not decide who may talk, what lets two teams'
    leads message each other: named channels only, any lead to any lead, or a
    list in the config?
13. When a seat has two managers, who answers its escalations and its timed-out
    human items: both, the first listed, or a primary manager named in the
    config?
14. Do department budgets still cap the teams in them? The decision covered
    reporting lines. Departments and teams also group seats for defaults and
    budgets, and that grouping may or may not count as the org chart.
15. Should fixed workflows be built, and in which version of teamwire? Which
    steps must always wait for a person?
16. Is it acceptable that a step's timeout only fires while `watch` runs? The
    other way is a timer outside teamwire, and ADR 0001 rules out a daemon.
17. Who runs a long check after an agent finishes: the next `watch` poll, or a
    command the lead runs? The Stop hook cannot.
18. How does teamwire know which person is approving a step?
19. May a person skip a step, given a reason that is recorded like a waiver?
20. When a check fails, does the agent get the log path, the log text, or only
    the exit code? This changes how much it can fix.
21. Does each agent step start a fresh member session, or reprompt a member
    already running in that seat?

## Where the full reports are

The team's five reports, briefs and message log are archived at
`~/.claude/team-archive/twco-2026-09-30/`. The combined report there (`reports/closer.md`) lists every
checked citation and every graded finding.

The workflow research from 2026-10-05 is archived at
`~/.claude/team-archive/twflow-2026-10-05/` (`reports/flows.md`).

## Sources

"Checked" means the checker opened the page during the research and found the
claim there.

| Key | URL | Checked |
| --- | --- | --- |
| sqlite.org/lang_attach.html | https://www.sqlite.org/lang_attach.html | yes |
| sub-agents | https://code.claude.com/docs/en/sub-agents | yes |
| agent-teams | https://code.claude.com/docs/en/agent-teams | yes |
| cross-session | https://code.claude.com/docs/en/cross-session-messaging | yes |
| cue | https://cuelang.org/docs/concept/the-logic-of-cue/ | yes |
| paperclip | https://github.com/paperclipai/paperclip | yes |
| paperclip-spec | https://raw.githubusercontent.com/paperclipai/paperclip/master/doc/SPEC.md | yes |
| autogen-config | https://microsoft.github.io/autogen/stable/user-guide/core-user-guide/framework/component-config.html | yes |
| crewai-tasks | https://docs.crewai.com/concepts/tasks | yes |
| crewai-collab | https://docs.crewai.com/concepts/collaboration | yes |
| google-scaling | https://research.google/blog/towards-a-science-of-scaling-agent-systems-when-and-why-agent-systems-work/ (2026-01-28) | yes |
| erlang-sup | https://www.erlang.org/doc/system/sup_princ.html | yes |
| lg-interrupts | https://docs.langchain.com/oss/python/langgraph/interrupts | yes |
| lg-persistence | https://docs.langchain.com/oss/python/langgraph/persistence | no |
| temporal-history | https://docs.temporal.io/encyclopedia/event-history | no |
| temporal-event | https://docs.temporal.io/workflow-execution/event | yes |
| metagpt | https://arxiv.org/html/2308.00352 | no |
| macnet | https://arxiv.org/abs/2406.07155 | no |
| agentprune | https://arxiv.org/abs/2410.02506 | no |
| chatdev | https://arxiv.org/abs/2307.07924, https://github.com/OpenBMB/ChatDev | no |
| a2a | https://a2a-protocol.org/latest/specification/ | no |
| magentic-one | https://microsoft.github.io/autogen/stable/user-guide/agentchat-user-guide/magentic-one.html | no |
| openai-handoffs | https://openai.github.io/openai-agents-python/handoffs/ | no |
| kustomize | https://kubectl.docs.kubernetes.io/references/kustomize/glossary/ | no |
| terraform | https://developer.hashicorp.com/terraform/language/modules/develop | no |
| fowler | https://martinfowler.com/eaaDev/EventSourcing.html | no |
| linear | https://linear.app/docs/parent-and-sub-issues | no |
| conway | https://www.melconway.com/Home/Committees_Paper.html | no |
| anthropic-agents | https://www.anthropic.com/engineering/building-effective-agents | yes (lead, 2026-10-05) |
| stripe-minions | https://stripe.dev/blog/minions-stripes-one-shot-end-to-end-coding-agents-part-2 | yes (lead, 2026-10-05) |
| sfn-errors | https://docs.aws.amazon.com/step-functions/latest/dg/concepts-error-handling.html | yes (lead, 2026-10-05) |
| cc-hooks | https://code.claude.com/docs/en/hooks | yes (lead, 2026-10-05) |
| temporal-def | https://docs.temporal.io/workflow-definition | no |
| crewai-flows | https://docs.crewai.com/concepts/flows | no |
| lg-graph | https://docs.langchain.com/oss/python/langgraph/graph-api | no |
| aflow | https://arxiv.org/abs/2410.10762 | no |
| mast | https://arxiv.org/abs/2503.13657 | no |

[sqlite.org/lang_attach.html]: https://www.sqlite.org/lang_attach.html
[sub-agents]: https://code.claude.com/docs/en/sub-agents
[agent-teams]: https://code.claude.com/docs/en/agent-teams
[cross-session]: https://code.claude.com/docs/en/cross-session-messaging
[cue]: https://cuelang.org/docs/concept/the-logic-of-cue/
[paperclip]: https://github.com/paperclipai/paperclip
[paperclip-spec]: https://raw.githubusercontent.com/paperclipai/paperclip/master/doc/SPEC.md
[autogen-config]: https://microsoft.github.io/autogen/stable/user-guide/core-user-guide/framework/component-config.html
[crewai-tasks]: https://docs.crewai.com/concepts/tasks
[crewai-collab]: https://docs.crewai.com/concepts/collaboration
[google-scaling]: https://research.google/blog/towards-a-science-of-scaling-agent-systems-when-and-why-agent-systems-work/
[erlang-sup]: https://www.erlang.org/doc/system/sup_princ.html
[lg-interrupts]: https://docs.langchain.com/oss/python/langgraph/interrupts
[lg-persistence]: https://docs.langchain.com/oss/python/langgraph/persistence
[temporal-history]: https://docs.temporal.io/encyclopedia/event-history
[temporal-event]: https://docs.temporal.io/workflow-execution/event
[metagpt]: https://arxiv.org/html/2308.00352
[macnet]: https://arxiv.org/abs/2406.07155
[agentprune]: https://arxiv.org/abs/2410.02506
[anthropic-agents]: https://www.anthropic.com/engineering/building-effective-agents
[stripe-minions]: https://stripe.dev/blog/minions-stripes-one-shot-end-to-end-coding-agents-part-2
[sfn-errors]: https://docs.aws.amazon.com/step-functions/latest/dg/concepts-error-handling.html
[cc-hooks]: https://code.claude.com/docs/en/hooks
[temporal-def]: https://docs.temporal.io/workflow-definition
[crewai-flows]: https://docs.crewai.com/concepts/flows
[lg-graph]: https://docs.langchain.com/oss/python/langgraph/graph-api
[aflow]: https://arxiv.org/abs/2410.10762
[mast]: https://arxiv.org/abs/2503.13657
