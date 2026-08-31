use rusqlite::{Transaction, params};

use super::{DatabaseError, DatabaseResult, player_id, unix_timestamp};

pub fn add_item(
    transaction: &Transaction<'_>,
    user_id: u64,
    definition_id: &str,
    quantity: i64,
    slot_index: i64,
) -> DatabaseResult<i64> {
    let player_id = player_id(user_id)?;
    transaction
        .execute(
            "INSERT INTO item_instances(
                 player_id, definition_id, quantity, quality, created_at
             ) VALUES(?1, ?2, ?3, 'legacy', ?4)",
            params![player_id, definition_id, quantity, unix_timestamp()],
        )
        .map_err(DatabaseError::from_sqlite)?;
    let item_id = transaction.last_insert_rowid();
    transaction
        .execute(
            "INSERT INTO inventory_slots(player_id, slot_index, item_instance_id)
             VALUES(?1, ?2, ?3)",
            params![player_id, slot_index, item_id],
        )
        .map_err(DatabaseError::from_sqlite)?;
    Ok(item_id)
}

pub fn list(
    transaction: &Transaction<'_>,
    user_id: u64,
) -> DatabaseResult<Vec<(i64, String, i64)>> {
    let mut statement = transaction
        .prepare(
            "SELECT slot.slot_index, item.definition_id, item.quantity
             FROM inventory_slots slot
             JOIN item_instances item USING(item_instance_id)
             WHERE slot.player_id=?1 ORDER BY slot.slot_index",
        )
        .map_err(DatabaseError::from_sqlite)?;
    let rows = statement
        .query_map([player_id(user_id)?], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .map_err(DatabaseError::from_sqlite)?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(DatabaseError::from_sqlite)
}
