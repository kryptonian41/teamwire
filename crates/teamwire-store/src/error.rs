//! The store's error type and the mapping from `SQLite` errors to it.

use rusqlite::ErrorCode;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum StoreError {
    #[error("the database stayed locked past the busy timeout")]
    Busy,
    #[error("the disk is full")]
    Full,
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
    #[error("{0}")]
    Io(String),
}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        let code = match &e {
            rusqlite::Error::SqliteFailure(err, _) => Some(err.code),
            _ => None,
        };
        match code {
            Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked) => Self::Busy,
            Some(ErrorCode::DiskFull) => Self::Full,
            Some(ErrorCode::ConstraintViolation) => Self::Constraint(e.to_string()),
            Some(ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase) => {
                Self::Corrupt(e.to_string())
            }
            _ => match e {
                rusqlite::Error::FromSqlConversionFailure(..)
                | rusqlite::Error::InvalidColumnType(..)
                | rusqlite::Error::IntegralValueOutOfRange(..) => Self::Corrupt(e.to_string()),
                _ => Self::Io(e.to_string()),
            },
        }
    }
}

impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

impl StoreError {
    /// A stored value that does not parse: the row is damaged, not the caller wrong.
    pub(crate) fn bad_value(table: &str, key: &str, what: impl std::fmt::Display) -> Self {
        Self::Corrupt(format!("{table} {key}: {what}"))
    }
}
