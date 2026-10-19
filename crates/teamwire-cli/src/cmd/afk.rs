//! `teamwire afk start`: open an AFK run. The run is an ordinary team whose
//! row carries the end of its budget; members join it with
//! `teamwire launch --add`.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use teamwire_core::{Clock, Config, SessionId, TeamName, TeamState, Timestamp};
use teamwire_launch::{
    LeadPlan, SpecError, ValidateEnv, check_title, ensure_hold, lead_process, new_team_lead,
};
use teamwire_store::{Ctx, NewTeam, Store, StoreError, database_path, team_names};

use crate::cmd::doctor::{self, Host, Level};
use crate::cmd::install::wanted_entry;
use crate::cmd::lead_setup_check;
use crate::cmd::send::{REFUSED, STORE, fail, home_or_refuse, refuse_member, say};
use crate::recorded::Recorded;
use crate::wiring::{Wiring, claude_harness_id};
use crate::{AfkArgs, AfkCommand, AfkStartArgs, Global};

const COMMAND: &str = "afk start";

pub(crate) fn run(args: &AfkArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    match &args.command {
        AfkCommand::Start(a) => start(a, global, wiring),
    }
}

fn refuse(code: &str, message: &str) -> ExitCode {
    fail(COMMAND, &format!("refused: {code}: {message}"), REFUSED)
}

fn refuse_spec(e: &SpecError) -> ExitCode {
    refuse(&e.code, &e.message)
}

/// The seven steps of the design, in order. The first that fails ends the
/// command, and nothing is created before the fourth.
fn start(args: &AfkStartArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    // 1. A lead session that does not already lead an open AFK run.
    if let Some(code) = refuse_member(COMMAND) {
        return code;
    }
    // AFK-2: only a Claude Code lead, whose pane the run can be proven to reach.
    if wiring.lead_harness != claude_harness_id() {
        return refuse(
            "unsupported_lead_harness",
            "an AFK run starts only from a Claude Code lead",
        );
    }
    let home = match home_or_refuse(COMMAND, global) {
        Ok(home) => home,
        Err(code) => return code,
    };
    let config = Config::default();
    let env = ValidateEnv {
        home: &home,
        registry: &wiring.registry,
        platform: &*wiring.platform,
        clock: &wiring.clock,
        config: &config,
        lead_harness: wiring.lead_harness.clone(),
        lead_host: wiring.lead_host.clone(),
        user_home: std::env::var_os("HOME").map(PathBuf::from),
        teamwire_bin: std::env::current_exe().unwrap_or_else(|_| PathBuf::from("teamwire")),
    };
    let lead = match new_team_lead(&env) {
        Ok(lead) => lead,
        Err(e) => return refuse_spec(&e),
    };
    if let Some((session, _)) = &lead.session
        && let Some(running) = open_afk_team_of(&home, session)
    {
        return refuse(
            "afk_already_running",
            &format!(
                "this session already leads the open AFK run {running}; close it first with teamwire close {running}"
            ),
        );
    }

    // 2. The name, the title, and the task file. The file is read now, so a
    // missing one leaves no team behind.
    let team = match TeamName::parse(&args.team) {
        Ok(team) => team,
        Err(e) => return refuse("bad_name", &e.to_string()),
    };
    let title = match check_title(&args.title) {
        Ok(title) => title,
        Err(e) => return refuse_spec(&e),
    };
    if database_path(&home, &team).is_file() {
        return refuse(
            "team_exists",
            &format!("team {team} already exists; pick another name"),
        );
    }
    let task = match std::fs::read(&args.task) {
        Ok(bytes) => bytes,
        Err(e) => {
            return refuse(
                "bad_task",
                &format!("the task file {} cannot be read: {e}", args.task.display()),
            );
        }
    };

    // 3. The checks of `teamwire doctor`.
    if let Err(code) = doctor_gate(&home, global, wiring) {
        return code;
    }

    // The hold is step 6, but a run that cannot take it must not start: find
    // the lead's process now, while nothing exists yet.
    if wiring.platform.sleep_hold().is_some() {
        let mut notes = Vec::new();
        let found = lead
            .session
            .as_ref()
            .and_then(|(s, _)| lead_process(&wiring.registry, &lead.harness, s, &mut notes));
        if found.is_none() {
            return refuse(
                "no_lead_process",
                &format!(
                    "no sleep hold can be taken: {}",
                    notes
                        .first()
                        .map_or("the lead's process was not found", String::as_str)
                ),
            );
        }
    }

    // 4 to 6. The team, the task copy and the hold.
    let now = wiring.clock.now();
    let Some(ends) = now.checked_add(args.budget) else {
        return refuse("bad_budget", "the budget ends beyond the last date");
    };
    let mut store = match Store::open(&home, &team) {
        Ok(store) => Recorded::new(store, &wiring.clock, "afk"),
        Err(e) => {
            return fail(
                COMMAND,
                &format!("the store could not be opened: {e}"),
                STORE,
            );
        }
    };
    let mut ctx = Ctx {
        home: &home,
        store: &mut store,
        registry: &wiring.registry,
        platform: &*wiring.platform,
        clock: &wiring.clock as &dyn Clock,
        config: &config,
    };
    let new = New {
        team: &team,
        title: &title,
        lead: &lead,
        task: &task,
        ends,
    };
    let warnings = match create(&mut ctx, &new) {
        Ok(warnings) => warnings,
        Err(Failed::Taken) => {
            return refuse(
                "team_exists",
                &format!("team {team} already exists; pick another name"),
            );
        }
        Err(Failed::Other(e)) => {
            return fail(
                COMMAND,
                &format!("{e}; close the team with teamwire close {team} and start again"),
                STORE,
            );
        }
    };
    report(&home, &team, ends, args.budget, &warnings);
    ExitCode::SUCCESS
}

