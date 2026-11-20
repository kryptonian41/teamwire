//! The set rules of LCH-34: warnings about the readers of a set and about
//! a reconciler. They warn; they never refuse, and code never picks a
//! harness or a model from a family (ADR 0053, ADR 0056).
//!
//! A reader is a member with a set label that is not a reconciler. The
//! rules read the members of the spec file and the members the team
//! already holds. A stored reader counts while it is neither removed nor
//! failed, or when it reported before it left: the lead replaces a reader
//! that was lost without a report, and the replacement must not warn.
//!
//! A rule about one member warns only for a member of this spec file, and
//! a rule about two members only when one of them is in this spec file.
//! The team's own members were warned about when they launched.

use std::collections::{BTreeMap, BTreeSet};

use teamwire_core::{Member, MemberKind, MemberState, ModelFamily, Registry, Role};
use teamwire_store::{ReadTx, StoreError};

use crate::PlannedMember;
use crate::plan::Cx;

/// One member as the set rules see it, from the spec file or from the store.
struct Seat<'a> {
    role: &'a str,
    kind: MemberKind,
    set: Option<&'a str>,
    model: Option<&'a str>,
    family: ModelFamily,
    /// The planned member, for a member of this spec file.
    planned: Option<&'a PlannedMember>,
    /// The store holds a completion of the member's current generation.
    reported: bool,
}

impl Seat<'_> {
    fn is_reader(&self) -> bool {
        self.set.is_some() && self.kind != MemberKind::Reconciler
    }

    fn in_spec(&self) -> bool {
        self.planned.is_some()
    }

    /// A known family, or `None` when the member names no model or its
    /// family cannot be told.
    fn known_family(&self) -> Option<ModelFamily> {
        self.model?;
        (self.family != ModelFamily::Unknown).then_some(self.family)
    }
}

/// The roles in `members` whose newest completion is of the member's
/// current generation: a reader that was reprompted after it reported has
/// not reported. Only members with a set label are read, because only the
/// set rules ask.
pub(crate) fn reported(tx: &ReadTx<'_>, members: &[Member]) -> Result<BTreeSet<Role>, StoreError> {
    let mut out = BTreeSet::new();
    for m in members.iter().filter(|m| m.set.is_some()) {
        let current = tx
            .latest_completion(&m.role)?
            .is_some_and(|c| c.completion.generation == m.generation);
        if current {
            out.insert(m.role.clone());
        }
    }
    Ok(out)
}

/// The family of `model` as the harness `id` reads it. A harness the
/// registry does not hold reads the string alone, as core does.
fn family_of(
    registry: &Registry,
    id: &teamwire_core::HarnessId,
    model: Option<&str>,
) -> ModelFamily {
    let Some(model) = model else {
        return ModelFamily::Unknown;
    };
    registry.harness(id).map_or_else(
        |_| teamwire_core::model_family(model),
        |h| h.model_family(model),
    )
}

/// The stored members that count and that the spec file does not launch
/// again, oldest first, then the members of the spec file. A warning about
/// two members names the older one first.
fn seats<'a>(cx: &'a Cx<'_>, members: &'a [PlannedMember]) -> Vec<Seat<'a>> {
    let registry = cx.env.registry;
    let in_spec: BTreeSet<&str> = members.iter().map(|m| m.spec.role.as_str()).collect();
    let mut out = Vec::new();
    if let Some(existing) = cx.existing {
        for m in &existing.members {
            let reported = existing.reported.contains(&m.role);
            let gone = matches!(m.state, MemberState::Removed | MemberState::Failed);
            if in_spec.contains(m.role.as_str()) || (gone && !reported) {
                continue;
            }
            let model = existing.models.get(&m.role).and_then(Option::as_deref);
            out.push(Seat {
                role: m.role.as_str(),
                kind: m.kind,
                set: m.set.as_deref(),
                model,
                family: family_of(registry, &m.harness, model),
                planned: None,
                reported,
            });
        }
    }
    out.extend(members.iter().map(|m| Seat {
        role: m.spec.role.as_str(),
        kind: m.kind,
        set: m.spec.set.as_deref(),
        model: m.spec.model.as_deref(),
        family: family_of(registry, &m.harness, m.spec.model.as_deref()),
        planned: Some(m),
        reported: false,
    }));
    out
}

