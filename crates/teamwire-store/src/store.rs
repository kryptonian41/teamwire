//! Opening the database: pragmas, the forward migration, the refusal of a
//! newer or foreign file, and the two transaction entry points.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OpenFlags, TransactionBehavior};
use teamwire_core::{TeamName, Timestamp};

use crate::error::StoreError;
use crate::older::show_missing_columns;
use crate::tx::{ReadTx, Tx};
use crate::{
    SCHEMA_V1_SQL, SCHEMA_V2_SQL, SCHEMA_V3_SQL, SCHEMA_V4_SQL, SCHEMA_V5_SQL, SCHEMA_V6_SQL,
    SCHEMA_VERSION,
};

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
pub(crate) const MIGRATIONS: [&str; SCHEMA_VERSION as usize] = [
    SCHEMA_V1_SQL,
    SCHEMA_V2_SQL,
    SCHEMA_V3_SQL,
    SCHEMA_V4_SQL,
    SCHEMA_V5_SQL,
    SCHEMA_V6_SQL,
];

/// One team's database. One `rusqlite::Connection`, never shared across
/// processes.
#[derive(Debug)]
pub struct Store {
    conn: Connection,
    writer_version: String,
    read_only: bool,
    /// The first failed transaction that `record_failure` has not written
    /// down yet. In a `RefCell`, because a read fails through `&self`.
    failed: RefCell<Option<StoreError>>,
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
            failed: RefCell::new(None),
            cleanup: no_side_files.then(|| path.to_path_buf()),
        })
    }

    fn open_read_write(path: &Path, create: OpenFlags) -> Result<Self, StoreError> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX | create,
        )?;
        Self::ready(conn)
    }

    /// Settings and migration of a freshly opened read-write connection. The
    /// switch to WAL and the migration write to the file outside
    /// `Store::write`, so a full disk is given the file's path here too.
    fn ready(mut conn: Connection) -> Result<Self, StoreError> {
        let prepared = Self::prepare(&mut conn);
        prepared.map_err(|e| e.at(conn.path()))?;
        Ok(Self {
            conn,
            writer_version: WRITER_VERSION.to_owned(),
            read_only: false,
            failed: RefCell::new(None),
            cleanup: None,
        })
    }

    fn prepare(conn: &mut Connection) -> Result<(), StoreError> {
        conn.busy_timeout(Duration::from_millis(u64::from(BUSY_TIMEOUT_MS)))?;
        // Look before touching anything: switching a foreign or newer file to
        // WAL would already be a write.
        match found(conn)? {
            Found::Foreign => return Err(StoreError::NotTeamwire),
            Found::Ours(v) if v > SCHEMA_VERSION => {
                return Err(StoreError::SchemaNewer {
                    db: v,
                    binary: SCHEMA_VERSION,
                });
            }
            Found::Fresh | Found::Ours(_) => {}
        }
        apply_pragmas(conn)?;
        migrate(conn)
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
            .transaction_with_behavior(TransactionBehavior::Immediate);
        let out = tx.map_err(StoreError::from).and_then(|tx| {
            let out = f(&Tx::new(&tx, &self.writer_version))?;
            tx.commit()?;
            Ok(out)
        });
        out.map_err(|e| self.remember(e.at(self.conn.path())))
    }

    /// Keeps the first busy, full, corrupt or io failure of a store opened
    /// for writing, for `record_failure`. A read-only store never writes, so
    /// it keeps nothing.
    fn remember(&self, e: StoreError) -> StoreError {
        if !self.read_only && e.is_recorded() {
            self.failed.borrow_mut().get_or_insert_with(|| e.clone());
        }
        e
    }

    /// Writes the `store.error` row (`docs/metrics.md`) of the first failed
    /// transaction (a write, or a read of a store opened for writing) since
    /// the last call, in its own transaction now that the failed one rolled
    /// back. Each command, hook, tool call and watch poll calls it
    /// on its way out with its own name as `op`; `at` is the caller's, because
    /// the store never reads the clock. Best effort: when this write fails
    /// too, the failure is kept for the next call, so a full disk is recorded
    /// once it has room again. A file another binary migrated since is not
    /// written to (STO-6), and the failure is dropped.
    ///
    /// A failure while the store is being opened leaves no row, because
    /// there is no `Store` to call this on. The open failed with no
    /// connection, or with an unreadable header, or in the switch to WAL or
    /// the migration of a file that is not yet at this binary's schema, which
    /// this method refuses to write to. A busy header read means another
    /// process held the database past the busy timeout, so an immediate
    /// second write would wait again and fail the same way. The caller still
    /// reports the failure where it happens.
    pub fn record_failure(&mut self, at: Timestamp, op: &str) {
        let Some(failed) = self.failed.take() else {
            return;
        };
        let written = self.write(|tx| {
            // Read under the write lock, so no migration can come between
            // the check and the row.
            let db: i64 = tx
                .connection()
                .query_row("PRAGMA user_version", [], |r| r.get(0))?;
            if db != i64::from(SCHEMA_VERSION) {
                return Ok(());
            }
            tx.record_store_error(at, op, &failed)
        });
        if written.is_err() {
            *self.failed.get_mut() = Some(failed);
        }
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
        out.map_err(|e| self.remember(e.at(self.conn.path())))
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
mod full_path_tests {
    use rusqlite::Connection;
    use teamwire_fakes::TestHome;

    use super::Store;
    use crate::StoreError;

    #[test]
    fn a_full_disk_while_opening_names_the_file() {
        let home = TestHome::new().unwrap();
        let path = home.path().join("team.db");
        let conn = Connection::open(&path).unwrap();
        // One page holds the header and nothing else, so creating the schema
        // meets SQLITE_FULL the way a full volume does.
        let _max: i64 = conn
            .query_row("PRAGMA max_page_count = 1", [], |r| r.get(0))
            .unwrap();
        let named = std::fs::canonicalize(&path).unwrap();
        assert_eq!(
            Store::ready(conn).unwrap_err(),
            StoreError::Full { path: named }
        );
    }

    #[test]
    fn a_full_disk_inside_a_read_names_the_file() {
        let home = TestHome::new().unwrap();
        let team = teamwire_core::TeamName::parse("alpha").unwrap();
        let store = Store::open(home.path(), &team).unwrap();
        let full = store.read(|_| {
            Err::<(), _>(StoreError::Full {
                path: std::path::PathBuf::new(),
            })
        });
        let path = std::fs::canonicalize(crate::database_path(home.path(), &team)).unwrap();
        assert_eq!(full, Err(StoreError::Full { path }));
    }

    /// An open that fails has no `Store`, so nothing can write its
    /// `store.error` row (`record_failure` also refuses a file at another
    /// schema). Writing the row some other way is decided against: a busy
    /// header read means the database stayed locked past the busy timeout, so
    /// a second write would wait and fail the same way.
    #[test]
    fn an_open_that_fails_while_migrating_leaves_no_store_error_row() {
        let home = TestHome::new().unwrap();
        let team = teamwire_core::TeamName::parse("alpha").unwrap();
        let path = crate::database_path(home.path(), &team);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let conn = Connection::open(&path).unwrap();
        for step in [
            crate::SCHEMA_V1_SQL,
            crate::SCHEMA_V2_SQL,
            crate::SCHEMA_V3_SQL,
            crate::SCHEMA_V4_SQL,
            crate::SCHEMA_V5_SQL,
        ] {
            conn.execute_batch(step).unwrap();
        }
        conn.pragma_update(None, "application_id", crate::APPLICATION_ID)
            .unwrap();
        conn.pragma_update(None, "user_version", 5).unwrap();
        // Version 6 rebuilds `events`, so the file needs room for a second
        // copy of this row.
        conn.execute(
            "INSERT INTO events (ulid, time, type, data, writer_version)
             VALUES ('01J9ZQ000000000000000EVT01', '2026-10-01T00:00:01.000Z',
                     'team.created', ?1, '0.3.0')",
            [format!("\"{}\"", "x".repeat(256 * 1024))],
        )
        .unwrap();
        let pages: i64 = conn
            .query_row("PRAGMA page_count", [], |r| r.get(0))
            .unwrap();
        let _max: i64 = conn
            .query_row(&format!("PRAGMA max_page_count = {pages}"), [], |r| {
                r.get(0)
            })
            .unwrap();
        let named = std::fs::canonicalize(&path).unwrap();
        assert_eq!(
            Store::ready(conn).unwrap_err(),
            StoreError::Full { path: named }
        );

        // The failed migration rolled back, so a normal open migrates the
        // file, and no row of the failure is in it.
        let store = Store::open(home.path(), &team).unwrap();
        assert_eq!(store.schema_version().unwrap(), crate::SCHEMA_VERSION);
        let kinds: Vec<_> = store
            .read(|tx| tx.events_after(0, None, None)?.all_or_corrupt())
            .unwrap()
            .into_iter()
            .map(|e| e.kind)
            .collect();
        assert_eq!(kinds, [teamwire_core::EventType::TeamCreated]);
    }
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
