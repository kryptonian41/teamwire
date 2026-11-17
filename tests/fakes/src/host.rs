//! `FakeHost`: an in-process `Host` with places in a map. Every call is
//! recorded with its kind, and a test scripts faults per call.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use teamwire_core::{
    Address, Host, HostCaps, HostError, HostErrorCode, HostId, Key, Occupant, OneLine, Place,
    PlaceState, SpawnRequest, SplitRequest, SubmitAck,
};

use crate::calls::{CallKind, CallLog, lock};
use crate::meeting::Meeting;

mod split;

/// What can go wrong with one call. Scripted per method name with
/// `FakeHost::inject`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fault {
    /// The call fails with this error and changes nothing.
    Fail(HostError),
    /// The call runs past its timeout: it changes nothing and returns
    /// `HostErrorCode::Timeout`, as an adapter does when its timer fires.
    Hang,
    /// The host does the work and then reports failure, so a caller that
    /// retries would do it twice. For `spawn`, `submit`, `send_keys`,
    /// `close`, `split_run` and `close_pane`.
    AcceptedButFail(HostError),
    /// `occupant` reports this instead of the truth, once per firing.
    WrongOccupant(Occupant),
    /// `occupant` reports the truth, then the place's occupant becomes this:
    /// the occupant changes between two calls.
    ChangeOccupantAfter(Occupant),
}

/// The title, argv and environment a place was spawned with.
pub type SpawnedWith = (String, Vec<String>, Vec<(String, String)>);

#[derive(Debug, Clone)]
struct PlaceRec {
    name: String,
    title: String,
    cwd: PathBuf,
    argv: Vec<String>,
    env: Vec<(String, String)>,
    state: PlaceState,
    occupant: Occupant,
    screen: String,
    submitted: Vec<String>,
    keys: Vec<Key>,
    /// The whole request of a place `spawn` made; none for one a test added.
    request: Option<SpawnRequest>,
    /// The request of a place `split_run` made.
    split: Option<SplitRequest>,
}

#[derive(Debug)]
struct State {
    caps: HostCaps,
    place_env_var: Option<&'static str>,
    place_suffix_var: Option<&'static str>,
    places: HashMap<String, PlaceRec>,
    order: Vec<String>,
    foreign_agents: Vec<String>,
    faults: Vec<(&'static str, Fault, u32)>,
}

/// An in-process `Host`.
#[derive(Debug)]
pub struct FakeHost {
    id: HostId,
    log: CallLog,
    next: AtomicU64,
    state: Mutex<State>,
    reads: Meeting,
}

impl FakeHost {
    pub const ID: &'static str = "fake";

    /// A host that can type, reports state, knows the session of a place,
    /// acknowledges a submit as `submitted`, refuses to type into a dialog,
    /// and does not set environment at spawn (as herdr).
    pub fn caps() -> HostCaps {
        HostCaps {
            can_type: true,
            reports_state: true,
            knows_session_of_place: true,
            submit_ack: SubmitAck::Submitted,
            sets_env_at_spawn: false,
            guards_dialogs: true,
            splits_pane: true,
        }
    }

    pub fn new() -> Self {
        Self::with_id(HostId::from_static(Self::ID))
    }

    pub fn with_id(id: HostId) -> Self {
        Self {
            id,
            log: CallLog::default(),
            next: AtomicU64::new(1),
            state: Mutex::new(State {
                caps: Self::caps(),
                place_env_var: Some("FAKE_PLACE_ID"),
                place_suffix_var: None,
                places: HashMap::new(),
                order: Vec::new(),
                foreign_agents: Vec::new(),
                faults: Vec::new(),
            }),
            reads: Meeting::default(),
        }
    }

    pub fn log(&self) -> &CallLog {
        &self.log
    }

    pub fn set_caps(&self, caps: HostCaps) {
        lock(&self.state).caps = caps;
    }

    pub fn set_place_env_var(&self, var: Option<&'static str>) {
        lock(&self.state).place_env_var = var;
    }

    /// A host whose place id is the place variable, `@`, and the value of
    /// `var` (as tmux joins the pane and its server). `None` is the default.
    pub fn set_place_suffix_var(&self, var: Option<&'static str>) {
        lock(&self.state).place_suffix_var = var;
    }

    /// Fires `fault` on the next `times` calls of the method `op`
    /// (`"spawn"`, `"submit"`, `"state"`, `"occupant"`, `"read_tail"`,
    /// `"send_keys"`, `"close"`, `"list"`, `"live_agent_names"`,
    /// `"split_run"`, `"close_pane"`). Faults for
    /// one method fire in the order they were injected.
    pub fn inject(&self, op: &'static str, fault: Fault, times: u32) {
        lock(&self.state).faults.push((op, fault, times));
    }

