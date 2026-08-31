use rusqlite::{Error, ErrorCode};

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

pub type DatabaseResult<T> = Result<T, DatabaseError>;
