use rusqlite::{Connection, Transaction, params};

use super::{DatabaseError, DatabaseResult, player_id, unix_timestamp};

pub fn is_enabled(connection: &Connection, group_id: u64) -> DatabaseResult<bool> {
    connection
        .query_row(
            "SELECT COALESCE((SELECT enabled FROM groups WHERE group_id=?1), 0)",
            [player_id(group_id)?],
            |row| row.get(0),
        )
        .map_err(DatabaseError::from_sqlite)
}

pub fn set_enabled(
    transaction: &Transaction<'_>,
    group_id: u64,
    enabled: bool,
) -> DatabaseResult<()> {
    transaction
        .execute(
            "INSERT INTO groups(group_id, enabled, created_at, updated_at)
             VALUES(?1, ?2, ?3, ?3)
             ON CONFLICT(group_id) DO UPDATE SET
                 enabled=excluded.enabled,
                 updated_at=excluded.updated_at",
            params![player_id(group_id)?, enabled, unix_timestamp()],
        )
        .map_err(DatabaseError::from_sqlite)?;
    Ok(())
}

pub fn feature_enabled(
    connection: &Connection,
    group_id: u64,
    feature_code: &str,
) -> DatabaseResult<bool> {
    connection
        .query_row(
            "SELECT COALESCE((
                 SELECT enabled FROM group_features
                 WHERE group_id=?1 AND feature_code=?2
             ), 1)",
            params![player_id(group_id)?, feature_code],
            |row| row.get(0),
        )
        .map_err(DatabaseError::from_sqlite)
}

pub fn set_feature(
    transaction: &Transaction<'_>,
    group_id: u64,
    feature_code: &str,
    enabled: bool,
) -> DatabaseResult<()> {
    transaction
        .execute(
            "INSERT INTO group_features(group_id, feature_code, enabled, updated_at)
             VALUES(?1, ?2, ?3, ?4)
             ON CONFLICT(group_id, feature_code) DO UPDATE SET
                 enabled=excluded.enabled,
                 updated_at=excluded.updated_at",
            params![
                player_id(group_id)?,
                feature_code,
                enabled,
                unix_timestamp()
            ],
        )
        .map_err(DatabaseError::from_sqlite)?;
    Ok(())
}

pub fn ensure(transaction: &Transaction<'_>, group_id: u64) -> DatabaseResult<()> {
    transaction
        .execute(
            "INSERT INTO groups(group_id, created_at, updated_at)
             VALUES(?1, ?2, ?2) ON CONFLICT(group_id) DO NOTHING",
            params![player_id(group_id)?, unix_timestamp()],
        )
        .map_err(DatabaseError::from_sqlite)?;
    Ok(())
}

pub fn ranking(connection: &Connection, limit: usize) -> DatabaseResult<Vec<String>> {
    let limit = i64::try_from(limit.clamp(1, 100)).unwrap_or(100);
    let mut statement = connection
        .prepare(
            "SELECT profile.display_name, COALESCE(wins.metric_value, 0), cultivation.realm_index
             FROM players player
             JOIN player_profiles profile USING(player_id)
             JOIN player_cultivation cultivation USING(player_id)
             LEFT JOIN player_statistics wins
                    ON wins.player_id=player.player_id AND wins.metric_code='wins'
             WHERE player.status='active' AND player.registration_state='active'
             ORDER BY COALESCE(wins.metric_value, 0) DESC, cultivation.realm_index DESC
             LIMIT ?1",
        )
        .map_err(DatabaseError::from_sqlite)?;
    let rows = statement
        .query_map([limit], |row| {
            Ok(format!(
                "{} 胜场 {} 境界 {}",
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)? + 1
            ))
        })
        .map_err(DatabaseError::from_sqlite)?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(DatabaseError::from_sqlite)
}
