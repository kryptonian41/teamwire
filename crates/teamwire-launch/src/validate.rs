//! Whole-spec validation (LCH-2): every problem is found and reported before
//! anything is created. Reads only: the store is opened read-only and only
//! when the team already has a database.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::Duration;

use teamwire_core::{
    EventType, Host, HostId, LeadRoute, Member, MemberState, Role, SessionId, SessionKind,
    TeamName, TeamSpec, TeamState,
};
use teamwire_store::{Store, TeamRow, database_path};

use crate::plan::{self, Cx};
use crate::{LeadPlan, SpecError, ValidPlan, ValidateEnv};

/// How long a listing of the host's agent names may take.
const LIST_TIMEOUT: Duration = Duration::from_secs(30);
/// The longest title (LCH-23).
pub(crate) const TITLE_MAX: usize = 28;

/// What the store already holds about the team.
pub(crate) struct Existing {
    pub(crate) team: TeamRow,
    pub(crate) members: Vec<Member>,
    /// Roles that reached `started` at any time (LCH-5).
    pub(crate) ever_started: BTreeSet<Role>,
}

/// Checks the whole spec, and every path the launch will touch (LCH-2).
pub fn validate(
    spec: &TeamSpec,
    add: bool,
    env: &ValidateEnv<'_>,
) -> Result<ValidPlan, Vec<SpecError>> {
    let harness = env
        .registry
        .harness(&env.lead_harness)
        .map_err(|e| vec![SpecError::new(None, None, "unknown_harness", e.to_string())])?;
    let session = harness.current_session();
    if session
        .as_ref()
        .is_some_and(|s| s.kind == SessionKind::Background)
    {
        return Err(vec![SpecError::new(
            None,
            None,
            "background_lead",
            "this session runs in the background and has no terminal of its own, so member reports \
             would go to another session. Launch the team from a session in a terminal.",
        )]);
    }
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let existing = read_existing(env, &spec.team)?;
    match (&existing, add) {
        (Some(_), false) => errors.push(SpecError::new(
            None,
            Some("team"),
            "team_exists",
            format!(
                "team {} already exists; pass --add to extend it, pick another name, or close it first",
                spec.team
            ),
        )),
        (None, true) => errors.push(SpecError::new(
            None,
            Some("team"),
            "no_such_team",
            format!("--add needs a running team, and team {} has no database", spec.team),
        )),
        (Some(e), true) if e.team.state != TeamState::Open => errors.push(SpecError::new(
            None,
            Some("team"),
            "team_closed",
            format!("team {} is {}; nothing can be added", spec.team, e.team.state),
        )),
        _ => {}
    }
    let lead = lead_plan(env, existing.as_ref(), session.map(|s| (s.id, s.kind)));
    let lead = match lead {
        Ok(lead) => lead,
        Err(e) => {
            errors.push(e);
            return Err(errors);
        }
    };
    if session_missing(&lead) {
        warnings.push(
            "the lead's session id could not be read; results reach the lead through the watch, \
             and no sleep hold is started"
                .to_owned(),
        );
    }
    if existing.is_none() {
        lead_must_be_locatable(env, &lead, &mut errors);
    }
    team_title(spec, existing.as_ref(), &mut errors);
    plugin_keys(&spec.plugins, None, &mut errors);
    if spec.children.is_empty() {
        errors.push(SpecError::new(
            None,
            Some("children"),
            "no_children",
            "the spec has no members",
        ));
    }
    let mut cx = Cx::new(env, spec, existing.as_ref(), &lead);
    let members: Vec<_> = spec
        .children
        .iter()
        .filter_map(|child| plan::plan_member(&mut cx, child, &mut errors, &mut warnings))
        .collect();
    disk_check(env, &members, &mut errors);
    crate::plan_rules::planning_warnings(&cx, &members, &mut warnings);
    crate::afk::model_warnings(existing.as_ref(), &members, &mut warnings);
    if !errors.is_empty() {
        return Err(errors);
    }
    // `--add` may leave the title out: the team row already holds it.
    let mut planned = spec.clone();
    if let Some(e) = &existing
        && plan::clean(&planned.title).is_empty()
    {
        planned.title.clone_from(&e.team.title);
    }
    Ok(ValidPlan {
        spec: planned,
        add,
        warnings,
        lead,
        members,
        teamwire_bin: env.teamwire_bin.clone(),
    })
}

/// A lead whose harness cannot read `teamwire watch` is reached only by
/// typing into its pane, so it must be found there before anything is made
/// (ADR 0023 point 9). A harness that reads watch lines is not checked.
fn lead_must_be_locatable(env: &ValidateEnv<'_>, lead: &LeadPlan, errors: &mut Vec<SpecError>) {
    let (Ok(harness), Ok(host)) = (
        env.registry.harness(&lead.harness),
        env.registry.host(&lead.host),
    ) else {
        return;
    };
    if harness.capabilities().reads_watch_lines {
        return;
    }
    let found = match &lead.session {
        Some((session, _)) => {
            teamwire_deliver::locate_session(env.platform, env.clock, session, harness, host)
                .map(|_| ())
                .map_err(|e| e.to_string())
        }
        None => Err("the lead's session id could not be read".to_owned()),
    };
    if let Err(why) = found {
        errors.push(SpecError::new(
            None,
            None,
            "lead_not_locatable",
            format!(
                "teamwire cannot find the lead's pane ({why}), and this lead cannot read \
                 `teamwire watch`, so results would reach nobody. Start the lead so its session \
                 runs inside its own pane: for Codex, run `codex --no-daemon`."
            ),
        ));
    }
}

