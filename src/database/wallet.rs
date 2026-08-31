use rusqlite::{OptionalExtension, Transaction, params};

use super::{DatabaseError, DatabaseResult, player_id, unix_timestamp};

#[derive(Clone, Debug)]
pub struct WalletEntry {
    pub transaction_id: i64,
    pub balance_after: i64,
}

pub fn balance(transaction: &Transaction<'_>, user_id: u64, currency: &str) -> DatabaseResult<i64> {
    transaction
        .query_row(
            "SELECT amount FROM player_balances
             WHERE player_id=?1 AND currency_code=?2",
            params![player_id(user_id)?, currency],
            |row| row.get(0),
        )
        .optional()
        .map(|value| value.unwrap_or(0))
        .map_err(DatabaseError::from_sqlite)
}

pub fn credit(
    transaction: &Transaction<'_>,
    user_id: u64,
    currency: &str,
    amount: i64,
    reason: &str,
    idempotency_key: &str,
) -> DatabaseResult<WalletEntry> {
    change(
        transaction,
        user_id,
        currency,
        amount.max(1),
        reason,
        idempotency_key,
    )
}

pub fn debit(
    transaction: &Transaction<'_>,
    user_id: u64,
    currency: &str,
    amount: i64,
    reason: &str,
    idempotency_key: &str,
) -> DatabaseResult<WalletEntry> {
    change(
        transaction,
        user_id,
        currency,
        -amount.max(1),
        reason,
        idempotency_key,
    )
}

fn change(
    transaction: &Transaction<'_>,
    user_id: u64,
    currency: &str,
    delta: i64,
    reason: &str,
    idempotency_key: &str,
) -> DatabaseResult<WalletEntry> {
    if let Some(existing) = transaction
        .query_row(
            "SELECT transaction_id, balance_after FROM wallet_transactions
             WHERE idempotency_key=?1",
            [idempotency_key],
            |row| {
                Ok(WalletEntry {
                    transaction_id: row.get(0)?,
                    balance_after: row.get(1)?,
                })
            },
        )
        .optional()
        .map_err(DatabaseError::from_sqlite)?
    {
        return Ok(existing);
    }

    let player_id = player_id(user_id)?;
    transaction
        .execute(
            "INSERT INTO player_balances(player_id, currency_code, amount)
             VALUES(?1, ?2, 0) ON CONFLICT(player_id, currency_code) DO NOTHING",
            params![player_id, currency],
        )
        .map_err(DatabaseError::from_sqlite)?;
    let changed = transaction
        .execute(
            "UPDATE player_balances SET amount=amount+?3
             WHERE player_id=?1 AND currency_code=?2 AND amount+?3>=0",
            params![player_id, currency, delta],
        )
        .map_err(DatabaseError::from_sqlite)?;
    if changed == 0 {
        return Err(DatabaseError::InsufficientBalance);
    }
    let balance_after = balance(transaction, user_id, currency)?;
    transaction
        .execute(
            "INSERT INTO wallet_transactions(
                 player_id, currency_code, delta, balance_after, reason_code,
                 idempotency_key, created_at
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                player_id,
                currency,
                delta,
                balance_after,
                reason,
                idempotency_key,
                unix_timestamp()
            ],
        )
        .map_err(DatabaseError::from_sqlite)?;

    Ok(WalletEntry {
        transaction_id: transaction.last_insert_rowid(),
        balance_after,
    })
}
