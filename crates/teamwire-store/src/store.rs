//! Opening the database: pragmas, the forward migration, the refusal of a
//! newer or foreign file, and the two transaction entry points.

use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OpenFlags, TransactionBehavior};
use teamwire_core::TeamName;

use crate::error::StoreError;
use crate::older::show_missing_columns;
use crate::tx::{ReadTx, Tx};
use crate::{SCHEMA_V1_SQL, SCHEMA_V2_SQL, SCHEMA_V3_SQL, SCHEMA_VERSION};

/// `PRAGMA application_id` of every teamwire database: the ASCII bytes
/// `tWIR`. A file with another id (including 0 with tables in it) is not ours.
pub const APPLICATION_ID: i32 = 0x7457_4952;

/// The build identity recorded in each event's `writer_version`, unless a
/// caller overrides it with `Store::with_writer_version`: the crate version,
/// `+`, and the first 12 hex digits of the git commit (`unknown` when the
/// build could not read one). Compare it as text, never as semver.
pub const WRITER_VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    "+",
    env!("TEAMWIRE_BUILD_COMMIT")
);

/// How long a connection waits for another writer before `StoreError::Busy`.
pub const BUSY_TIMEOUT_MS: u32 = 5000;

/// `MIGRATIONS[i]` takes a database from schema version `i` to `i + 1`.
const MIGRATIONS: [&str; SCHEMA_VERSION as usize] = [SCHEMA_V1_SQL, SCHEMA_V2_SQL, SCHEMA_V3_SQL];

/// One team's database. One `rusqlite::Connection`, never shared across
/// processes.
#[derive(Debug)]
pub struct Store {
    conn: Connection,
    writer_version: String,
    read_only: bool,
    /// Set when a read-only open found no `-wal` or `-shm` file, so that
    /// opening created them: closing the store then removes them again.
    cleanup: Option<PathBuf>,
}

/// The `-wal` and `-shm` files `SQLite` keeps beside a WAL database.
fn side_files(path: &Path) -> [PathBuf; 2] {
    ["-wal", "-shm"].map(|suffix| {
        let mut name = path.as_os_str().to_owned();
        name.push(suffix);
        PathBuf::from(name)
    })
}

impl Drop for Store {
    /// A read-only connection cannot remove the `-wal` and `-shm` files that
    /// opening a WAL database created, and a status run must leave the
    /// directory as it found it. Once this connection is closed, a short
    /// read-write open and close makes `SQLite` do what it does for any last
    /// connection: check the (empty) log and delete both files. It changes no
    /// page, and does nothing while another process still has the database open.
    fn drop(&mut self) {
        let Some(path) = self.cleanup.take() else {
            return;
        };
        if let Ok(placeholder) = Connection::open_in_memory() {
            drop(std::mem::replace(&mut self.conn, placeholder));
        }
        remove_side_files(&path);
    }
}

/// Makes a short read-write open and close, which deletes the `-wal` and
/// `-shm` files once no other connection holds the database.
fn remove_side_files(path: &Path) {
    if let Ok(conn) = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        // The connection only opens the log when it reads, and only a
        // connection that opened it cleans up on close.
        let _version: Option<i64> = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).ok();
        drop(conn);
    }
}

/// What a database file is, judged from its header and table list alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Found {
    /// No application id, no version, no tables: new or empty.
    Fresh,
    /// A teamwire database at this schema version.
    Ours(u32),
    Foreign,
}

/// The database file of `team`: `<home>/teams/<team>/team.db`.
pub fn database_path(home: &Path, team: &TeamName) -> PathBuf {
    home.join("teams").join(team.as_str()).join("team.db")
}

/// Every team under `home`: each directory of `<home>/teams` that names a
/// valid team and holds a database, sorted by name. No `teams` directory is
/// no teams; an entry that cannot be read is an error, and the caller
/// decides whether one unreadable entry hides the rest.
pub fn team_names(home: &Path) -> Result<Vec<TeamName>, StoreError> {
    let Ok(entries) = std::fs::read_dir(home.join("teams")) else {
        return Ok(Vec::new());
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry?;
        if let Some(name) = entry
            .file_name()
            .to_str()
            .and_then(|n| TeamName::parse(n).ok())
            && database_path(home, &name).is_file()
        {
            names.push(name);
        }
    }
    names.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    Ok(names)
}

impl Store {
    /// Creates the directory and database if missing; applies pragmas;
    /// migrates forward; refuses a newer schema (`StoreError::SchemaNewer`).
    pub fn open(home: &Path, team: &TeamName) -> Result<Self, StoreError> {
        let path = database_path(home, team);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        Self::open_read_write(&path, OpenFlags::SQLITE_OPEN_CREATE)
    }

