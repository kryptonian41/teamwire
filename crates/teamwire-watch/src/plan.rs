//! What one poll decides, without touching the store: which events to
//! announce, which results to move, and whether the team is complete. The
//! decisions are applied afterwards in one write transaction (`apply.rs`).

use std::collections::HashSet;
use std::time::Duration;

use teamwire_core::{
    CompletionStatus, DeliveryErrorCode, DeliveryState, LeadRoute, MemberState, ReportState, Role,
    Timestamp, Ulid,
};
use teamwire_store::{CompletionRecord, Ctx, Party};

use crate::memory::{MemberMemory, WatchMemory, slept_since};
use crate::observe::{Activity, Listings, Seen, observe};
use crate::snapshot::{MemberInfo, Question, Snapshot};
use crate::text::{
    STALLED_PREFIX, age, agent_text, blocked_key, context_key, died_key, late_key, question_key,
    result_key, result_path, result_written, silent_key, stalled_key, unanswered_key, unread_key,
    unreported_key,
};
use crate::{UNANSWERED_AFTER, UNREAD_AFTER};

/// One thing the poll will do in its write transaction.
pub(crate) enum Action {
    /// Print `text` once: only when `key` was not announced before.
    Announce {
        key: String,
        name: &'static str,
        text: String,
    },
    /// Route B: move a `pending` result to `delivered` through the watch, and
    /// print `text` if this watch was the one that moved it.
    ViaWatch {
        completion: Ulid,
        key: String,
        text: String,
    },
    /// ADR 0007 point 6a: a result left in `delivering` becomes
    /// `undelivered` with `delivery_interrupted`, and prints `text` if this
    /// watch was the one that changed it.
    Interrupt {
        completion: Ulid,
        key: String,
        text: String,
    },
    /// The open `TEAM-STALLED` episode is over: nothing is printed.
    EndStall,
}

pub(crate) struct Plan {
    pub actions: Vec<Action>,
    /// The body of the `TEAM-COMPLETE` event, when every member is settled.
    pub complete: Option<String>,
}

/// What one member's state means for the team.
struct Verdict {
    /// The label `TEAM-COMPLETE` gives the member once it is settled:
    /// `done`, or how it is not done.
    settled: Option<String>,
    /// Nothing will move without the lead.
    stuck: bool,
}

impl Verdict {
    fn waiting() -> Self {
        Self {
            settled: None,
            stuck: false,
        }
    }

    fn settled(label: impl Into<String>) -> Self {
        Self {
            settled: Some(label.into()),
            stuck: false,
        }
    }
}

struct Env<'a> {
    ctx: &'a Ctx<'a>,
    snap: &'a Snapshot,
    now: Timestamp,
    grace_from: Option<Timestamp>,
    sleeps: Vec<(Timestamp, Duration)>,
    listings: Listings,
    /// Keys this plan already announces, so one occurrence is one action.
    planned: HashSet<String>,
    actions: Vec<Action>,
}

impl Env<'_> {
    fn announce(&mut self, key: String, name: &'static str, text: String) {
        if !self.snap.announced.contains(&key) && self.planned.insert(key.clone()) {
            self.actions.push(Action::Announce { key, name, text });
        }
    }

    /// Route B: the lead's route is the watch, or the member's harness
    /// never reports a turn end (ADR 0007).
    fn reports_through_watch(&self, role: &Role) -> bool {
        self.snap.team.lead_route == LeadRoute::Watch
            || self
                .ctx
                .adapters(&Party::Member(role.clone()))
                .is_ok_and(|(harness, _)| !harness.capabilities().turn_end_hook)
    }
}