/// Runs doctor's checks. A `warn` is printed; a `FAIL` refuses.
#[expect(clippy::print_stderr, reason = "the binary reports on stderr")]
fn doctor_gate(home: &std::path::Path, global: &Global, wiring: &Wiring) -> Result<(), ExitCode> {
    let repo = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let lead_entry = wanted_entry(global);
    let pick = doctor::pick_host(
        std::env::var("HERDR_ENV").ok().as_deref(),
        std::env::var("TMUX").ok().as_deref(),
        &|pid| wiring.platform.parent_of(pid),
    );
    let inputs = doctor::Inputs {
        host: match pick.id {
            "herdr" => Host::Herdr,
            "tmux" => Host::Tmux,
            _ => Host::None,
        },
        home: home.to_path_buf(),
        platform: wiring.platform.as_ref(),
        registry: &wiring.registry,
        clock: &wiring.clock,
        path_var: std::env::var_os("PATH"),
        claude: wiring.claude.clone(),
        codex: wiring.codex_setup(),
        running_a_build: lead_entry
            .as_ref()
            .is_ok_and(|e| lead_setup_check::is_build_output(std::path::Path::new(&e.command))),
        lead_entry,
        repo,
        env_names: std::env::vars_os()
            .filter(|(_, v)| !v.is_empty())
            .filter_map(|(k, _)| k.into_string().ok())
            .collect(),
    };
    let checks = doctor::diagnose(&inputs);
    let only = |level| {
        let picked: Vec<_> = checks
            .iter()
            .filter(|c| c.level == level)
            .cloned()
            .collect();
        doctor::render(&picked)
    };
    eprint!("{}", only(Level::Warn));
    let failed = only(Level::Fail);
    if failed.is_empty() {
        return Ok(());
    }
    eprint!("{failed}");
    Err(refuse(
        "doctor_failed",
        "teamwire doctor found a problem that would break a launch; fix it and start again",
    ))
}

/// The name of an open AFK team that `session` leads, if any. A team that
/// cannot be read is skipped, as the Stop hook skips it.
#[expect(
    clippy::redundant_closure_for_method_calls,
    reason = "`ReadTx::team` as a path does not infer the transaction's lifetime for `Store::read`"
)]
fn open_afk_team_of(home: &std::path::Path, session: &SessionId) -> Option<TeamName> {
    team_names(home).ok()?.into_iter().find(|name| {
        Store::open_read_only(home, name)
            .ok()
            .and_then(|store| store.read(|tx| tx.team()).ok().flatten())
            .is_some_and(|row| {
                row.state == TeamState::Open
                    && row.afk_budget_ends_at.is_some()
                    && row.lead_session.as_ref() == Some(session)
            })
    })
}

struct New<'a> {
    team: &'a TeamName,
    title: &'a str,
    lead: &'a LeadPlan,
    task: &'a [u8],
    ends: Timestamp,
}

/// Why `create` stopped.
#[derive(Debug)]
enum Failed {
    /// A team of that name already exists; nothing of this run was made.
    Taken,
    /// The team may exist by now, so the lead has to close it.
    Other(String),
}

