use rusqlite::Transaction;

use super::{DatabaseError, DatabaseResult, player_id};

#[derive(Debug)]
pub struct CultivationState {
    pub system_id: String,
    pub realm_index: u32,
    pub progress: u64,
}

pub fn get(transaction: &Transaction<'_>, user_id: u64) -> DatabaseResult<CultivationState> {
    transaction
        .query_row(
            "SELECT system_id, realm_index, progress FROM player_cultivation
             WHERE player_id=?1",
            [player_id(user_id)?],
            |row| {
                Ok(CultivationState {
                    system_id: row.get(0)?,
                    realm_index: row.get(1)?,
                    progress: row.get(2)?,
                })
            },
        )
        .map_err(DatabaseError::from_sqlite)
}