fn session_missing(lead: &LeadPlan) -> bool {
    lead.session.is_none()
}

fn read_existing(
    env: &ValidateEnv<'_>,
    team: &TeamName,
) -> Result<Option<Existing>, Vec<SpecError>> {
    if !database_path(env.home, team).is_file() {
        return Ok(None);
    }
    let unreadable = |e: teamwire_store::StoreError| {
        vec![SpecError::new(
            None,
            Some("team"),
            "store_unreadable",
            format!("team {team}'s database could not be read: {e}"),
        )]
    };
    let store = Store::open_read_only(env.home, team).map_err(unreadable)?;
    let (row, members, started) = store
        .read(|tx| {
            let members = tx.members()?.all_or_corrupt()?;
            let started = tx
                .events_of_type(EventType::MemberStarted)?
                .all_or_corrupt()?;
            Ok((tx.team()?, members, started))
        })
        .map_err(unreadable)?;
    let Some(team_row) = row else {
        return Ok(None);
    };
    Ok(Some(Existing {
        team: team_row,
        members,
        ever_started: started.into_iter().filter_map(|e| e.role).collect(),
    }))
}

fn lead_plan(
    env: &ValidateEnv<'_>,
    existing: Option<&Existing>,
    session: Option<(SessionId, SessionKind)>,
) -> Result<LeadPlan, SpecError> {
    let (harness, host, session) = match existing {
        Some(e) => (
            e.team.lead_harness.clone(),
            e.team.lead_host.clone(),
            e.team.lead_session.clone().zip(e.team.lead_kind),
        ),
        None => (env.lead_harness.clone(), env.lead_host.clone(), session),
    };
    let route = if let Some(e) = existing {
        e.team.lead_route
    } else {
        let lead_host = env
            .registry
            .host(&host)
            .map_err(|e| SpecError::new(None, None, "unknown_host", e.to_string()))?;
        if lead_host.capabilities().can_type {
            LeadRoute::Sink
        } else {
            LeadRoute::Watch
        }
    };
    Ok(LeadPlan {
        harness,
        host,
        route,
        session,
    })
}

fn team_title(spec: &TeamSpec, existing: Option<&Existing>, errors: &mut Vec<SpecError>) {
    if existing.is_some() {
        return;
    }
    let title = plan::clean(&spec.title);
    if title.is_empty() {
        errors.push(SpecError::new(
            None,
            Some("title"),
            "title_missing",
            format!("the spec needs a title of at most {TITLE_MAX} characters"),
        ));
    } else if title.chars().count() > TITLE_MAX {
        errors.push(SpecError::new(
            None,
            Some("title"),
            "title_too_long",
            format!("team title {title:?} is over {TITLE_MAX} characters"),
        ));
    }
}

/// A refusal for each plugin key that is not `name@marketplace`. The id goes
/// into the harness's settings file, so a stray character must not reach it.
pub(crate) fn plugin_keys(
    plugins: &BTreeMap<String, bool>,
    role: Option<&str>,
    errors: &mut Vec<SpecError>,
) {
    let part_ok = |p: &str| {
        !p.is_empty()
            && p.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    };
    for key in plugins.keys() {
        if !key
            .split_once('@')
            .is_some_and(|(n, m)| part_ok(n) && part_ok(m))
        {
            errors.push(SpecError::new(
                role,
                Some("plugins"),
                "bad_plugin_id",
                format!(
                    "plugin id {key:?} must be name@marketplace, each part made of letters, digits, '.', '_' or '-'"
                ),
            ));
        }
    }
}

/// The nearest ancestor of `path` that exists, for a free-space question
/// about a directory that has not been made yet.
fn existing_ancestor(path: &Path) -> &Path {
    path.ancestors()
        .find(|p| p.exists())
        .unwrap_or_else(|| Path::new("/"))
}

fn disk_check(
    env: &ValidateEnv<'_>,
    members: &[crate::PlannedMember],
    errors: &mut Vec<SpecError>,
) {
    let mut paths = vec![env.home.to_path_buf()];
    paths.extend(members.iter().map(|m| m.cwd.clone()));
    let mut low: BTreeMap<String, u64> = BTreeMap::new();
    for path in &paths {
        let probe = existing_ancestor(path);
        match env.platform.free_bytes(probe) {
            Ok(free) if free < env.config.min_free_bytes => {
                low.insert(probe.display().to_string(), free);
            }
            _ => {}
        }
    }
    for (path, free) in low {
        errors.push(SpecError::new(
            None,
            None,
            "low_disk",
            format!(
                "{path} has {} MB free; a launch needs at least {} MB",
                free / (1024 * 1024),
                env.config.min_free_bytes / (1024 * 1024)
            ),
        ));
    }
}

/// The live agent names of `host`, asked once per host per validation.
pub(crate) fn live_names(host: &dyn Host) -> Result<Vec<String>, String> {
    host.live_agent_names(LIST_TIMEOUT)
        .map_err(|e| format!("the host's agent list could not be read: {e}"))
}

/// A member's host id, or the team's.
pub(crate) fn host_or<'a>(own: Option<&'a HostId>, team: &'a HostId) -> &'a HostId {
    own.unwrap_or(team)
}

/// True when a member row is one that an earlier launch left behind.
pub(crate) fn is_failed(m: &Member) -> bool {
    m.state == MemberState::Failed
}
