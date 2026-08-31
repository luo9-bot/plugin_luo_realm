use rusqlite::{OptionalExtension, Transaction, params};

use super::{
    DatabaseError, DatabaseResult, player_id, unix_timestamp,
    wallet::{self, WalletEntry},
};

pub enum CheckInResult {
    Completed { streak: i64, reward: WalletEntry },
    AlreadyCompleted,
}

pub fn check_in(
    transaction: &Transaction<'_>,
    user_id: u64,
    date: &str,
) -> DatabaseResult<CheckInResult> {
    let player_id = player_id(user_id)?;
    let existing: Option<i64> = transaction
        .query_row(
            "SELECT streak FROM daily_checkins WHERE player_id=?1 AND checkin_date=?2",
            params![player_id, date],
            |row| row.get(0),
        )
        .optional()
        .map_err(DatabaseError::from_sqlite)?;
    if existing.is_some() {
        return Ok(CheckInResult::AlreadyCompleted);
    }

    let previous_streak: i64 = transaction
        .query_row(
            "SELECT streak FROM daily_checkins WHERE player_id=?1
             ORDER BY checkin_date DESC LIMIT 1",
            [player_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(DatabaseError::from_sqlite)?
        .unwrap_or(0);
    let streak = previous_streak + 1;
    let reward = wallet::credit(
        transaction,
        user_id,
        "coins",
        100,
        "daily_checkin",
        &format!("checkin:{user_id}:{date}:coins"),
    )?;
    transaction
        .execute(
            "INSERT INTO daily_checkins(player_id, checkin_date, streak, reward_transaction_id)
             VALUES(?1, ?2, ?3, ?4)",
            params![player_id, date, streak, reward.transaction_id],
        )
        .map_err(DatabaseError::from_sqlite)?;
    increment_statistic(transaction, user_id, "checkins", 1)?;

    Ok(CheckInResult::Completed { streak, reward })
}

pub fn increment_statistic(
    transaction: &Transaction<'_>,
    user_id: u64,
    metric: &str,
    delta: i64,
) -> DatabaseResult<()> {
    transaction
        .execute(
            "INSERT INTO player_statistics(player_id, metric_code, metric_value, updated_at)
             VALUES(?1, ?2, ?3, ?4)
             ON CONFLICT(player_id, metric_code) DO UPDATE SET
                 metric_value=metric_value+excluded.metric_value,
                 updated_at=excluded.updated_at",
            params![player_id(user_id)?, metric, delta, unix_timestamp()],
        )
        .map_err(DatabaseError::from_sqlite)?;
    Ok(())
}