/// Steps 4 to 6: the team row with the budget end, `afk/task.md`, and the
/// sleep hold. Returns the hold's warnings.
fn create(ctx: &mut Ctx<'_>, new: &New<'_>) -> Result<Vec<String>, Failed> {
    let at = ctx.clock.now();
    let row = NewTeam {
        name: new.team.clone(),
        title: new.title.to_owned(),
        lead_harness: new.lead.harness.clone(),
        lead_host: new.lead.host.clone(),
        lead_route: new.lead.route,
        lead_session: new.lead.session.clone(),
    };
    // The folder lets `reopen` resume the lead from where it runs.
    let cwd = new
        .lead
        .session
        .as_ref()
        .and_then(|(session, _)| {
            ctx.registry
                .harness(&new.lead.harness)
                .ok()?
                .session_cwd(session)
                .ok()?
        })
        .map(|p| p.display().to_string());
    let extra = cwd.map_or(
        serde_json::json!({}),
        |cwd| serde_json::json!({ "lead_cwd": cwd }),
    );
    // The team row has a fixed primary key, so of two runs with one name the
    // insert of the loser breaks a constraint, and the winner's files stay.
    ctx.store
        .write(|tx| tx.create_afk_team(at, &row, new.ends, &extra).map(|_| ()))
        .map_err(|e| match e {
            StoreError::Constraint(_) => Failed::Taken,
            e => Failed::Other(format!("the team could not be recorded: {e}")),
        })?;
    let dir = afk_dir(ctx.home, new.team);
    std::fs::create_dir_all(&dir)
        .map_err(|e| Failed::Other(format!("{} could not be made: {e}", dir.display())))?;
    ctx.platform
        .write_atomic(&dir.join("task.md"), new.task)
        .map_err(|e| Failed::Other(format!("afk/task.md could not be written: {e}")))?;
    let mut warnings = Vec::new();
    let lead = (&new.lead.harness, new.lead.session.as_ref().map(|(s, _)| s));
    ensure_hold(ctx, new.team, lead, &mut warnings);
    // A platform with no sleep hold at all only warns; anywhere else, a run
    // without a recorded hold would let the machine sleep under it.
    #[expect(
        clippy::redundant_closure_for_method_calls,
        reason = "`ReadTx::team` as a path does not infer the transaction's lifetime for `Store::read`"
    )]
    let held = ctx
        .store
        .read(|tx| tx.team())
        .ok()
        .flatten()
        .is_some_and(|t| t.sleep_hold.is_some());
    if !held && ctx.platform.sleep_hold().is_some() {
        return Err(Failed::Other(format!(
            "the sleep hold could not be taken: {}",
            warnings.join("; ")
        )));
    }
    Ok(warnings)
}

fn afk_dir(home: &std::path::Path, team: &TeamName) -> PathBuf {
    home.join("teams").join(team.as_str()).join("afk")
}

