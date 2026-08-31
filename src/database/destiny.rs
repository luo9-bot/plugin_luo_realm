use rusqlite::{OptionalExtension, Transaction, params};

use super::{DatabaseError, DatabaseResult, player_id};

pub fn daily_event(
    transaction: &Transaction<'_>,
    user_id: u64,
    date: &str,
    definition_id: &str,
    seed: &str,
) -> DatabaseResult<String> {
    let player_id = player_id(user_id)?;
    if let Some(existing) = transaction
        .query_row(
            "SELECT definition_id FROM destiny_events
             WHERE player_id=?1 AND event_date=?2 LIMIT 1",
            params![player_id, date],
            |row| row.get(0),
        )
        .optional()
        .map_err(DatabaseError::from_sqlite)?
    {
        return Ok(existing);
    }
    transaction
        .execute(
            "INSERT INTO destiny_events(player_id, event_date, definition_id, seed)
             VALUES(?1, ?2, ?3, ?4)",
            params![player_id, date, definition_id, seed],
        )
        .map_err(DatabaseError::from_sqlite)?;
    Ok(definition_id.to_owned())
}