    /// Like `inject` for every later call.
    pub fn inject_always(&self, op: &'static str, fault: Fault) {
        self.inject(op, fault, u32::MAX);
    }

    /// Makes each `read_tail` wait, up to `wait`, until `want` reads are in
    /// progress at once. `read_peak` is the most it saw. Reads made one by one
    /// never get past one, so a test proves calls run at the same time
    /// without measuring wall time.
    pub fn meet_reads(&self, want: usize, wait: Duration) {
        self.reads.set(want, wait);
    }

    /// The most `read_tail` calls in progress at once since `meet_reads`.
    pub fn read_peak(&self) -> usize {
        self.reads.peak()
    }

    pub fn clear_faults(&self) {
        lock(&self.state).faults.clear();
    }

    /// An agent some other tool started: it shows in `live_agent_names`
    /// but is not a place teamwire made.
    pub fn add_foreign_agent(&self, name: &str) {
        lock(&self.state).foreign_agents.push(name.to_owned());
    }

    /// Adds a place as if a member were already running in it.
    pub fn add_place(&self, name: &str, state: PlaceState, occupant: Occupant) -> Place {
        let id = self.new_id();
        self.add_place_with_id(id, name, state, occupant)
    }

    /// Like `add_place` with the id chosen by the caller.
    pub fn add_place_with_id(
        &self,
        id: String,
        name: &str,
        state: PlaceState,
        occupant: Occupant,
    ) -> Place {
        let mut st = lock(&self.state);
        st.places.insert(
            id.clone(),
            PlaceRec {
                name: name.to_owned(),
                title: name.to_owned(),
                cwd: PathBuf::new(),
                argv: Vec::new(),
                env: Vec::new(),
                state,
                occupant,
                screen: String::new(),
                submitted: Vec::new(),
                keys: Vec::new(),
                request: None,
                split: None,
            },
        );
        st.order.push(id.clone());
        Place {
            host: self.id.clone(),
            id,
        }
    }

    pub fn set_state(&self, place: &Place, state: PlaceState) {
        self.with_rec(place, |r| r.state = state);
    }

    pub fn set_occupant(&self, place: &Place, occupant: Occupant) {
        self.with_rec(place, |r| r.occupant = occupant);
    }

    /// Replaces what `read_tail` shows.
    pub fn set_screen(&self, place: &Place, text: &str) {
        self.with_rec(place, |r| text.clone_into(&mut r.screen));
    }

    /// Lines `submit` accepted for the place, in order.
    pub fn submitted(&self, place: &Place) -> Vec<String> {
        self.read_rec(place, |r| r.submitted.clone())
            .unwrap_or_default()
    }

    /// Keys `send_keys` accepted for the place, in order.
    pub fn keys_sent(&self, place: &Place) -> Vec<Key> {
        self.read_rec(place, |r| r.keys.clone()).unwrap_or_default()
    }

    /// The request that made the place, as far as the fake keeps it.
    pub fn spawned_with(&self, place: &Place) -> Option<SpawnedWith> {
        self.read_rec(place, |r| (r.title.clone(), r.argv.clone(), r.env.clone()))
    }

    /// The whole request `spawn` got for the place; `None` for a place a
    /// test added without `spawn`.
    pub fn spawn_request_of(&self, place: &Place) -> Option<SpawnRequest> {
        self.read_rec(place, |r| r.request.clone()).flatten()
    }

    pub fn cwd_of(&self, place: &Place) -> Option<PathBuf> {
        self.read_rec(place, |r| r.cwd.clone())
    }

    fn new_id(&self) -> String {
        format!("p{}", self.next.fetch_add(1, Ordering::Relaxed))
    }

    fn with_rec(&self, place: &Place, f: impl FnOnce(&mut PlaceRec)) {
        if let Some(rec) = lock(&self.state).places.get_mut(&place.id) {
            f(rec);
        }
    }

    fn read_rec<T>(&self, place: &Place, f: impl FnOnce(&PlaceRec) -> T) -> Option<T> {
        lock(&self.state).places.get(&place.id).map(f)
    }

    /// Takes the next fault scripted for `op`, if any.
    fn take_fault(&self, op: &'static str) -> Option<Fault> {
        let mut st = lock(&self.state);
        let slot = st
            .faults
            .iter_mut()
            .find(|(o, _, left)| *o == op && *left > 0)?;
        if slot.2 != u32::MAX {
            slot.2 -= 1;
        }
        Some(slot.1.clone())
    }

