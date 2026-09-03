use rusqlite::{OptionalExtension, Transaction, params};

use crate::{combat::EquipmentSlot, equipment};

use super::{DatabaseError, DatabaseResult, player_id, unix_timestamp};

#[derive(Clone, Debug)]
pub struct InventoryItem {
    pub item_id: i64,
    pub slot_index: i64,
    pub definition_id: String,
    pub quantity: i64,
    pub quality: String,
    pub level: u32,
    pub equipped_slot: Option<String>,
}

pub fn add_item(
    transaction: &Transaction<'_>,
    user_id: u64,
    definition_id: &str,
    quantity: i64,
    slot_index: i64,
) -> DatabaseResult<i64> {
    if quantity <= 0 || definition_id.trim().is_empty() {
        return Err(DatabaseError::InvalidData("物品定义和数量不合法".into()));
    }
    transaction
        .execute(
            "INSERT INTO item_instances(
                 player_id, definition_id, quantity, quality, created_at
             ) VALUES(?1, ?2, ?3, 'legacy', ?4)",
            params![
                player_id(user_id)?,
                definition_id,
                quantity,
                unix_timestamp()
            ],
        )
        .map_err(DatabaseError::from_sqlite)?;
    let item_id = transaction.last_insert_rowid();
    transaction
        .execute(
            "INSERT INTO inventory_slots(player_id, slot_index, item_instance_id)
             VALUES(?1, ?2, ?3)",
            params![player_id(user_id)?, slot_index, item_id],
        )
        .map_err(DatabaseError::from_sqlite)?;
    Ok(item_id)
}

pub fn list(transaction: &Transaction<'_>, user_id: u64) -> DatabaseResult<Vec<InventoryItem>> {
    let mut statement = transaction
        .prepare(
            "SELECT item.item_instance_id, slot.slot_index, item.definition_id,
                    item.quantity, item.quality, item.level, equipped.slot_code
             FROM inventory_slots slot
             JOIN item_instances item USING(item_instance_id)
             LEFT JOIN equipment_loadouts equipped USING(item_instance_id)
             WHERE slot.player_id=?1 ORDER BY slot.slot_index",
        )
        .map_err(DatabaseError::from_sqlite)?;
    statement
        .query_map([player_id(user_id)?], |row| {
            Ok(InventoryItem {
                item_id: row.get(0)?,
                slot_index: row.get(1)?,
                definition_id: row.get(2)?,
                quantity: row.get(3)?,
                quality: row.get(4)?,
                level: row.get(5)?,
                equipped_slot: row.get(6)?,
            })
        })
        .map_err(DatabaseError::from_sqlite)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(DatabaseError::from_sqlite)
}

