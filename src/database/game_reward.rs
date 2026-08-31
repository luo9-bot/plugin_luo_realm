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
