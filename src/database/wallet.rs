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

#[cfg(test)]
mod tests {
    use super::{balance, credit, debit};
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

    fn transaction_of(connection: &mut Connection) -> rusqlite::Transaction<'_> {
        connection.transaction().expect("begin transaction")
    }

    fn count_transactions(connection: &Connection, user_id: i64) -> i64 {
        connection
            .query_row(
                "SELECT COUNT(*) FROM wallet_transactions WHERE player_id=?1",
                [user_id],
                |row| row.get(0),
            )
            .expect("transaction count")
    }

    #[test]
    fn credit_and_debit_conserve_balance() {
        let mut connection = memory_database();
        let final_balance = {
            let transaction = transaction_of(&mut connection);
            assert_eq!(balance(&transaction, 10001, "coins").expect("balance"), 0);
            credit(&transaction, 10001, "coins", 100, "test", "k1").expect("credit");
            credit(&transaction, 10001, "coins", 50, "test", "k2").expect("credit");
            debit(&transaction, 10001, "coins", 30, "test", "k3").expect("debit");
            let result = balance(&transaction, 10001, "coins").expect("balance");
            transaction.commit().expect("commit");
            result
        };

        assert_eq!(final_balance, 120);
        let verify = transaction_of(&mut connection);
        assert_eq!(count_transactions(&verify, 10001), 3);
        verify.commit().expect("commit");
    }

    #[test]
    fn idempotency_key_never_pays_twice() {
        let mut connection = memory_database();
        let first = {
            let transaction = transaction_of(&mut connection);
            let first = credit(&transaction, 10001, "coins", 100, "test", "same-key")
                .expect("first credit");
            let replay = credit(&transaction, 10001, "coins", 100, "test", "same-key")
                .expect("replayed credit");
            assert_eq!(first.transaction_id, replay.transaction_id);
            assert_eq!(first.balance_after, replay.balance_after);
            transaction.commit().expect("commit");
            first
        };

        assert_eq!(first.balance_after, 100);
        let verify = transaction_of(&mut connection);
        assert_eq!(balance(&verify, 10001, "coins").expect("balance"), 100);
        assert_eq!(count_transactions(&verify, 10001), 1);
        verify.commit().expect("commit");
    }

    #[test]
    fn overdraw_is_rejected_without_partial_write() {
        let mut connection = memory_database();
        let error = {
            let transaction = transaction_of(&mut connection);
            credit(&transaction, 10001, "coins", 50, "test", "seed").expect("credit");
            let error =
                debit(&transaction, 10001, "coins", 100, "test", "over").expect_err("overdraw");
            let balance_after_failure = balance(&transaction, 10001, "coins").expect("balance");
            transaction.commit().expect("commit");
            assert_eq!(balance_after_failure, 50);
            error
        };

        assert!(matches!(
            error,
            crate::database::DatabaseError::InsufficientBalance
        ));
        let verify = transaction_of(&mut connection);
        assert_eq!(balance(&verify, 10001, "coins").expect("balance"), 50);
        assert_eq!(count_transactions(&verify, 10001), 1);
        verify.commit().expect("commit");
    }
}
