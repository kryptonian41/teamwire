//! Checking one member of a spec and working out how it will launch.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use teamwire_core::{
    Address, HarnessId, HostId, IdentityChannel, MemberKind, MemberSpec, MemberState, Mode, NoHost,
    Role, TeamSpec,
};

use crate::validate::{Existing, TITLE_MAX, host_or, is_failed, live_names};
use crate::{LeadPlan, PlannedMember, SpecError, ValidateEnv, WorktreePlan};

/// State shared while the members of one spec are checked.
pub(crate) struct Cx<'a> {
    pub(crate) env: &'a ValidateEnv<'a>,
    pub(crate) spec: &'a TeamSpec,
    pub(crate) existing: Option<&'a Existing>,
    pub(crate) lead: &'a LeadPlan,
    pub(crate) roles: BTreeSet<String>,
    pub(crate) titles: BTreeMap<String, Role>,
    pub(crate) slugs: BTreeMap<(PathBuf, String), String>,
    pub(crate) live: BTreeMap<HostId, Result<Vec<String>, String>>,
}

impl<'a> Cx<'a> {
    pub(crate) fn new(
        env: &'a ValidateEnv<'a>,
        spec: &'a TeamSpec,
        existing: Option<&'a Existing>,
        lead: &'a LeadPlan,
    ) -> Self {
        let titles = existing
            .into_iter()
            .flat_map(|e| &e.members)
            .map(|m| (m.title.to_lowercase(), m.role.clone()))
            .collect();
        Self {
            env,
            spec,
            existing,
            lead,
            roles: BTreeSet::new(),
            titles,
            slugs: BTreeMap::new(),
            live: BTreeMap::new(),
        }
    }
}

/// Collapses runs of white space and trims the ends.
pub(crate) fn clean(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn absolute(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().map_or_else(|_| path.to_path_buf(), |d| d.join(path))
    }
}

fn is_home(env: &ValidateEnv<'_>, path: &Path) -> bool {
    let Some(home) = &env.user_home else {
        return false;
    };
    match (path.canonicalize(), home.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => path == home,
    }
}

/// A branch name git accepts and that makes a safe directory name.
pub(crate) fn branch_ok(branch: &str) -> bool {
    !branch.is_empty()
        && !branch.starts_with(['-', '/', '.'])
        && !branch.ends_with(['/', '.'])
        && !branch.contains("..")
        && !branch.contains("//")
        && branch
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/'))
}

pub(crate) fn plan_member(
    cx: &mut Cx<'_>,
    child: &MemberSpec,
    errors: &mut Vec<SpecError>,
    warnings: &mut Vec<String>,
) -> Option<PlannedMember> {
    let before = errors.len();
    let role = child.role.as_str();
    let mut bad = |field: &str, code: &str, message: String| {
        errors.push(SpecError::new(Some(role), Some(field), code, message));
    };
    if !cx.roles.insert(role.to_owned()) {
        bad(
            "role",
            "duplicate_role",
            format!("role {role} appears twice in the spec"),
        );
    }
    let relaunch = role_state(cx, child, &mut bad);
    let title = clean(&child.title);
    if title.is_empty() {
        bad(
            "title",
            "title_missing",
            format!("role {role} needs a title of at most {TITLE_MAX} characters"),
        );
    } else if title.chars().count() > TITLE_MAX {
        bad(
            "title",
            "title_too_long",
            format!("title {title:?} is over {TITLE_MAX} characters"),
        );
    } else if cx
        .titles
        .get(&title.to_lowercase())
        .is_some_and(|other| *other != child.role)
        || cx
            .titles
            .insert(title.to_lowercase(), child.role.clone())
            .is_some_and(|o| o != child.role)
    {
        bad(
            "title",
            "title_repeated",
            format!("title {title:?} is already used in this team"),
        );
    }
    let brief = brief_text(child, &mut bad);
    let kind = child.kind.unwrap_or(match child.mode {
        Mode::Worktree => MemberKind::Implementer,
        Mode::Pane => MemberKind::Investigator,
    });
    if kind == MemberKind::Implementer && child.mode != Mode::Worktree {
        bad(
            "mode",
            "implementer_needs_worktree",
            format!(
                "role {role} is an implementer, so it needs mode worktree; two writers in one checkout corrupt each other"
            ),
        );
    }
    let harness_id = child
        .harness
        .clone()
        .or_else(|| cx.spec.harness.clone())
        .unwrap_or_else(|| cx.env.lead_harness.clone());
    let host_id = host_or(cx.spec.host.as_ref(), &cx.lead.host).clone();
    let channel = adapters(cx, role, &harness_id, &host_id, &mut bad);
    let (cwd, worktree) = location(cx, child, &mut bad, warnings);
    let address = Address::of(&cx.spec.team, &child.role);
    live_name_check(cx, role, &address, &host_id, &mut bad, warnings);
    if errors.len() > before {
        return None;
    }
    Some(PlannedMember {
        spec: MemberSpec {
            title,
            ..child.clone()
        },
        kind,
        harness: harness_id,
        host: host_id,
        address,
        channel: channel?,
        cwd: cwd?,
        worktree,
        relaunch,
        deadline_s: child.deadline_s.unwrap_or(cx.env.config.default_deadline_s),
        brief: brief?,
    })
}

pub(crate) type Bad<'a> = dyn FnMut(&str, &str, String) + 'a;