#[expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "the binary reports on stdout and stderr"
)]
fn report(
    home: &std::path::Path,
    team: &TeamName,
    ends: Timestamp,
    budget: Duration,
    warnings: &[String],
) {
    for w in warnings {
        eprintln!("warning: {w}");
    }
    let dir = afk_dir(home, team);
    say(&format!("afk run {team} is open, with no members yet\n"));
    say(&format!("folder: {}\n", dir.display()));
    say(&format!("report: {}\n", dir.join("REPORT.md").display()));
    say(&format!(
        "budget: new implementers stop at {ends} ({}m from now)\n",
        budget.as_secs() / 60
    ));
    say("keep the lid open and the machine on power: a closed lid still sleeps a Mac\n");
    println!("next: teamwire launch --add --spec FILE");
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::redundant_closure_for_method_calls,
        reason = "`ReadTx::team` as a path does not infer the transaction's lifetime for `Store::read`"
    )]

    use std::sync::Arc;

    use teamwire_core::{
        HarnessId, HostId, LeadRoute, Liveness, Pid, PlaceState, Registry, SessionKind,
    };
    use teamwire_fakes::{FakeClock, FakeHarness, FakeHost, FakePlatform, TestHome};

    use super::*;

    fn session() -> SessionId {
        SessionId::parse("lead-session").unwrap()
    }

    fn lead() -> LeadPlan {
        LeadPlan {
            harness: HarnessId::from_static(FakeHarness::ID),
            host: HostId::from_static(FakeHost::ID),
            route: LeadRoute::Sink,
            session: Some((session(), SessionKind::Interactive)),
        }
    }

    /// Runs `create` for team `run` on fakes where the lead is process 4242.
    fn created(task: &[u8]) -> (TestHome, Store, FakePlatform) {
        let (home, store, platform, result) = run_create(FakePlatform::new(), task);
        let warnings = result.unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        (home, store, platform)
    }

    fn run_create(
        platform: FakePlatform,
        task: &[u8],
    ) -> (TestHome, Store, FakePlatform, Result<Vec<String>, Failed>) {
        let home = TestHome::new().unwrap();
        let harness = Arc::new(FakeHarness::new());
        harness.set_session_cwd(session(), "/work/lead");
        harness.add_session(
            session(),
            vec![Pid::new(4242)],
            Liveness::Alive(PlaceState::Working),
        );
        let mut registry = Registry::new();
        registry.add_harness(harness).unwrap();
        registry.add_host(Arc::new(FakeHost::new())).unwrap();
        let clock = FakeClock::new();
        let team = TeamName::parse("run").unwrap();
        let mut store = Store::open(home.path(), &team).unwrap();
        let lead = lead();
        let ends = clock
            .now()
            .checked_add(Duration::from_secs(8 * 3600))
            .unwrap();
        let config = Config::default();
        let result = create(
            &mut Ctx {
                home: home.path(),
                store: &mut store,
                registry: &registry,
                platform: &platform,
                clock: &clock,
                config: &config,
            },
            &New {
                team: &team,
                title: "Overnight run",
                lead: &lead,
                task,
                ends,
            },
        );
        (home, store, platform, result)
    }

    #[test]
    fn create_records_the_budget_the_task_copy_and_the_hold() {
        let task = b"build it\n\xff not utf-8\r\n";
        let (home, store, platform) = created(task);
        let row = store.read(|tx| tx.team()).unwrap().unwrap();
        assert_eq!(row.title, "Overnight run");
        assert_eq!(row.lead_session, Some(session()));
        let ends = row.afk_budget_ends_at.unwrap();
        assert_eq!(
            ends.saturating_since(row.created_at),
            Duration::from_secs(8 * 3600)
        );
        let copy = std::fs::read(home.path().join("teams/run/afk/task.md")).unwrap();
        assert_eq!(copy, task);
        let hold = row.sleep_hold.unwrap();
        assert!(hold.owned);
        assert_eq!(platform.hold_events().len(), 1);
        let events = store
            .read(|tx| {
                tx.events_of_type(teamwire_core::EventType::TeamCreated)?
                    .all_or_corrupt()
            })
            .unwrap();
        let data = &events.first().unwrap().data;
        assert_eq!(data.get("lead_cwd").unwrap(), "/work/lead");
        assert_eq!(data.get("afk_budget_ends_at").unwrap(), &ends.to_string());
    }

    #[test]
    fn a_hold_that_cannot_be_taken_is_an_error_after_the_team_exists() {
        let platform = FakePlatform::new();
        platform.fail_hold_start(Some(teamwire_core::PlatformError::new(
            teamwire_core::PlatformErrorCode::Io,
            "no caffeinate",
        )));
        let (_home, store, _, result) = run_create(platform, b"x");
        let Failed::Other(why) = result.unwrap_err() else {
            panic!("the hold failure is not a taken name");
        };
        assert!(why.contains("sleep hold could not be taken"), "{why}");
        assert!(why.contains("no caffeinate"), "{why}");
        let row = store.read(|tx| tx.team()).unwrap().unwrap();
        assert_eq!(row.sleep_hold, None);
    }

    #[test]
    fn a_second_create_on_one_store_reports_the_name_taken_and_keeps_the_first_task() {
        let (home, mut store, platform) = created(b"first");
        let clock = FakeClock::new();
        let registry = Registry::new();
        let config = Config::default();
        let team = TeamName::parse("run").unwrap();
        let lead = lead();
        let result = create(
            &mut Ctx {
                home: home.path(),
                store: &mut store,
                registry: &registry,
                platform: &platform,
                clock: &clock,
                config: &config,
            },
            &New {
                team: &team,
                title: "Again",
                lead: &lead,
                task: b"second",
                ends: clock.now(),
            },
        );
        assert!(matches!(result, Err(Failed::Taken)), "{result:?}");
        let copy = std::fs::read(home.path().join("teams/run/afk/task.md")).unwrap();
        assert_eq!(copy, b"first");
    }

    #[test]
    fn only_an_open_afk_team_of_the_same_session_counts_as_running() {
        let (home, mut store, _) = created(b"x");
        let other = SessionId::parse("other-session").unwrap();
        assert_eq!(
            open_afk_team_of(home.path(), &session()),
            Some(TeamName::parse("run").unwrap())
        );
        assert_eq!(open_afk_team_of(home.path(), &other), None);
        let at = FakeClock::new().now();
        store
            .write(|tx| tx.begin_close(at, teamwire_core::Pid::new(1)).map(|_| ()))
            .unwrap();
        assert_eq!(open_afk_team_of(home.path(), &session()), None);
    }
}
