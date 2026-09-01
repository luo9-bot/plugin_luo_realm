use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, params};

use super::error::{DatabaseError, DatabaseResult};

const MIGRATIONS: &[(i64, &str)] = &[
    (1, include_str!("../../migrations/0001_initial.sql")),
    (2, include_str!("../../migrations/0002_admin.sql")),
    (3, include_str!("../../migrations/0003_registration.sql")),
    (4, include_str!("../../migrations/0004_daily_world.sql")),
    (5, include_str!("../../migrations/0005_ascii_fpv.sql")),
    (6, include_str!("../../migrations/0006_game_sessions.sql")),
];

pub(crate) const CURRENT_SCHEMA_VERSION: i64 = MIGRATIONS[MIGRATIONS.len() - 1].0;

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

    MIGRATIONS
        .iter()
        .copied()
        .filter(|(version, _)| *version > current_version)
        .try_for_each(|(version, sql)| {
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
            transaction.commit().map_err(DatabaseError::from_sqlite)
        })
}

fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
