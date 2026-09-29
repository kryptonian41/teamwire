//! The metrics of `docs/metrics.md` section 2: name, unit, kind and, for a
//! histogram, its bucket bounds.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Counter,
    Histogram,
    Gauge,
}

#[derive(Debug)]
pub(crate) struct Def {
    pub(crate) name: &'static str,
    pub(crate) unit: &'static str,
    pub(crate) kind: Kind,
    /// Upper bounds of the histogram buckets; empty for the other kinds.
    pub(crate) bounds: &'static [f64],
}

const SECONDS: &[f64] = &[0.01, 0.05, 0.25, 1.0, 5.0, 15.0, 60.0, 300.0, 1800.0];
const ATTEMPTS: &[f64] = &[1.0, 2.0, 3.0, 5.0, 10.0];

const fn counter(name: &'static str, unit: &'static str) -> Def {
    Def {
        name,
        unit,
        kind: Kind::Counter,
        bounds: &[],
    }
}

const fn gauge(name: &'static str, unit: &'static str) -> Def {
    Def {
        name,
        unit,
        kind: Kind::Gauge,
        bounds: &[],
    }
}

const fn histogram(name: &'static str, unit: &'static str, bounds: &'static [f64]) -> Def {
    Def {
        name,
        unit,
        kind: Kind::Histogram,
        bounds,
    }
}

pub(crate) static LEAD_WAKEUPS: Def = counter("teamwire.lead.wakeups", "{wakeup}");
pub(crate) static WATCH_LINES: Def = counter("teamwire.watch.lines", "{line}");
pub(crate) static WATCH_EVENTS: Def = counter("teamwire.watch.events", "{event}");
pub(crate) static MESSAGES_SENT: Def = counter("teamwire.messages.sent", "{message}");
pub(crate) static MESSAGES_REFUSED: Def = counter("teamwire.messages.refused", "{message}");
pub(crate) static DELIVERIES: Def = counter("teamwire.deliveries", "{delivery}");
pub(crate) static DELIVERIES_UNDELIVERED: Def =
    counter("teamwire.deliveries.undelivered", "{delivery}");
pub(crate) static DELIVERY_DURATION: Def = histogram("teamwire.delivery.duration", "s", SECONDS);
pub(crate) static DELIVERY_ATTEMPTS: Def =
    histogram("teamwire.delivery.attempts", "{attempt}", ATTEMPTS);
pub(crate) static MESSAGES_ACKED: Def = counter("teamwire.messages.acked", "{message}");
pub(crate) static ACK_DURATION: Def = histogram("teamwire.ack.duration", "s", SECONDS);
pub(crate) static MESSAGES_UNREAD: Def = gauge("teamwire.messages.unread", "{message}");
pub(crate) static COMPLETIONS: Def = counter("teamwire.completions", "{completion}");
pub(crate) static COMPLETIONS_REFUSED: Def =
    counter("teamwire.completions.refused", "{completion}");
pub(crate) static RESULTS: Def = counter("teamwire.results", "{result}");
pub(crate) static RESULT_DURATION: Def = histogram("teamwire.result.duration", "s", SECONDS);
pub(crate) static HOLDS: Def = counter("teamwire.holds", "{hold}");
pub(crate) static MEMBER_RUN_DURATION: Def =
    histogram("teamwire.member.run.duration", "s", SECONDS);
pub(crate) static MEMBER_REPROMPTS: Def = counter("teamwire.member.reprompts", "{reprompt}");
pub(crate) static MEMBERS_LIVE: Def = gauge("teamwire.members.live", "{member}");
pub(crate) static LAUNCH_MEMBERS: Def = counter("teamwire.launch.members", "{member}");
pub(crate) static LAUNCH_DURATION: Def = histogram("teamwire.launch.duration", "s", SECONDS);
pub(crate) static LOCATOR_REFUSALS: Def = counter("teamwire.locator.refusals", "{refusal}");
pub(crate) static VERIFY_RUNS: Def = counter("teamwire.verify.runs", "{run}");
pub(crate) static VERIFY_FLAGS: Def = counter("teamwire.verify.flags", "{flag}");
pub(crate) static VERIFY_CHECK_DURATION: Def =
    histogram("teamwire.verify.check.duration", "s", SECONDS);
pub(crate) static CLOSE_RUNS: Def = counter("teamwire.close.runs", "{run}");
pub(crate) static STORE_ERRORS: Def = counter("teamwire.store.errors", "{error}");
pub(crate) static HOOK_DURATION: Def = histogram("teamwire.hook.duration", "s", SECONDS);
pub(crate) static EXPORT_LAG: Def = gauge("teamwire.export.lag", "{event}");

// The three derived lines of `teamwire metrics`. They exist locally only.
pub(crate) const WAKEUPS_PER_MEMBER: &str = "teamwire.lead.wakeups_per_member";
pub(crate) const READ_SHARE: &str = "teamwire.messages.read_share";
pub(crate) const FIRST_TRY_SHARE: &str = "teamwire.results.first_try_share";