/// Every warning of LCH-34, in the order of the rules' table.
pub(crate) fn set_warnings(cx: &Cx<'_>, members: &[PlannedMember], warnings: &mut Vec<String>) {
    let seats = seats(cx, members);
    let mut sets: BTreeMap<&str, Vec<&Seat<'_>>> = BTreeMap::new();
    for seat in seats.iter().filter(|s| s.is_reader()) {
        if let Some(label) = seat.set {
            sets.entry(label).or_default().push(seat);
        }
    }
    let touched: BTreeSet<&str> = seats
        .iter()
        .filter(|s| s.in_spec())
        .filter_map(|s| s.set)
        .collect();
    for (label, readers) in sets.iter().filter(|(l, _)| touched.contains(*l)) {
        reader_warnings(label, readers, warnings);
    }
    for rec in seats
        .iter()
        .filter(|s| s.in_spec() && s.kind == MemberKind::Reconciler)
    {
        reconciler_warnings(rec, &sets, warnings);
    }
}

/// The rules about the readers of one set: the first six rows of LCH-34.
fn reader_warnings(label: &str, readers: &[&Seat<'_>], warnings: &mut Vec<String>) {
    if let [only] = readers {
        warnings.push(format!(
            "set_too_small: set {label} has one reader ({}); a set needs at least two",
            only.role
        ));
    }
    let pairs = || {
        readers.iter().enumerate().flat_map(move |(i, a)| {
            readers
                .iter()
                .skip(i + 1)
                .filter(move |b| a.in_spec() || b.in_spec())
                .map(move |b| (*a, *b))
        })
    };
    for (a, b) in pairs() {
        if let Some(family) = a.known_family()
            && b.known_family() == Some(family)
        {
            warnings.push(format!(
                "set_same_family: set {label}: {} and {} are both {family} models; a set needs a \
                 different family per reader",
                a.role, b.role
            ));
        }
    }
    for r in readers.iter().filter(|r| r.in_spec()) {
        if r.known_family().is_none() {
            warnings.push(format!(
                "set_family_unknown: set {label}: cannot tell the family of {} ({}); check it by eye",
                r.role,
                r.model.unwrap_or("no model")
            ));
        }
    }
    for (a, b) in pairs() {
        if a.kind != b.kind {
            warnings.push(format!(
                "set_mixed_kinds: set {label}: {} is a {} and {} is a {}; the readers of a set \
                 share one kind",
                a.role, a.kind, b.role, b.kind
            ));
        }
    }
    for r in readers {
        let Some(m) = r.planned else { continue };
        if !(m.spec.quiet == Some(true) || m.kind.is_quiet()) {
            warnings.push(format!(
                "set_reader_not_quiet: set {label}: {} may send messages; give it \"quiet\": true \
                 so the readers stay independent",
                r.role
            ));
        }
    }
    for (a, b) in pairs() {
        if let (Some(x), Some(y)) = (a.planned, b.planned)
            && x.brief != y.brief
        {
            warnings.push(format!(
                "set_briefs_differ: set {label}: {} and {} have different briefs; the readers of \
                 a set get the same brief",
                a.role, b.role
            ));
        }
    }
}

/// The rules about one reconciler of the spec file: the last two rows of
/// LCH-34.
fn reconciler_warnings(
    rec: &Seat<'_>,
    sets: &BTreeMap<&str, Vec<&Seat<'_>>>,
    warnings: &mut Vec<String>,
) {
    let Some((label, readers)) = rec.set.and_then(|l| sets.get_key_value(l)) else {
        warnings.push(format!(
            "set_reconciler_alone: {} is a reconciler with no set to read",
            rec.role
        ));
        return;
    };
    for r in readers.iter().filter(|r| !r.reported) {
        warnings.push(format!(
            "set_reconciler_early: {} reconciles set {label}, but {} has not reported; add the \
             reconciler after every reader has reported",
            rec.role, r.role
        ));
    }
}