    /// Checks the fault before an effect. `Ok(Some(err))` means: do the
    /// effect, then return `err`.
    fn before(&self, op: &'static str) -> Result<Option<HostError>, HostError> {
        match self.take_fault(op) {
            Some(Fault::Fail(e)) => Err(e),
            Some(Fault::Hang) => Err(timeout_error(op)),
            Some(Fault::AcceptedButFail(e)) => Ok(Some(e)),
            Some(Fault::WrongOccupant(_) | Fault::ChangeOccupantAfter(_)) | None => Ok(None),
        }
    }

    fn not_found(place: &Place) -> HostError {
        HostError::new(
            HostErrorCode::NotFound,
            format!("no place {} on this host", place.id),
        )
    }

    fn check_place(&self, place: &Place) -> Result<(), HostError> {
        if place.host == self.id && lock(&self.state).places.contains_key(&place.id) {
            Ok(())
        } else {
            Err(Self::not_found(place))
        }
    }

    fn finish<T>(value: T, accepted_but_failed: Option<HostError>) -> Result<T, HostError> {
        match accepted_but_failed {
            Some(e) => Err(e),
            None => Ok(value),
        }
    }
}

fn timeout_error(op: &str) -> HostError {
    HostError::new(HostErrorCode::Timeout, format!("{op} ran past its timeout"))
}

impl Default for FakeHost {
    fn default() -> Self {
        Self::new()
    }
}

impl Host for FakeHost {
    fn id(&self) -> HostId {
        self.id.clone()
    }

    fn capabilities(&self) -> HostCaps {
        lock(&self.state).caps
    }

    fn spawn(&self, req: &SpawnRequest, _timeout: Duration) -> Result<Place, HostError> {
        self.log.record("spawn", CallKind::Write, req.name.as_str());
        let late_error = self.before("spawn")?;
        // A place whose agent is gone (a dead session left in its tab) holds
        // no live agent, here and in `live_agent_names`, so its name is free.
        let taken = {
            let st = lock(&self.state);
            st.places
                .values()
                .any(|r| r.name == req.name.as_str() && r.state != PlaceState::Gone)
                || st.foreign_agents.iter().any(|n| n == req.name.as_str())
        };
        if taken {
            return Err(HostError::new(
                HostErrorCode::BadArgument,
                format!(
                    "agent_name_taken: an agent named {} is already live",
                    req.name
                ),
            ));
        }
        let place = self.add_place(
            req.name.as_str(),
            PlaceState::Idle,
            Occupant {
                session: None,
                pids: Vec::new(),
            },
        );
        self.with_rec(&place, |r| {
            r.title.clone_from(&req.title);
            r.cwd.clone_from(&req.cwd);
            r.argv.clone_from(&req.argv);
            r.env.clone_from(&req.env);
            r.request = Some(req.clone());
        });
        Self::finish(place, late_error)
    }

    fn submit(&self, place: &Place, text: &OneLine, _timeout: Duration) -> Result<(), HostError> {
        self.log
            .record("submit", CallKind::Write, format!("{} {}", place.id, text));
        let late_error = self.before("submit")?;
        self.check_place(place)?;
        let caps = lock(&self.state).caps;
        if !caps.can_type {
            return Err(HostError::unsupported());
        }
        let blocked = self.read_rec(place, |r| r.state) == Some(PlaceState::Blocked);
        if blocked && caps.guards_dialogs {
            return Err(HostError::new(
                HostErrorCode::Blocked,
                "the place shows a dialog",
            ));
        }
        self.with_rec(place, |r| {
            r.submitted.push(text.as_str().to_owned());
            r.screen.push_str(text.as_str());
            r.screen.push('\n');
        });
        Self::finish((), late_error)
    }

    fn state(&self, place: &Place, _timeout: Duration) -> Result<PlaceState, HostError> {
        self.log.record("state", CallKind::Read, place.id.as_str());
        self.before("state")?;
        self.check_place(place)?;
        if !lock(&self.state).caps.reports_state {
            return Ok(PlaceState::Unknown);
        }
        self.read_rec(place, |r| r.state)
            .ok_or_else(|| Self::not_found(place))
    }