pub fn equipped(
    transaction: &Transaction<'_>,
    user_id: u64,
) -> DatabaseResult<Vec<equipment::EquipmentItem>> {
    let id = player_id(user_id)?;
    let mut statement = transaction
        .prepare(
            "SELECT item.item_instance_id, item.definition_id, item.quality,
                    item.level, equipped.slot_code
             FROM equipment_loadouts equipped
             JOIN item_instances item USING(item_instance_id)
             WHERE equipped.player_id=?1 ORDER BY equipped.slot_code",
        )
        .map_err(DatabaseError::from_sqlite)?;
    let rows = statement
        .query_map([id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, u32>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(DatabaseError::from_sqlite)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(DatabaseError::from_sqlite)?;
    rows.into_iter()
        .map(|(item_id, definition_id, quality, level, slot_code)| {
            let slot = EquipmentSlot::from_code(&slot_code)
                .ok_or_else(|| DatabaseError::InvalidData(format!("未知装备槽：{slot_code}")))?;
            Ok(equipment::EquipmentItem {
                item_id,
                definition_id,
                quality,
                level,
                slot,
                modifiers: modifiers(transaction, item_id)?,
            })
        })
        .collect()
}

pub fn equip(
    transaction: &Transaction<'_>,
    user_id: u64,
    item_id: i64,
    requested_slot: EquipmentSlot,
) -> DatabaseResult<()> {
    let id = player_id(user_id)?;
    let definition_id = transaction
        .query_row(
            "SELECT definition_id FROM item_instances
             WHERE item_instance_id=?1 AND player_id=?2 AND quantity=1",
            params![item_id, id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(DatabaseError::from_sqlite)?
        .ok_or(DatabaseError::NotFound)?;
    let definition_slot = equipment::slot_for_definition(&definition_id)
        .ok_or_else(|| DatabaseError::InvalidData("该物品不是可穿戴装备".into()))?;
    let slot_allowed = definition_slot == requested_slot
        || (definition_slot == EquipmentSlot::AccessoryOne
            && requested_slot == EquipmentSlot::AccessoryTwo);
    if !slot_allowed {
        return Err(DatabaseError::InvalidData(format!(
            "装备只能放入 {}",
            definition_slot.code()
        )));
    }
    transaction
        .execute(
            "INSERT INTO equipment_loadouts(player_id, slot_code, item_instance_id)
             VALUES(?1, ?2, ?3)
             ON CONFLICT(player_id, slot_code) DO UPDATE SET
                 item_instance_id=excluded.item_instance_id",
            params![id, requested_slot.code(), item_id],
        )
        .map_err(DatabaseError::from_sqlite)?;
    Ok(())
}

pub fn unequip(
    transaction: &Transaction<'_>,
    user_id: u64,
    slot: EquipmentSlot,
) -> DatabaseResult<bool> {
    transaction
        .execute(
            "DELETE FROM equipment_loadouts WHERE player_id=?1 AND slot_code=?2",
            params![player_id(user_id)?, slot.code()],
        )
        .map(|changed| changed == 1)
        .map_err(DatabaseError::from_sqlite)
}

pub fn refine(transaction: &Transaction<'_>, user_id: u64, item_id: i64) -> DatabaseResult<u32> {
    let changed = transaction
        .execute(
            "UPDATE item_instances SET level=level+1
             WHERE item_instance_id=?1 AND player_id=?2 AND quantity=1 AND level<100",
            params![item_id, player_id(user_id)?],
        )
        .map_err(DatabaseError::from_sqlite)?;
    if changed != 1 {
        return Err(DatabaseError::NotFound);
    }
    transaction
        .query_row(
            "SELECT level FROM item_instances WHERE item_instance_id=?1",
            [item_id],
            |row| row.get(0),
        )
        .map_err(DatabaseError::from_sqlite)
}

fn modifiers(transaction: &Transaction<'_>, item_id: i64) -> DatabaseResult<Vec<(String, i64)>> {
    let mut statement = transaction
        .prepare(
            "SELECT modifier_code, modifier_value FROM item_modifiers
             WHERE item_instance_id=?1 ORDER BY modifier_code",
        )
        .map_err(DatabaseError::from_sqlite)?;
    statement
        .query_map([item_id], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(DatabaseError::from_sqlite)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(DatabaseError::from_sqlite)
}

#[cfg(test)]
mod tests {
    use super::{add_item, equip, equipped, list, unequip};
    use crate::combat::EquipmentSlot;
    use crate::database::DatabaseError;
    use crate::database::migrations;
    use rusqlite::Connection;

    fn memory_database() -> Connection {
        let mut connection = Connection::open_in_memory().expect("open in-memory database");
        migrations::apply(&mut connection).expect("apply migrations");
        for player in [10001, 10002] {
            connection
                .execute(
                    "INSERT INTO players(player_id, created_at, updated_at) VALUES(?1, 0, 0)",
                    [player],
                )
                .expect("insert player");
        }
        connection
    }

    fn transaction_of(connection: &mut Connection) -> rusqlite::Transaction<'_> {
        connection.transaction().expect("begin transaction")
    }

    #[test]
    fn equip_and_unequip_round_trip_within_one_transaction() {
        let mut connection = memory_database();
        let transaction = transaction_of(&mut connection);
        let item_id = add_item(&transaction, 10001, "iron_sword", 1, 0).expect("add item");
        equip(&transaction, 10001, item_id, EquipmentSlot::MainHand).expect("equip");
        let equipped_items = equipped(&transaction, 10001).expect("equipped");
        let removed = unequip(&transaction, 10001, EquipmentSlot::MainHand).expect("unequip");
        transaction.commit().expect("commit");

        assert_eq!(equipped_items.len(), 1);
        assert_eq!(equipped_items[0].item_id, item_id);
        assert_eq!(equipped_items[0].slot, EquipmentSlot::MainHand);
        assert!(removed);

        let verify = transaction_of(&mut connection);
        let items = list(&verify, 10001).expect("list");
        verify.commit().expect("commit");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].equipped_slot, None);
    }

    #[test]
    fn equip_rejects_foreign_items_and_mismatched_slots() {
        let mut connection = memory_database();
        let transaction = transaction_of(&mut connection);
        let own = add_item(&transaction, 10001, "iron_sword", 1, 0).expect("own item");
        let foreign = add_item(&transaction, 10002, "iron_sword", 1, 0).expect("foreign item");
        let herb = add_item(&transaction, 10001, "herb_bundle", 1, 1).expect("non-equipment");

        let foreign_error =
            equip(&transaction, 10001, foreign, EquipmentSlot::MainHand).expect_err("foreign");
        assert!(matches!(foreign_error, DatabaseError::NotFound));
        let herb_error =
            equip(&transaction, 10001, herb, EquipmentSlot::MainHand).expect_err("non-equipment");
        assert!(matches!(herb_error, DatabaseError::InvalidData(_)));
        let slot_error = equip(&transaction, 10001, own, EquipmentSlot::Body).expect_err("slot");
        assert!(matches!(slot_error, DatabaseError::InvalidData(_)));
        equip(&transaction, 10001, own, EquipmentSlot::MainHand).expect("valid equip");
        transaction.commit().expect("commit");

        let verify = transaction_of(&mut connection);
        let items = list(&verify, 10001).expect("list");
        verify.commit().expect("commit");
        assert_eq!(items[0].equipped_slot.as_deref(), Some("main_hand"));
    }
}
