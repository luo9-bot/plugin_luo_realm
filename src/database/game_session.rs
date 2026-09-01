use rusqlite::{OptionalExtension, Transaction, params};

use super::{DatabaseError, DatabaseResult, player_id};

pub fn reserve_issued_at(
    transaction: &Transaction<'_>,
    player: u64,
    current_time: i64,
    minimum_interval_seconds: i64,
) -> DatabaseResult<Option<u32>> {
    let previous = transaction
        .query_row(
            "SELECT last_issued_at FROM game_session_issuance WHERE player_id=?1",
            [player_id(player)?],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(DatabaseError::from_sqlite)?;
    if previous.is_some_and(|value| current_time < value.saturating_add(minimum_interval_seconds)) {
        return Ok(None);
    }
    let issued_at = previous.map_or(current_time, |value| {
        current_time.max(value.saturating_add(1))
    });
    let encoded = u32::try_from(issued_at)
        .map_err(|_| DatabaseError::InvalidData("game session timestamp is out of range".into()))?;

    transaction
        .execute(
            "INSERT INTO game_session_issuance(player_id, last_issued_at)
             VALUES(?1, ?2)
             ON CONFLICT(player_id) DO UPDATE SET last_issued_at=excluded.last_issued_at",
            params![player_id(player)?, issued_at],
        )
        .map_err(DatabaseError::from_sqlite)?;
    Ok(Some(encoded))
}
