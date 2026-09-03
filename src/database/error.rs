use rusqlite::{Error, ErrorCode};

use crate::domain::error_code::StableErrorCode;

#[derive(Debug, thiserror::Error)]
pub enum DatabaseError {
    #[error("database unavailable: {0}")]
    Unavailable(#[source] Error),
    #[error("database busy")]
    Busy,
    #[error("database constraint violation: {0}")]
    Constraint(#[source] Error),
    #[error("database object not found")]
    NotFound,
    #[error("insufficient balance")]
    InsufficientBalance,
    #[error("database integrity check failed: {0}")]
    Corrupt(String),
    #[error("database migration failed: {0}")]
    Migration(String),
    #[error("numeric identifier is outside SQLite INTEGER range")]
    InvalidIdentifier,
    #[error("invalid data: {0}")]
    InvalidData(String),
}

impl DatabaseError {
    pub fn from_sqlite(error: Error) -> Self {
        match &error {
            Error::SqliteFailure(details, _)
                if matches!(
                    details.code,
                    ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked
                ) =>
            {
                Self::Busy
            }
            Error::SqliteFailure(details, _) if details.code == ErrorCode::ConstraintViolation => {
                Self::Constraint(error)
            }
            _ => Self::Unavailable(error),
        }
    }
}

impl StableErrorCode for DatabaseError {
    fn error_code(&self) -> &'static str {
        match self {
            Self::Unavailable(_) => "database.unavailable",
            Self::Busy => "database.busy",
            Self::Constraint(_) => "database.constraint_violation",
            Self::NotFound => "database.not_found",
            Self::InsufficientBalance => "database.insufficient_balance",
            Self::Corrupt(_) => "database.corrupt",
            Self::Migration(_) => "database.migration_failed",
            Self::InvalidIdentifier => "database.invalid_identifier",
            Self::InvalidData(_) => "database.invalid_data",
        }
    }
}

pub type DatabaseResult<T> = Result<T, DatabaseError>;