    fn occupant(&self, place: &Place, _timeout: Duration) -> Result<Occupant, HostError> {
        self.log
            .record("occupant", CallKind::Read, place.id.as_str());
        let fault = self.take_fault("occupant");
        match &fault {
            Some(Fault::Fail(e)) => return Err(e.clone()),
            Some(Fault::Hang) => return Err(timeout_error("occupant")),
            _ => {}
        }
        self.check_place(place)?;
        let knows_session = lock(&self.state).caps.knows_session_of_place;
        let mut truth = self
            .read_rec(place, |r| r.occupant.clone())
            .ok_or_else(|| Self::not_found(place))?;
        if !knows_session {
            truth.session = None;
        }
        match fault {
            Some(Fault::WrongOccupant(other)) => Ok(other),
            Some(Fault::ChangeOccupantAfter(next)) => {
                self.with_rec(place, |r| r.occupant = next);
                Ok(truth)
            }
            _ => Ok(truth),
        }
    }

    fn read_tail(
        &self,
        place: &Place,
        lines: u16,
        _timeout: Duration,
    ) -> Result<String, HostError> {
        self.log
            .record("read_tail", CallKind::Read, place.id.as_str());
        let _met = self.reads.join();
        self.before("read_tail")?;
        self.check_place(place)?;
        let screen = self
            .read_rec(place, |r| r.screen.clone())
            .ok_or_else(|| Self::not_found(place))?;
        let all: Vec<&str> = screen.lines().collect();
        let skip = all.len().saturating_sub(usize::from(lines));
        Ok(all.into_iter().skip(skip).collect::<Vec<_>>().join("\n"))
    }

    fn send_keys(&self, place: &Place, keys: &[Key], _timeout: Duration) -> Result<(), HostError> {
        self.log.record(
            "send_keys",
            CallKind::Write,
            format!("{} {keys:?}", place.id),
        );
        let late_error = self.before("send_keys")?;
        self.check_place(place)?;
        if !lock(&self.state).caps.can_type {
            return Err(HostError::unsupported());
        }
        self.with_rec(place, |r| r.keys.extend_from_slice(keys));
        Self::finish((), late_error)
    }

    fn close(
        &self,
        place: &Place,
        expected: &Address,
        _timeout: Duration,
    ) -> Result<(), HostError> {
        self.log.record("close", CallKind::Write, place.id.as_str());
        let late_error = self.before("close")?;
        self.check_place(place)?;
        if self.read_rec(place, |r| r.name != expected.as_str()) == Some(true) {
            return Err(Self::not_found(place));
        }
        let mut st = lock(&self.state);
        st.places.remove(&place.id);
        st.order.retain(|id| id != &place.id);
        drop(st);
        Self::finish((), late_error)
    }

    fn list(&self, _timeout: Duration) -> Result<Vec<Place>, HostError> {
        self.log.record("list", CallKind::Read, "");
        self.before("list")?;
        let st = lock(&self.state);
        // A place with no name holds no teamwire agent (a split pane), and
        // herdr and tmux list only those that do.
        Ok(st
            .order
            .iter()
            .filter(|id| st.places.get(*id).is_some_and(|r| !r.name.is_empty()))
            .map(|id| Place {
                host: self.id.clone(),
                id: id.clone(),
            })
            .collect())
    }

    fn live_agent_names(&self, _timeout: Duration) -> Result<Vec<String>, HostError> {
        self.log.record("live_agent_names", CallKind::Read, "");
        self.before("live_agent_names")?;
        let st = lock(&self.state);
        let mut names: Vec<String> = st
            .order
            .iter()
            .filter_map(|id| st.places.get(id))
            .filter(|r| r.state != PlaceState::Gone && !r.name.is_empty())
            .map(|r| r.name.clone())
            .collect();
        names.extend(st.foreign_agents.iter().cloned());
        Ok(names)
    }

    fn place_env_var(&self) -> Option<&'static str> {
        lock(&self.state).place_env_var
    }

    fn place_from_env(&self, read: &dyn Fn(&str) -> Option<String>) -> Option<String> {
        let (var, suffix) = {
            let st = lock(&self.state);
            (st.place_env_var?, st.place_suffix_var)
        };
        let id = read(var).filter(|id| !id.is_empty())?;
        Some(match suffix.and_then(read) {
            Some(extra) => format!("{id}@{extra}"),
            None => id,
        })
    }

    fn split_run(
        &self,
        beside: &Place,
        req: &SplitRequest,
        _timeout: Duration,
    ) -> Result<Place, HostError> {
        self.split_beside(beside, req)
    }

    fn close_pane(&self, place: &Place, _timeout: Duration) -> Result<(), HostError> {
        self.close_split(place)
    }
}
