//! What every command shares at startup and on the way out: who the caller
//! says it is, the state directory, opening the team's store into a `Ctx`,
//! and printing and exit codes. A command that opens a team's store goes
//! through `with_ctx` or `with_read_ctx`, so the refusals read the same.

use std::io::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use teamwire_core::{Config, TeamName};
use teamwire_msg::CallerEnv;
use teamwire_store::{Ctx, NO_DATABASE_HINT, Store, StoreError};

use crate::Global;
use crate::recorded::Recorded;
use crate::wiring::Wiring;

/// Refused by a rule; nothing changed (docs/api/cli.md).
pub(crate) const REFUSED: u8 = 1;
/// Part of the work was done, or the check found a problem.
pub(crate) const PARTIAL: u8 = 2;
/// The store could not be read or written.
pub(crate) const STORE: u8 = 6;

/// What the process says about itself: the member's variables and the
/// claimed sender (ADR 0008). Every command reads the caller here, so an
/// empty variable counts as unset for all of them, as it does for the token.
pub(crate) fn caller_env(from: Option<String>) -> CallerEnv {
    let var = |name| std::env::var(name).ok().filter(|v: &String| !v.is_empty());
    CallerEnv {
        team: var("TEAMWIRE_TEAM"),
        role: var("TEAMWIRE_ROLE"),
        token: member_token(),
        token_file: var("TEAMWIRE_TOKEN_FILE"),
        user_key: var("TEAMWIRE_USER_KEY"),
        from,
        hints: Vec::new(),
    }
}

/// The member token the process carries. Every command reads it here, so
/// they agree on what counts as having one: the variable, else the file
/// `TEAMWIRE_TOKEN_FILE` names (ADR 0023 point 4).
pub(crate) fn member_token() -> Option<String> {
    teamwire_msg::caller_token(|name| std::env::var(name).ok())
}

/// Whether the process runs inside a member's session. The rule is
/// `CallerEnv::is_member_session`, the one the message crate's proofs use, so
/// the CLI and the proofs can never disagree about who is a member.
pub(crate) fn carries_member_identity() -> bool {
    caller_env(None).is_member_session()
}

/// A lead-only command (`close`, `watch`, a real `launch`) run from a member's
/// session is refused.
pub(crate) fn refuse_member(command: &str) -> Option<ExitCode> {
    carries_member_identity().then(|| {
        fail(
            command,
            "refused.identity: this command is for the lead; a member session cannot run it",
            REFUSED,
        )
    })
}

