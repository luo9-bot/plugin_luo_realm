use rusqlite::{OptionalExtension, Transaction, params};

use crate::game::VerifiedVoucher;

use super::{DatabaseError, DatabaseResult, player_id, unix_timestamp, wallet};

#[derive(Debug)]
pub enum RedemptionResult {
    Redeemed {
        reward: i64,
        balance_after: i64,
        remaining_today: u32,
    },
    AlreadyRedeemed,
    DailyLimitReached,
}

pub fn redeem(
    transaction: &Transaction<'_>,
    voucher: &VerifiedVoucher,
    redemption_date: &str,
    daily_limit: u32,
) -> DatabaseResult<RedemptionResult> {
    let existing = transaction
        .query_row(
            "SELECT 1 FROM game_voucher_redemptions WHERE voucher_nonce=?1",
            [&voucher.nonce],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(DatabaseError::from_sqlite)?
        .is_some();
    if existing {
        return Ok(RedemptionResult::AlreadyRedeemed);
    }

    let redeemed_today: u32 = transaction
        .query_row(
            "SELECT COUNT(*) FROM game_voucher_redemptions
             WHERE player_id=?1 AND game_id=?2 AND redemption_date=?3",
            params![
                player_id(voucher.player_id)?,
                voucher.game_id,
                redemption_date
            ],
            |row| row.get(0),
        )
        .map_err(DatabaseError::from_sqlite)?;
    if redeemed_today >= daily_limit {
        return Ok(RedemptionResult::DailyLimitReached);
    }

    let wallet_entry = wallet::credit(
        transaction,
        voucher.player_id,
        "coins",
        voucher.reward,
        "ascii_fpv_reward",
        &format!("ascii-fpv:{}", voucher.nonce),
    )?;
    transaction
        .execute(
            "INSERT INTO game_voucher_redemptions(
                 voucher_nonce, player_id, game_id, score, reward_amount,
                 redemption_date, reward_transaction_id, issued_at, redeemed_at
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                voucher.nonce,
                player_id(voucher.player_id)?,
                voucher.game_id,
                voucher.score,
                voucher.reward,
                redemption_date,
                wallet_entry.transaction_id,
                voucher.issued_at,
                unix_timestamp(),
            ],
        )
        .map_err(DatabaseError::from_sqlite)?;

    Ok(RedemptionResult::Redeemed {
        reward: voucher.reward,
        balance_after: wallet_entry.balance_after,
        remaining_today: daily_limit.saturating_sub(redeemed_today + 1),
    })
}

#[cfg(test)]
mod tests {
    use super::{RedemptionResult, redeem};
    use crate::database::migrations;
    use crate::game::VerifiedVoucher;
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

    fn voucher(nonce: &str, reward: i64) -> VerifiedVoucher {
        VerifiedVoucher {
            nonce: nonce.into(),
            player_id: 10001,
            game_id: "ascii-fpv".into(),
            score: 1_200,
            reward,
            issued_at: 0,
        }
    }

    #[test]
    fn redeem_is_idempotent_per_nonce_and_capped_per_day() {
        let mut connection = memory_database();
        let transaction = connection.transaction().expect("begin transaction");

        let first =
            redeem(&transaction, &voucher("n1", 50), "2026-09-03", 2).expect("first redeem");
        let replay =
            redeem(&transaction, &voucher("n1", 50), "2026-09-03", 2).expect("replayed redeem");
        let second =
            redeem(&transaction, &voucher("n2", 30), "2026-09-03", 2).expect("second redeem");
        let third =
            redeem(&transaction, &voucher("n3", 30), "2026-09-03", 2).expect("third redeem");
        transaction.commit().expect("commit");

        let RedemptionResult::Redeemed {
            balance_after,
            remaining_today,
            ..
        } = first
        else {
            panic!("first redeem should succeed");
        };
        assert_eq!((balance_after, remaining_today), (50, 1));
        assert!(matches!(replay, RedemptionResult::AlreadyRedeemed));
        let RedemptionResult::Redeemed {
            balance_after: after_second,
            remaining_today: zero,
            ..
        } = second
        else {
            panic!("second redeem should succeed");
        };
        assert_eq!((after_second, zero), (80, 0));
        assert!(matches!(third, RedemptionResult::DailyLimitReached));

        let balance: i64 = connection
            .query_row(
                "SELECT amount FROM player_balances WHERE player_id=10001 AND currency_code='coins'",
                [],
                |row| row.get(0),
            )
            .expect("balance");
        assert_eq!(balance, 80);
        let redemptions: i64 = connection
            .query_row("SELECT COUNT(*) FROM game_voucher_redemptions", [], |row| {
                row.get(0)
            })
            .expect("redemption count");
        assert_eq!(redemptions, 2);
    }
}
