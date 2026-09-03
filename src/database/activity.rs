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

    let previous: Option<(String, i64)> = transaction
        .query_row(
            "SELECT checkin_date, streak FROM daily_checkins WHERE player_id=?1
             ORDER BY checkin_date DESC LIMIT 1",
            [player_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(DatabaseError::from_sqlite)?;
    let streak = match previous {
        Some((previous_date, previous_streak)) if is_consecutive_day(&previous_date, date) => {
            previous_streak + 1
        }
        _ => 1,
    };
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

/// 判断 `current` 是否恰好是 `previous` 的次日（两个 `YYYY-MM-DD` 日期）。
fn is_consecutive_day(previous: &str, current: &str) -> bool {
    match (serial_day(previous), serial_day(current)) {
        (Some(previous_day), Some(current_day)) => current_day == previous_day + 1,
        _ => false,
    }
}

/// 把 `YYYY-MM-DD` 转换为自 1970-01-01 起的天数（Howard Hinnant 民历算法）。
///
/// 日期由 `Database::local_date` 以本地时区生成，格式固定；解析失败返回
/// `None`，调用方按“不连续”处理，保持连击从 1 重新开始。
fn serial_day(date: &str) -> Option<i64> {
    let mut parts = date.split('-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: i64 = parts.next()?.parse().ok()?;
    let day: i64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    let shifted_year = if month <= 2 { year - 1 } else { year };
    let era = shifted_year.div_euclid(400);
    let year_of_era = shifted_year - era * 400;
    let shifted_month = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Some(era * 146_097 + day_of_era - 719_468)
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

#[cfg(test)]
mod tests {
    use super::{CheckInResult, check_in};
    use crate::database::migrations;
    use rusqlite::Connection;

    fn memory_database() -> Connection {
        let mut connection = Connection::open_in_memory().expect("open in-memory database");
        migrations::apply(&mut connection).expect("apply migrations");
        connection
            .execute(
                "INSERT INTO players(player_id, created_at, updated_at) VALUES(10001, 0, 0)",
                [],
            )
            .expect("insert player");
        connection
    }

    #[test]
    fn check_in_is_idempotent_per_day_and_tracks_streak() {
        let mut connection = memory_database();
        let transaction = connection.transaction().expect("begin transaction");

        let first = check_in(&transaction, 10001, "2026-09-01").expect("first check-in");
        let replay = check_in(&transaction, 10001, "2026-09-01").expect("replayed check-in");
        let consecutive = check_in(&transaction, 10001, "2026-09-02").expect("next day");
        let after_gap = check_in(&transaction, 10001, "2026-09-05").expect("after gap");
        transaction.commit().expect("commit");

        let CheckInResult::Completed { streak, reward } = first else {
            panic!("first check-in should complete");
        };
        assert_eq!(streak, 1);
        assert_eq!(reward.balance_after, 100);
        assert!(matches!(replay, CheckInResult::AlreadyCompleted));
        let CheckInResult::Completed { streak: second, .. } = consecutive else {
            panic!("second day should complete");
        };
        assert_eq!(second, 2);
        let CheckInResult::Completed { streak: reset, .. } = after_gap else {
            panic!("check-in after gap should complete");
        };
        assert_eq!(reset, 1);

        let checkins: i64 = connection
            .query_row(
                "SELECT metric_value FROM player_statistics
                 WHERE player_id=10001 AND metric_code='checkins'",
                [],
                |row| row.get(0),
            )
            .expect("checkin statistic");
        assert_eq!(checkins, 3);
    }

    #[test]
    fn consecutive_day_detection_handles_boundaries() {
        use super::{is_consecutive_day, serial_day};

        assert!(is_consecutive_day("2026-09-02", "2026-09-03"));
        assert!(is_consecutive_day("2026-08-31", "2026-09-01"));
        assert!(is_consecutive_day("2026-12-31", "2027-01-01"));
        assert!(is_consecutive_day("2028-02-28", "2028-02-29"));
        assert!(is_consecutive_day("2027-02-28", "2027-03-01"));
        assert!(!is_consecutive_day("2028-02-28", "2028-03-01"));
        assert!(!is_consecutive_day("2026-09-01", "2026-09-03"));
        assert!(!is_consecutive_day("2026-09-03", "2026-09-02"));
        assert_eq!(serial_day("1970-01-01"), Some(0));
        assert_eq!(serial_day("not-a-date"), None);
    }
}