/// `--home`, else `$TEAMWIRE_HOME`, else `~/.teamwire`, made absolute. A
/// relative path is handed on to members, whose working directory is not
/// this one, so it must not stay relative. None when nothing names a
/// directory.
pub(crate) fn state_home(global: &Global) -> Option<PathBuf> {
    let Global { home } = global;
    let chosen = home
        .clone()
        .or_else(|| std::env::var_os("TEAMWIRE_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".teamwire")))?;
    std::path::absolute(chosen).ok()
}

/// `state_home`, or the refusal to print and exit with when nothing names
/// the state directory.
pub(crate) fn home_or_refuse(command: &str, global: &Global) -> Result<PathBuf, ExitCode> {
    state_home(global).ok_or_else(|| {
        fail(
            command,
            "no state directory: set --home, TEAMWIRE_HOME or HOME",
            REFUSED,
        )
    })
}

/// The refusal text for a team whose database file is missing.
pub(crate) fn no_database_text(team: &TeamName, path: &std::path::Path) -> String {
    format!(
        "no database for team {team} at {}. {NO_DATABASE_HINT}",
        path.display()
    )
}

/// Opens the team's store and runs `f` with a `Ctx` on it. A store that
/// cannot be opened is reported and turned into the exit code to return. A
/// failed write is recorded as `op` (`Recorded`).
pub(crate) fn with_ctx<T>(
    global: &Global,
    wiring: &Wiring,
    team: &TeamName,
    op: &'static str,
    f: impl FnOnce(&mut Ctx<'_>) -> T,
) -> Result<T, ExitCode> {
    with_ctx_using(global, wiring, team, op, Opening::default(), f)
}

/// How `with_ctx_using` opens the store and words its refusals. The
/// default is `with_ctx`'s.
#[derive(Default)]
pub(crate) struct Opening<'a> {
    /// The command a refusal names (`teamwire close: ...`); empty names none.
    pub(crate) command: &'a str,
    /// Open the database read-only, so the command cannot change anything.
    pub(crate) read_only: bool,
    /// Put before the refusal for a team with no database.
    pub(crate) refusal_prefix: &'a str,
    /// The settings the `Ctx` carries.
    pub(crate) config: Config,
}

/// `with_ctx`, opened and worded as `opening` says.
pub(crate) fn with_ctx_using<T>(
    global: &Global,
    wiring: &Wiring,
    team: &TeamName,
    op: &'static str,
    opening: Opening<'_>,
    f: impl FnOnce(&mut Ctx<'_>) -> T,
) -> Result<T, ExitCode> {
    let Opening {
        command,
        read_only,
        refusal_prefix,
        config,
    } = opening;
    let home = home_or_refuse(command, global)?;
    let opened = if read_only {
        Store::open_read_only(&home, team)
    } else {
        Store::open_existing(&home, team)
    };
    let mut store = match opened {
        Ok(store) => Recorded::new(store, &wiring.clock, op),
        Err(StoreError::NoDatabase { path }) => {
            let message = no_database_text(team, &path);
            return Err(fail(
                command,
                &format!("{refusal_prefix}{message}"),
                REFUSED,
            ));
        }
        Err(e) => return Err(fail(command, &e.to_string(), STORE)),
    };
    let mut ctx = Ctx {
        home: &home,
        store: &mut store,
        registry: &wiring.registry,
        platform: wiring.platform.as_ref(),
        clock: &wiring.clock,
        config: &config,
    };
    Ok(f(&mut ctx))
}

/// `with_ctx` for a command that only reads (`log`, `members`): the store is
/// opened read-only, never created or migrated, and a database of another
/// schema version is refused (WAT-11). A read never writes, not even a
/// refusal.
pub(crate) fn with_read_ctx<T>(
    global: &Global,
    wiring: &Wiring,
    team: &TeamName,
    f: impl FnOnce(&mut Ctx<'_>) -> T,
) -> Result<T, ExitCode> {
    let home = home_or_refuse("", global)?;
    let mut store = match Store::open_read_only(&home, team) {
        Ok(store) => store,
        Err(StoreError::NoDatabase { .. }) => {
            return Err(fail(
                "",
                &format!("no database for team {team} under {}", home.display()),
                REFUSED,
            ));
        }
        Err(e) => return Err(fail("", &e.to_string(), STORE)),
    };
    let config = Config::default();
    let mut ctx = Ctx {
        home: &home,
        store: &mut store,
        registry: &wiring.registry,
        platform: wiring.platform.as_ref(),
        clock: &wiring.clock,
        config: &config,
    };
    Ok(f(&mut ctx))
}

#[expect(clippy::print_stdout, reason = "the command's result is its output")]
pub(crate) fn say(text: &str) {
    print!("{text}");
}

/// `say` for one line. The line is flushed because another program may read it.
#[expect(clippy::print_stdout, reason = "the command's result is its output")]
pub(crate) fn say_line(text: &str) {
    println!("{text}");
    if let Err(e) = std::io::stdout().flush() {
        tracing::warn!(error = %e, "could not flush standard output");
    }
}

#[expect(clippy::print_stderr, reason = "the binary reports on stderr")]
pub(crate) fn warn(text: &str) {
    eprintln!("{text}");
}

#[expect(
    clippy::print_stderr,
    reason = "the binary reports a refusal on stderr"
)]
pub(crate) fn fail(command: &str, message: &str, code: u8) -> ExitCode {
    if command.is_empty() {
        eprintln!("teamwire: {message}");
    } else {
        eprintln!("teamwire {command}: {message}");
    }
    ExitCode::from(code)
}
