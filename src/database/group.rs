use rusqlite::{Connection, Transaction, params};

use super::{DatabaseError, DatabaseResult, player_id, unix_timestamp};

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