/// LCH-5: a role is never reused, except one that failed before it started.
fn role_state(cx: &Cx<'_>, child: &MemberSpec, bad: &mut Bad<'_>) -> bool {
    let Some(old) = cx
        .existing
        .and_then(|e| e.members.iter().find(|m| m.role == child.role))
    else {
        return false;
    };
    let started = cx
        .existing
        .is_some_and(|e| e.ever_started.contains(&child.role));
    if is_failed(old) && !started {
        return true;
    }
    let why = match old.state {
        MemberState::Removed => "was removed",
        MemberState::Failed => "started once and then failed",
        MemberState::Started => "is running",
        MemberState::Creating => "was left half-made; close it first",
    };
    bad(
        "role",
        "role_reused",
        format!(
            "role {} {why}; a role is never reused. Pick a new name.",
            child.role
        ),
    );
    false
}

fn brief_text(child: &MemberSpec, bad: &mut Bad<'_>) -> Option<String> {
    match (&child.task, &child.task_file) {
        (Some(task), None) if !task.trim().is_empty() => Some(task.clone()),
        (None, Some(file)) => {
            let file = absolute(file);
            match std::fs::read_to_string(&file) {
                Ok(text) if !text.trim().is_empty() => Some(text),
                Ok(_) => {
                    bad(
                        "task_file",
                        "bad_task",
                        format!("task_file {} is empty", file.display()),
                    );
                    None
                }
                Err(e) => {
                    bad(
                        "task_file",
                        "bad_task",
                        format!("task_file {} cannot be read: {e}", file.display()),
                    );
                    None
                }
            }
        }
        _ => {
            bad(
                "task",
                "bad_task",
                "give exactly one non-empty task or task_file".to_owned(),
            );
            None
        }
    }
}

/// Finds the adapters and the identity channel (LCH-20).
fn adapters(
    cx: &Cx<'_>,
    role: &str,
    harness_id: &HarnessId,
    host_id: &HostId,
    bad: &mut Bad<'_>,
) -> Option<IdentityChannel> {
    let harness = cx.env.registry.harness(harness_id);
    let host = cx.env.registry.host(host_id);
    if let Err(e) = &harness {
        bad("harness", "unknown_harness", e.to_string());
    }
    if let Err(e) = &host {
        bad("host", "unknown_host", e.to_string());
    }
    let (harness, host) = (harness.ok()?, host.ok()?);
    if host_id.as_str() == NoHost::ID {
        bad(
            "host",
            "no_host",
            format!("role {role}: host none cannot start a member; v1 needs herdr or tmux"),
        );
        return None;
    }
    if harness.capabilities().session_env_via_config {
        Some(IdentityChannel::HarnessConfig)
    } else if host.capabilities().sets_env_at_spawn {
        Some(IdentityChannel::HostEnv)
    } else {
        bad(
            "identity",
            "no_identity_channel",
            format!(
                "role {role}: neither harness {harness_id} (per-session config) nor host {host_id} (spawn environment) can give the member its identity"
            ),
        );
        None
    }
}

fn live_name_check(
    cx: &mut Cx<'_>,
    role: &str,
    address: &Address,
    host_id: &HostId,
    bad: &mut Bad<'_>,
    warnings: &mut Vec<String>,
) {
    let Ok(host) = cx.env.registry.host(host_id) else {
        return;
    };
    if host_id.as_str() == NoHost::ID {
        return;
    }
    let names = cx
        .live
        .entry(host_id.clone())
        .or_insert_with(|| live_names(host));
    match names {
        Ok(names) if names.iter().any(|n| n == address.as_str()) => bad(
            "role",
            "name_taken",
            format!("role {role}: an agent named {address} is already live in {host_id}"),
        ),
        Ok(_) => {}
        // An unreadable list is not a name that is taken. The host itself
        // refuses a second live agent of the same name when it starts one.
        Err(e) => {
            let text =
                format!("{e}; a name already live in {host_id} is caught when the agent starts");
            if !warnings.contains(&text) {
                warnings.push(text);
            }
        }
    }
}

/// The directory the agent starts in, and the worktree to make for it.
fn location(
    cx: &mut Cx<'_>,
    child: &MemberSpec,
    bad: &mut Bad<'_>,
    warnings: &mut Vec<String>,
) -> (Option<PathBuf>, Option<WorktreePlan>) {
    let role = child.role.as_str();
    let (field, dir) = match child.mode {
        Mode::Worktree => ("repo", child.repo.as_deref()),
        Mode::Pane => ("cwd", child.cwd.as_deref()),
    };
    let Some(dir) = dir else {
        bad(field, "missing_path", format!("role {role} needs {field}"));
        return (None, None);
    };
    if !dir.is_absolute() {
        bad(
            field,
            "bad_path",
            format!("{field} {} must be an absolute path", dir.display()),
        );
        return (None, None);
    }
    if !dir.is_dir() {
        bad(
            field,
            "bad_path",
            format!("{field} {} is not a directory", dir.display()),
        );
        return (None, None);
    }
    if is_home(cx.env, dir) {
        bad(
            field,
            "home_directory",
            format!(
                "role {role}: {field} is the home directory; name the project directory instead"
            ),
        );
        return (None, None);
    }
    if child.mode == Mode::Pane {
        return (Some(dir.to_path_buf()), None);
    }
    let plan = crate::plan_worktree::worktree_plan(cx, child, dir, bad, warnings);
    (plan.as_ref().map(|w| w.path.clone()), plan)
}