pub(crate) fn plan(
    ctx: &Ctx<'_>,
    snap: &Snapshot,
    memory: &mut WatchMemory,
    now: Timestamp,
) -> Plan {
    let interval = Duration::from_secs(ctx.config.poll_s.saturating_mul(3));
    // The wall clock runs on while the machine sleeps and the monotonic
    // clock does not. A wall gap far longer than the monotonic gap means the
    // machine slept; a slow poll moves both clocks alike. Every member looks
    // quiet and every process looks gone after a sleep, so nothing counted
    // before it is evidence (WAT-14).
    let wall_gap = memory.last_poll.map(|last| now.saturating_since(last));
    let mono_gap = memory
        .last_mono
        .map(|last| ctx.clock.monotonic().saturating_duration_since(last));
    let gap = wall_gap.map(|wall| wall.saturating_sub(mono_gap.unwrap_or_default()));
    if let Some(gap) = gap.filter(|gap| *gap > interval) {
        memory
            .members
            .values_mut()
            .for_each(MemberMemory::forget_quiet);
        memory.grace_from = Some(now);
        memory.sleeps.push((
            now,
            gap.saturating_sub(Duration::from_secs(ctx.config.poll_s)),
        ));
    }
    memory
        .members
        .retain(|role, _| snap.members.iter().any(|m| &m.member.role == role));
    memory.watched_a_member |= !snap.members.is_empty();

    let mut env = Env {
        ctx,
        snap,
        now,
        grace_from: memory.grace_from,
        sleeps: memory.sleeps.clone(),
        listings: Listings::default(),
        planned: HashSet::new(),
        actions: Vec::new(),
    };
    let mut settled: Vec<(Role, String)> = Vec::new();
    let mut outstanding = 0_usize;
    let mut stuck = Vec::new();
    for info in &snap.members {
        let role = &info.member.role;
        let generation = info.member.generation;
        let resumed = MemberMemory::resumed(generation, info.silent_seen, info.blocked_seen);
        let mm = memory
            .members
            .entry(role.clone())
            .or_insert_with(|| resumed.clone());
        if mm.generation != generation {
            *mm = resumed;
        }
        let verdict = judge(&mut env, info, mm);
        if let Some(label) = verdict.settled {
            settled.push((role.clone(), label));
        } else {
            outstanding += 1;
            if verdict.stuck {
                stuck.push(role.clone());
            }
        }
    }
    for q in &snap.questions {
        question(&mut env, q);
    }

    // Time asleep gave nobody the chance to read or answer, as for `LATE`.
    let awake_since = |since: Timestamp| {
        now.saturating_since(since)
            .saturating_sub(slept_since(&memory.sleeps, since))
    };
    for u in &snap.unread {
        let waited = awake_since(u.since);
        // A removed or failed member's inbox is not waited on.
        if waited > UNREAD_AFTER && snap.members.iter().any(|m| m.member.role == u.to) {
            env.announce(
                unread_key(&u.message, &u.to),
                "UNREAD",
                format!("UNREAD {} -> {} {}", u.display_id, u.to, age(waited)),
            );
        }
    }
    for q in &snap.unanswered {
        let waited = awake_since(q.since);
        if waited > UNANSWERED_AFTER {
            env.announce(
                unanswered_key(&q.message),
                "UNANSWERED",
                format!(
                    "UNANSWERED {} from {} {}",
                    q.display_id,
                    q.sender,
                    age(waited)
                ),
            );
        }
    }

    // Everyone still outstanding is stuck right now: only that is worth a
    // push. It is said once per stall episode, and the episode ends when
    // something moves. Both facts are announcements in the store, so a
    // restarted watch finds the open episode and stays quiet.
    let episode = env
        .snap
        .announced
        .iter()
        .filter_map(|k| k.strip_prefix(STALLED_PREFIX)?.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    let open = u64::from(episode) > env.snap.stalls_ended;
    if outstanding > 0 && stuck.len() == outstanding {
        if !open {
            let roles: Vec<&str> = stuck.iter().map(Role::as_str).collect();
            env.announce(
                stalled_key(episode + 1),
                "TEAM-STALLED",
                format!("TEAM-STALLED {}", roles.join(", ")),
            );
        }
    } else if open {
        env.actions.push(Action::EndStall);
    }

    // The poll that says `WATCHING` may have read the roster a moment
    // before `launch --add` wrote a new member, and a watch that prints
    // `TEAM-COMPLETE` exits, so that member would run unwatched. That poll
    // leaves `TEAM-COMPLETE` to the next one (WAT-20).
    //
    // An empty roster completes the team only once this watch has seen a
    // live member: the lead has removed the last one, and nothing is left
    // to watch. Before that, the lead is still launching the first wave.
    let anything_to_report = !settled.is_empty() || memory.watched_a_member;
    let complete = (outstanding == 0 && anything_to_report && !memory.holds_complete())
        .then(|| complete_text(&settled, snap.closer_missing));
    Plan {
        actions: env.actions,
        complete,
    }
}

/// What `TEAM-COMPLETE` adds while the team's code has had no closer. Leads
/// took the line as the end of the run and closed teams with no closer.
// shortcut: a "not done" list past about 150 characters pushes this past
// the event cap, which cuts it; cut the list instead if a big team needs it.
const NO_CLOSER_YET: &str = ". If no wave is due: add the integrator when branches join, then the closer (teamwire preset closer).";

/// `TEAM-COMPLETE 2/3 done; not done: c died`. Only status `done` counts.
/// `TEAM-COMPLETE 0/0 done` for a team whose members the lead has all
/// removed: no live member is left to count, and nothing is added to it.
fn complete_text(settled: &[(Role, String)], closer_missing: bool) -> String {
    let done = settled.iter().filter(|(_, label)| label == "done").count();
    let mut text = format!("TEAM-COMPLETE {done}/{} done", settled.len());
    let rest: Vec<String> = settled
        .iter()
        .filter(|(_, label)| label != "done")
        .map(|(role, label)| format!("{role} {label}"))
        .collect();
    if !rest.is_empty() {
        text.push_str("; not done: ");
        text.push_str(&rest.join(", "));
    }
    if closer_missing && !settled.is_empty() {
        text.push_str(NO_CLOSER_YET);
    }
    text
}

fn question(env: &mut Env<'_>, q: &Question) {
    let name = if q.state == DeliveryState::Unconfirmed {
        "QUESTION-UNCONFIRMED"
    } else {
        "QUESTION-UNDELIVERED"
    };
    env.announce(
        question_key(name, &q.message),
        name,
        format!("{name} {} from {}", q.display_id, q.sender),
    );
}

fn judge(env: &mut Env<'_>, info: &MemberInfo, mm: &mut MemberMemory) -> Verdict {
    if let Some(record) = &info.completion {
        return judge_completion(env, info, mm, record);
    }
    if info.member.state != MemberState::Started {
        // Launch keeps a member `creating` while its setup command and the
        // host spawn run, and each has its own time limit. A launch still
        // `creating` after the grace and both limits counts as stuck. The
        // waits for the worktree, the trust dialog and the occupant are not
        // in the sum, so an extreme launch can still raise a false
        // TEAM-STALLED. It is not settled: launch may still finish it.
        let cfg = &env.ctx.config;
        let limit = cfg.startup_grace_s + cfg.setup_timeout_s + cfg.spawn_timeout_s;
        let late = env.now.saturating_since(info.run_start) >= Duration::from_secs(limit);
        return Verdict {
            settled: None,
            stuck: late,
        };
    }
    judge_running(env, info, mm)
}

fn judge_completion(
    env: &mut Env<'_>,
    info: &MemberInfo,
    mm: &mut MemberMemory,
    record: &CompletionRecord,
) -> Verdict {
    let c = &record.completion;
    let role = &info.member.role;
    let path = result_path(&env.snap.team_dir, role, record.result_sha256.as_ref());
    match c.report_state {
        ReportState::Delivered => Verdict::settled(if c.status == CompletionStatus::Done {
            "done"
        } else {
            c.status.as_str()
        }),
        ReportState::Pending if env.reports_through_watch(role) => {
            // `done` stores the completion and writes the result file just
            // after, and the file must exist before the line that delivers
            // it (REP-9). A poll between the two waits one poll for the
            // file, then reports anyway, so a `done` that died after storing
            // still reaches the lead, pointing at the member's report.
            mm.declared_polls += 1;
            if mm.declared_polls < 2
                && !result_written(&env.snap.team_dir, role, record.result_sha256.as_ref())
            {
                return Verdict::waiting();
            }
            env.actions.push(Action::ViaWatch {
                completion: c.ulid,
                key: result_key("COMPLETED", &c.ulid),
                text: format!(
                    "COMPLETED {role} ({}) {} Result: {path}",
                    c.status,
                    agent_text(&c.summary, 200)
                ),
            });
            Verdict::waiting()
        }
        ReportState::Pending => {
            // Not reported yet. A completion the hook delivers a moment
            // later needs no line, so one poll of grace comes first.
            mm.declared_polls += 1;
            if mm.declared_polls < 2 {
                // Idle polls before `done` say nothing about the hook, which
                // runs only at the turn end after it.
                mm.quiet = 0;
                return Verdict::waiting();
            }
            env.announce(
                result_key("DECLARED", &c.ulid),
                "DECLARED",
                format!(
                    "DECLARED {role} ({}) recorded, not reported yet; it is reported when its turn ends",
                    c.status
                ),
            );
            // Only the member's own turn-end hook reports this completion, so
            // a member that died before it ran is judged like a running one
            // (WAT-6). Looking only after `DECLARED` keeps that line first.
            let Some(seen) = look(env, info, mm) else {
                return Verdict::settled("died");
            };
            // An idle member has ended its turn, so its hook ran. Still
            // pending after the hook's budget and one poll more means the hook
            // failed or is an older build's; nothing else tells the lead.
            match seen {
                Seen::Alive(Activity::Idle) => mm.quiet += 1,
                Seen::Alive(_) => mm.quiet = 0,
                Seen::Absent | Seen::NoInfo => {}
            }
            let cfg = env.ctx.config;
            if u64::from(mm.quiet).saturating_mul(cfg.poll_s) > cfg.hook_budget_s + cfg.poll_s {
                env.announce(
                    unreported_key(role, info.member.generation),
                    "UNREPORTED",
                    format!(
                        "UNREPORTED {role}: done is recorded but its turn-end hook did not report it; run teamwire doctor"
                    ),
                );
            }
            Verdict::waiting()
        }
        ReportState::Delivering => {
            let stale = Duration::from_secs(env.ctx.config.delivering_stale_s);
            if record
                .delivering_since
                .is_some_and(|since| env.now.saturating_since(since) > stale)
            {
                env.actions.push(Action::Interrupt {
                    completion: c.ulid,
                    key: result_key("UNDELIVERED", &c.ulid),
                    text: format!(
                        "UNDELIVERED {role} {} Result: {path}",
                        DeliveryErrorCode::DeliveryInterrupted
                    ),
                });
            }
            Verdict::waiting()
        }
        ReportState::Undelivered => {
            let reason = c.reason.as_ref().map_or("unknown", |r| r.code.as_str());
            env.announce(
                result_key("UNDELIVERED", &c.ulid),
                "UNDELIVERED",
                format!("UNDELIVERED {role} {reason} Result: {path}"),
            );
            Verdict::settled("undelivered")
        }
        ReportState::Unconfirmed => {
            env.announce(
                result_key("UNCONFIRMED", &c.ulid),
                "UNCONFIRMED",
                format!("UNCONFIRMED {role} Result: {path}"),
            );
            Verdict::settled("unconfirmed")
        }
    }
}

/// One look at a started member. `None` once it is announced `DIED`.
fn look(env: &mut Env<'_>, info: &MemberInfo, mm: &mut MemberMemory) -> Option<Seen> {
    let role = &info.member.role;
    let generation = info.member.generation;
    let seen = observe(env.ctx, &info.member, &mut env.listings);
    match seen {
        Seen::Alive(_) => mm.absent = 0,
        Seen::Absent => mm.absent += 1,
        // Death needs three polls in a row that saw the member gone; a poll
        // that saw nothing breaks the run.
        Seen::NoInfo => mm.absent = 0,
    }
    if mm.absent >= 3 || env.snap.announced.contains(&died_key(role, generation)) {
        env.announce(died_key(role, generation), "DIED", format!("DIED {role}"));
        return None;
    }
    Some(seen)
}

fn judge_running(env: &mut Env<'_>, info: &MemberInfo, mm: &mut MemberMemory) -> Verdict {
    let role = &info.member.role;
    let generation = info.member.generation;
    let cfg = env.ctx.config;
    let Some(seen) = look(env, info, mm) else {
        return Verdict::settled("died");
    };

    // One read per live member per poll; the announcement key keeps it to
    // one line per generation.
    if let Some(c) = crate::context::read(env.ctx, &info.member, info.context_handoff_tokens)
        && c.tokens >= c.handoff_at
    {
        env.announce(
            context_key(role, generation),
            "CONTEXT",
            format!(
                "CONTEXT {role} {}% {}k of {}k (handoff at {}k)",
                c.pct,
                c.tokens / 1000,
                c.window / 1000,
                c.handoff_at / 1000
            ),
        );
    }

    // The quiet grace counts from the later of the run's start and the last
    // sleep gap. `LATE` counts from the run's start alone.
    let quiet_from = env
        .grace_from
        .map_or(info.run_start, |g| g.max(info.run_start));
    let in_grace = env.now.saturating_since(quiet_from) < Duration::from_secs(cfg.startup_grace_s);
    let mut idle = false;
    match seen {
        Seen::Alive(Activity::Idle) if !mm.seen_working && in_grace => mm.quiet = 0,
        Seen::Alive(Activity::Idle) => {
            mm.quiet += 1;
            idle = true;
        }
        Seen::Alive(_) => {
            mm.seen_working = true;
            mm.quiet = 0;
            // Working again after `SILENT` makes the next quiet stretch a
            // new episode.
            if mm.silent_open {
                mm.silent_ep += 1;
                mm.silent_open = false;
            }
        }
        Seen::Absent | Seen::NoInfo => {}
    }

    let mut stuck = false;
    if seen == Seen::Alive(Activity::Blocked) {
        mm.blocked_open = true;
        stuck = true;
        env.announce(
            blocked_key(role, generation, mm.blocked_ep),
            "BLOCKED",
            format!("BLOCKED {role}"),
        );
    } else if matches!(seen, Seen::Alive(_)) && mm.blocked_open {
        mm.blocked_ep += 1;
        mm.blocked_open = false;
    }
    if mm.quiet >= cfg.silent_polls {
        mm.silent_open = true;
        stuck = true;
        env.announce(
            silent_key(role, generation, mm.silent_ep),
            "SILENT",
            format!("SILENT {role}"),
        );
    }
    let run_age = env
        .now
        .saturating_since(info.run_start)
        .saturating_sub(slept_since(&env.sleeps, info.run_start));
    if run_age > Duration::from_secs(info.member.deadline_s) {
        // Overdue and still working is not stalled: it moves on its own.
        stuck |= idle;
        env.announce(
            late_key(role, generation),
            "LATE",
            format!("LATE {role} {}", age(run_age)),
        );
    }
    Verdict {
        settled: None,
        stuck,
    }
}
