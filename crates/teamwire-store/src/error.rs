//! The store's error type and the mapping from `SQLite` errors to it.

use rusqlite::ErrorCode;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum StoreError {
    #[error("the database stayed locked past the busy timeout")]
    Busy,
    /// `path` is the database file, so the message names the volume that
    /// filled. It is empty only until `Store` fills it in.
    #[error("the disk is full: cannot write {}", path.display())]
    Full { path: std::path::PathBuf },
    #[error("the database has schema {db}; this binary knows {binary}")]
    SchemaNewer { db: u32, binary: u32 },
    #[error("the database has schema {db}; this binary writes {binary}")]
    SchemaOlder { db: u32, binary: u32 },
    #[error("not a teamwire database")]
    NotTeamwire,
    #[error("the database is corrupt: {0}")]
    Corrupt(String),
    #[error("a constraint failed: {0}")]
    Constraint(String),
    /// The team has no database file. The one failure a command treats as
    /// "no such team" rather than as a broken store.
    #[error("no database at {}", path.display())]
    NoDatabase { path: std::path::PathBuf },
    #[error("{0}")]
    Io(String),
}

/// What every "no database" refusal adds, in the CLI and the MCP server
/// alike. The usual way to get there is a normal close, which deletes the
/// team's state after archiving it. The text is the same for every team and
/// caller, so it says nothing about whether this team ever existed.
pub const NO_DATABASE_HINT: &str =
    "A closed team's state is deleted; its archive is under $TEAMWIRE_HOME/archive/.";

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        if let rusqlite::Error::SqliteFailure(err, _) = &e {
            let code = err.code;
            if code == ErrorCode::DatabaseBusy || code == ErrorCode::DatabaseLocked {
                return Self::Busy;
            }
            if code == ErrorCode::DiskFull {
                return Self::Full {
                    path: std::path::PathBuf::new(),
                };
            }
            if code == ErrorCode::ConstraintViolation {
                return Self::Constraint(e.to_string());
            }
            if code == ErrorCode::DatabaseCorrupt || code == ErrorCode::NotADatabase {
                return Self::Corrupt(e.to_string());
            }
        }
        // A value that does not convert to the type the column holds is a
        // damaged row, not a caller mistake.
        if matches!(
            e,
            rusqlite::Error::FromSqlConversionFailure(..)
                | rusqlite::Error::InvalidColumnType(..)
                | rusqlite::Error::IntegralValueOutOfRange(..)
        ) {
            return Self::Corrupt(e.to_string());
        }
        Self::Io(e.to_string())
    }
}

impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

impl StoreError {
    /// The short name of the failure: the `code` of a `store.error` event and
    /// the word after `STORE-ERROR` in a watch line.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Busy => "busy",
            Self::Full { .. } => "full",
            Self::SchemaNewer { .. } => "schema_newer",
            Self::SchemaOlder { .. } => "schema_older",
            Self::NotTeamwire => "not_teamwire",
            Self::Corrupt(_) => "corrupt",
            Self::Constraint(_) => "constraint",
            Self::NoDatabase { .. } => "no_database",
            Self::Io(_) => "io",
        }
    }

    /// Whether a failed write leaves a `store.error` row. A refused schema
    /// must not be written to at all (STO-6), and a constraint or a missing
    /// file says nothing about the store's health.
    pub(crate) fn is_recorded(&self) -> bool {
        matches!(
            self,
            Self::Busy | Self::Full { .. } | Self::Corrupt(_) | Self::Io(_)
        )
    }

    /// Names the database file in a full-disk error that does not name one
    /// yet: `SQLite`'s own error does not say which file it was writing.
    pub(crate) fn at(self, db: Option<&str>) -> Self {
        match (self, db) {
            (Self::Full { path }, Some(db)) if path.as_os_str().is_empty() => Self::Full {
                path: std::path::PathBuf::from(db),
            },
            (e, _) => e,
        }
    }

    /// A stored value that does not parse: the row is damaged, not the caller wrong.
    pub(crate) fn bad_value(table: &str, key: &str, what: impl std::fmt::Display) -> Self {
        Self::Corrupt(format!("{table} {key}: {what}"))
    }
}
