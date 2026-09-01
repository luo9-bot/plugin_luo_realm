use rusqlite::{Transaction, params};

use crate::{
    combat::{CombatOutcome, CombatSnapshot},
    database::activity,
};

use super::{DatabaseError, DatabaseResult, player_id, unix_timestamp, wallet};

pub fn record_battle(
    transaction: &Transaction<'_>,
    group_id: u64,
    snapshot: &CombatSnapshot,
    outcome: &CombatOutcome,
) -> DatabaseResult<i64> {
    let now = unix_timestamp();
    let snapshot_json = serde_json::to_string(snapshot)
        .map_err(|error| DatabaseError::InvalidData(error.to_string()))?;
    let outcome_json = serde_json::to_string(outcome)
        .map_err(|error| DatabaseError::InvalidData(error.to_string()))?;
    transaction
        .execute(
            "INSERT INTO combat_records(
                 combat_type, group_id, seed, rule_version, winner_team,
                 end_reason, elapsed_ticks, snapshot_json, outcome_json,
                 started_at, finished_at
             ) VALUES('duel', ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
            params![
                player_id(group_id)?,
                snapshot.seed.to_string(),
                snapshot.rule_version,
                outcome.winner_team,
                format!("{:?}", outcome.end_reason).to_ascii_lowercase(),
                outcome.elapsed_ticks,
                snapshot_json,
                outcome_json,
                now,
            ],
        )
        .map_err(DatabaseError::from_sqlite)?;
    let combat_id = transaction.last_insert_rowid();
    snapshot
        .combatants
        .iter()
        .filter_map(|combatant| combatant.player_id.map(|player_id| (player_id, combatant)))
        .try_for_each(|(player_id_value, combatant)| {
            let health = outcome
                .combatants
                .iter()
                .find(|entry| entry.combatant_id == combatant.combatant_id)
                .map(|entry| entry.health)
                .unwrap_or(0);
            transaction
                .execute(
                    "INSERT INTO combat_participants(
                         combat_id, player_id, team, combatant_id, system_id,
                         universal_tier, power_before, hp_before, hp_after
                     ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        combat_id,
                        player_id(player_id_value)?,
                        combatant.team,
                        combatant.combatant_id,
                        combatant.system_id,
                        combatant.universal_tier,
                        combatant.power,
                        combatant.attributes.max_health,
                        health,
                    ],
                )
                .map_err(DatabaseError::from_sqlite)?;
            Ok(())
        })?;
    outcome.events.iter().try_for_each(|event| {
        let event_json = serde_json::to_string(event)
            .map_err(|error| DatabaseError::InvalidData(error.to_string()))?;
        transaction
            .execute(
                "INSERT INTO combat_events(combat_id, sequence, tick, event_json)
                 VALUES(?1, ?2, ?3, ?4)",
                params![combat_id, event.sequence, event.tick, event_json],
            )
            .map_err(DatabaseError::from_sqlite)?;
        Ok(())
    })?;
    let winners = snapshot
        .combatants
        .iter()
        .filter(|combatant| combatant.team == outcome.winner_team)
        .filter_map(|combatant| combatant.player_id);
    winners
        .map(|user_id| {
            wallet::credit(
                transaction,
                user_id,
                "coins",
                500,
                "duel_reward",
                &format!("combat:{combat_id}:{user_id}:coins"),
            )?;
            activity::increment_statistic(transaction, user_id, "wins", 1)
        })
        .collect::<DatabaseResult<Vec<_>>>()?;
    snapshot
        .combatants
        .iter()
        .filter(|combatant| combatant.team != outcome.winner_team)
        .filter_map(|combatant| combatant.player_id)
        .try_for_each(|user_id| activity::increment_statistic(transaction, user_id, "losses", 1))?;
    Ok(combat_id)
}
