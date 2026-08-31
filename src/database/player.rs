use rusqlite::{OptionalExtension, Transaction, params};

use crate::core::Player;

use super::{DatabaseError, DatabaseResult, player_id, unix_timestamp};

pub fn find_or_create(transaction: &Transaction<'_>, user_id: u64) -> DatabaseResult<Player> {
    let player_id = player_id(user_id)?;
    let now = unix_timestamp();
    transaction
        .execute(
            "INSERT INTO players(player_id, created_at, updated_at)
             VALUES(?1, ?2, ?2) ON CONFLICT(player_id) DO NOTHING",
            params![player_id, now],
        )
        .map_err(DatabaseError::from_sqlite)?;
    transaction
        .execute(
            "INSERT INTO player_profiles(player_id, display_name)
             VALUES(?1, 'LR·旅者') ON CONFLICT(player_id) DO NOTHING",
            [player_id],
        )
        .map_err(DatabaseError::from_sqlite)?;
    transaction
        .execute(
            "INSERT INTO player_cultivation(player_id, system_id, updated_at)
             VALUES(?1, 'orthodox', ?2) ON CONFLICT(player_id) DO NOTHING",
            params![player_id, now],
        )
        .map_err(DatabaseError::from_sqlite)?;

    get(transaction, user_id)?.ok_or(DatabaseError::NotFound)
}

pub fn get(transaction: &Transaction<'_>, user_id: u64) -> DatabaseResult<Option<Player>> {
    let player_id = player_id(user_id)?;
    transaction
        .query_row(
            "SELECT p.player_id, profile.display_name, cultivation.realm_index,
                    COALESCE(coins.amount, 0), COALESCE(marks.amount, 0),
                    COALESCE(wins.metric_value, 0), COALESCE(losses.metric_value, 0)
             FROM players p
             JOIN player_profiles profile USING(player_id)
             JOIN player_cultivation cultivation USING(player_id)
             LEFT JOIN player_balances coins
                    ON coins.player_id=p.player_id AND coins.currency_code='coins'
             LEFT JOIN player_balances marks
                    ON marks.player_id=p.player_id AND marks.currency_code='marks'
             LEFT JOIN player_statistics wins
                    ON wins.player_id=p.player_id AND wins.metric_code='wins'
             LEFT JOIN player_statistics losses
                    ON losses.player_id=p.player_id AND losses.metric_code='losses'
             WHERE p.player_id=?1",
            [player_id],
            |row| {
                let id: i64 = row.get(0)?;
                let realm_index: u32 = row.get(2)?;
                Ok(Player {
                    user_id: id.to_string(),
                    display_name: row.get(1)?,
                    level: realm_index + 1,
                    experience: 0,
                    coins: row.get::<_, i64>(3)?.max(0) as u64,
                    marks: row.get::<_, i64>(4)?.max(0) as u64,
                    base_hp: 1000 + i64::from(realm_index) * 200,
                    base_attack: 100 + i64::from(realm_index) * 30,
                    base_defense: 50 + i64::from(realm_index) * 15,
                    critical_rate: 5.0,
                    critical_multiplier: 1.5,
                    speed: 10 + i64::from(realm_index),
                    wins: row.get::<_, i64>(5)?.max(0) as u64,
                    losses: row.get::<_, i64>(6)?.max(0) as u64,
                })
            },
        )
        .optional()
        .map_err(DatabaseError::from_sqlite)
}

pub fn rename(
    transaction: &Transaction<'_>,
    user_id: u64,
    display_name: &str,
) -> DatabaseResult<()> {
    let changed = transaction
        .execute(
            "UPDATE player_profiles SET display_name=?2 WHERE player_id=?1",
            params![player_id(user_id)?, display_name],
        )
        .map_err(DatabaseError::from_sqlite)?;
    if changed == 0 {
        return Err(DatabaseError::NotFound);
    }
    Ok(())
}