    /// Like `open`, but the database must already exist.
    pub fn open_existing(home: &Path, team: &TeamName) -> Result<Self, StoreError> {
        let path = database_path(home, team);
        if !path.is_file() {
            return Err(StoreError::NoDatabase { path });
        }
        Self::open_read_write(&path, OpenFlags::empty())
    }

    /// For commands that only read (status, log, members, metrics, export,
    /// doctor): `mode=ro`, never creates, never migrates. A database at an
    /// older schema is read as it is, with each column a later version added
    /// read as NULL. A newer schema is refused with `SchemaNewer`, and an
    /// empty file with `SchemaOlder`. Nothing on disk changes (WAT-11).
    pub fn open_read_only(home: &Path, team: &TeamName) -> Result<Self, StoreError> {
        Self::open_read_only_at(&database_path(home, team))
    }

    /// `open_read_only` for a database file named by its path, such as an
    /// archived team's `team.db` (cross-archive metrics, the archive check).
    pub fn open_read_only_at(path: &Path) -> Result<Self, StoreError> {
        if !path.is_file() {
            return Err(StoreError::NoDatabase {
                path: path.to_path_buf(),
            });
        }
        let no_side_files = side_files(path).iter().all(|p| !p.exists());
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        let checked = (|| {
            conn.busy_timeout(Duration::from_millis(u64::from(BUSY_TIMEOUT_MS)))?;
            match found(&conn)? {
                Found::Foreign => Err(StoreError::NotTeamwire),
                Found::Fresh => Err(StoreError::SchemaOlder {
                    db: 0,
                    binary: SCHEMA_VERSION,
                }),
                Found::Ours(v) => {
                    check_version_not_newer(v)?;
                    show_missing_columns(&conn, v)
                }
            }
        })();
        if let Err(refused) = checked {
            // No `Store` exists to clean up after this open, so the files it
            // created are removed here (WAT-11).
            drop(conn);
            if no_side_files {
                remove_side_files(path);
            }
            return Err(refused);
        }
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA query_only = ON;")?;
        Ok(Self {
            conn,
            writer_version: WRITER_VERSION.to_owned(),
            read_only: true,
            cleanup: no_side_files.then(|| path.to_path_buf()),
        })
    }

    fn open_read_write(path: &Path, create: OpenFlags) -> Result<Self, StoreError> {
        let mut conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX | create,
        )?;
        conn.busy_timeout(Duration::from_millis(u64::from(BUSY_TIMEOUT_MS)))?;
        // Look before touching anything: switching a foreign or newer file to
        // WAL would already be a write.
        match found(&conn)? {
            Found::Foreign => return Err(StoreError::NotTeamwire),
            Found::Ours(v) if v > SCHEMA_VERSION => {
                return Err(StoreError::SchemaNewer {
                    db: v,
                    binary: SCHEMA_VERSION,
                });
            }
            Found::Fresh | Found::Ours(_) => {}
        }
        apply_pragmas(&conn)?;
        migrate(&mut conn)?;
        Ok(Self {
            conn,
            writer_version: WRITER_VERSION.to_owned(),
            read_only: false,
            cleanup: None,
        })
    }

    /// Sets the version recorded in the `writer_version` of every event this
    /// store writes. Tests use it to play two binaries against one database.
    #[must_use]
    pub fn with_writer_version(mut self, version: impl Into<String>) -> Self {
        self.writer_version = version.into();
        self
    }

    /// `PRAGMA user_version`, read fresh from the file. The watch checks it on
    /// every poll to notice that a newer binary migrated (ADR 0001).
    pub fn schema_version(&self) -> Result<u32, StoreError> {
        let v: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        u32::try_from(v).map_err(|_| StoreError::Corrupt(format!("user_version {v}")))
    }

    /// Runs `f` inside BEGIN IMMEDIATE; commits on Ok, rolls back on Err.
    pub fn write<T>(
        &mut self,
        f: impl FnOnce(&Tx<'_>) -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        if self.read_only {
            return Err(StoreError::Io("the store was opened read-only".to_owned()));
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let out = f(&Tx::new(&tx, &self.writer_version))?;
        tx.commit()?;
        Ok(out)
    }

    /// Runs `f` inside one read transaction: it sees a single snapshot, and
    /// a writer in another connection is never blocked by it. Writing through
    /// the connection fails.
    pub fn read<T>(
        &self,
        f: impl FnOnce(&ReadTx<'_>) -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        if !self.read_only {
            self.conn.pragma_update(None, "query_only", true)?;
        }
        let result = self.conn.unchecked_transaction().map_err(StoreError::from);
        let out = match result {
            Ok(tx) => f(&ReadTx::new(&tx)).and_then(|out| {
                tx.commit()?;
                Ok(out)
            }),
            Err(e) => Err(e),
        };
        if !self.read_only {
            self.conn.pragma_update(None, "query_only", false)?;
        }
        out
    }

    /// Copies the whole database to a new file with `SQLite`'s backup API,
    /// safe while other processes write. Refuses to overwrite a file.
    pub fn backup_to(&self, path: &Path) -> Result<(), StoreError> {
        if path.exists() {
            return Err(StoreError::Io(format!(
                "backup target {} already exists",
                path.display()
            )));
        }
        let mut dst = Connection::open(path)?;
        let backup = rusqlite::backup::Backup::new(&self.conn, &mut dst)?;
        backup.run_to_completion(64, Duration::from_millis(20), None)?;
        Ok(())
    }

    /// `PRAGMA quick_check`: `ok`, or the problems it found.
    pub fn quick_check(&self) -> Result<String, StoreError> {
        self.pragma_check("quick_check")
    }

    /// `PRAGMA integrity_check`: `ok`, or the problems it found.
    pub fn integrity_check(&self) -> Result<String, StoreError> {
        self.pragma_check("integrity_check")
    }

    fn pragma_check(&self, pragma: &str) -> Result<String, StoreError> {
        let mut stmt = self.conn.prepare(&format!("PRAGMA {pragma}"))?;
        let lines = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<String>, _>>()?;
        Ok(lines.join("\n"))
    }
}

