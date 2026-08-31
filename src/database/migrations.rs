use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, params};

use super::error::{DatabaseError, DatabaseResult};

const MIGRATIONS: &[(i64, &str)] = &[
    (1, include_str!("../../migrations/0001_initial.sql")),
    (2, include_str!("../../migrations/0002_admin.sql")),
];

pub fn apply(connection: &mut Connection) -> DatabaseResult<()> {
    let migration_table_exists: bool = connection
        .query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM sqlite_master
                 WHERE type='table' AND name='schema_migrations'
             )",
            [],
            |row| row.get(0),
        )
        .map_err(DatabaseError::from_sqlite)?;
    let current_version = if migration_table_exists {
        connection
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                [],
                |row| row.get(0),
            )
            .map_err(DatabaseError::from_sqlite)?
    } else {
        0
    };

    for &(version, sql) in MIGRATIONS {
        if version <= current_version {
            continue;
        }
        let transaction = connection
            .transaction()
            .map_err(DatabaseError::from_sqlite)?;
        transaction
            .execute_batch(sql)
            .map_err(|error| DatabaseError::Migration(error.to_string()))?;
        transaction
            .execute(
                "INSERT INTO schema_migrations(version, applied_at) VALUES(?1, ?2)",
                params![version, unix_timestamp()],
            )
            .map_err(DatabaseError::from_sqlite)?;
        transaction.commit().map_err(DatabaseError::from_sqlite)?;
    }

    Ok(())
}

fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
