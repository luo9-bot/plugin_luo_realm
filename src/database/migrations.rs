use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};

use super::error::{DatabaseError, DatabaseResult};

const INITIAL_SCHEMA: &str = include_str!("../../migrations/0001_initial.sql");

pub fn apply(connection: &mut Connection) -> DatabaseResult<()> {
    let current_version = connection
        .query_row(
            "SELECT version FROM schema_migrations ORDER BY version DESC LIMIT 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .or_else(|error| {
            if error.to_string().contains("no such table") {
                Ok(None)
            } else {
                Err(error)
            }
        })
        .map_err(DatabaseError::from_sqlite)?
        .unwrap_or(0);

    if current_version >= 1 {
        return Ok(());
    }

    let transaction = connection
        .transaction()
        .map_err(DatabaseError::from_sqlite)?;
    transaction
        .execute_batch(INITIAL_SCHEMA)
        .map_err(|error| DatabaseError::Migration(error.to_string()))?;
    transaction
        .execute(
            "INSERT INTO schema_migrations(version, applied_at) VALUES(1, ?1)",
            params![unix_timestamp()],
        )
        .map_err(DatabaseError::from_sqlite)?;
    transaction.commit().map_err(DatabaseError::from_sqlite)
}

fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