/// Reads the header and table list. A file that is not a database at all is
/// `Foreign`, not corrupt: nothing says it was ever ours.
fn found(conn: &Connection) -> Result<Found, StoreError> {
    let read = || -> rusqlite::Result<(i64, i64, i64)> {
        let app = conn.query_row("PRAGMA application_id", [], |r| r.get(0))?;
        let version = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        let tables = conn.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite\\_%' ESCAPE '\\'",
            [],
            |r| r.get(0),
        )?;
        Ok((app, version, tables))
    };
    let (app, version, tables) = match read() {
        Ok(values) => values,
        Err(rusqlite::Error::SqliteFailure(e, _))
            if e.code == rusqlite::ErrorCode::NotADatabase =>
        {
            return Ok(Found::Foreign);
        }
        Err(e) => return Err(e.into()),
    };
    if app == i64::from(APPLICATION_ID) {
        return match u32::try_from(version) {
            Ok(0) if tables == 0 => Ok(Found::Fresh),
            Ok(v) if v >= 1 => Ok(Found::Ours(v)),
            _ => Ok(Found::Foreign),
        };
    }
    if app == 0 && version == 0 && tables == 0 {
        return Ok(Found::Fresh);
    }
    Ok(Found::Foreign)
}

/// Settings every read-write connection needs (STO-2). WAL comes first so
/// the schema is created in it.
fn apply_pragmas(conn: &Connection) -> Result<(), StoreError> {
    let mode: String = conn.pragma_update_and_check(None, "journal_mode", "WAL", |r| r.get(0))?;
    if !mode.eq_ignore_ascii_case("wal") {
        return Err(StoreError::Io(format!(
            "the database would not switch to WAL (journal mode {mode}); is it on a network filesystem?"
        )));
    }
    conn.execute_batch("PRAGMA synchronous = FULL; PRAGMA foreign_keys = ON;")?;
    Ok(())
}

/// Brings the database to `SCHEMA_VERSION` in one transaction. The state is
/// read again after the write lock is taken, because another process may
/// have migrated (or created the file) in between.
fn migrate(conn: &mut Connection) -> Result<(), StoreError> {
    if found(conn)? == Found::Ours(SCHEMA_VERSION) {
        return Ok(());
    }
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let from = match found(&tx)? {
        Found::Foreign => return Err(StoreError::NotTeamwire),
        Found::Fresh => 0,
        Found::Ours(v) => {
            check_version_not_newer(v)?;
            v
        }
    };
    for step in MIGRATIONS.iter().skip(from as usize) {
        tx.execute_batch(step)?;
    }
    tx.pragma_update(None, "application_id", APPLICATION_ID)?;
    tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    tx.commit()?;
    Ok(())
}

fn check_version_not_newer(db: u32) -> Result<(), StoreError> {
    if db > SCHEMA_VERSION {
        return Err(StoreError::SchemaNewer {
            db,
            binary: SCHEMA_VERSION,
        });
    }
    Ok(())
}

#[cfg(test)]
mod build_identity_tests {
    use super::WRITER_VERSION;

    #[test]
    fn the_writer_version_is_the_crate_version_plus_a_commit_or_unknown() {
        let (version, commit) = WRITER_VERSION.split_once('+').unwrap_or(("", ""));
        assert_eq!(version, env!("CARGO_PKG_VERSION"), "{WRITER_VERSION}");
        let is_commit = commit.len() == 12 && commit.bytes().all(|b| b.is_ascii_hexdigit());
        assert!(is_commit || commit == "unknown", "{WRITER_VERSION}");
    }
}
