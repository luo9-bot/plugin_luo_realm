use rusqlite::{Transaction, params};

use crate::{core::CombatResult, database::activity};

use super::{DatabaseError, DatabaseResult, player_id, unix_timestamp, wallet};

pub struct DuelParticipant<'a> {
    pub user_id: u64,
    pub system_id: &'a str,
    pub realm_index: u32,
    pub power_before: f64,
    pub hp_before: i64,
}

pub fn record_duel(
    transaction: &Transaction<'_>,
    group_id: u64,
    left: DuelParticipant<'_>,
    right: DuelParticipant<'_>,
    result: &CombatResult,
) -> DatabaseResult<i64> {
    let now = unix_timestamp();
    let winner_id: u64 = result
        .winner_id
        .parse()
        .map_err(|_| DatabaseError::InvalidIdentifier)?;
    transaction
        .execute(
            "INSERT INTO combat_records(
                 combat_type, group_id, seed, winner_player_id, rounds, started_at, finished_at
             ) VALUES('duel', ?1, ?2, ?3, ?4, ?5, ?5)",
            params![
                player_id(group_id)?,
                result.seed.to_string(),
                player_id(winner_id)?,
                result.rounds,
                now
            ],
        )
        .map_err(DatabaseError::from_sqlite)?;
    let combat_id = transaction.last_insert_rowid();
    [(0, &left, result.left_hp), (1, &right, result.right_hp)]
        .into_iter()
        .try_for_each(|(side, participant, health)| {
            transaction
                .execute(
                    "INSERT INTO combat_participants(
                         combat_id, player_id, side, system_id, realm_index,
                         power_before, hp_before, hp_after
                     ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        combat_id,
                        player_id(participant.user_id)?,
                        side,
                        participant.system_id,
                        participant.realm_index,
                        participant.power_before.round() as i64,
                        participant.hp_before,
                        health
                    ],
                )
                .map_err(DatabaseError::from_sqlite)?;
            Ok(())
        })?;
    let loser_id = if winner_id == left.user_id {
        right.user_id
    } else {
        left.user_id
    };
    [(winner_id, 500, "wins"), (loser_id, 150, "losses")]
        .into_iter()
        .try_for_each(|(user_id, amount, metric)| {
            wallet::credit(
                transaction,
                user_id,
                "coins",
                amount,
                "duel_reward",
                &format!("combat:{combat_id}:{user_id}:coins"),
            )?;
            activity::increment_statistic(transaction, user_id, metric, 1)
        })?;
    Ok(combat_id)
}
