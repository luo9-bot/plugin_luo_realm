use rusqlite::{Transaction, params};

use crate::{
    combat::{CombatOutcome, CombatSnapshot},
    database::activity,
    domain::shared::GroupId,
};

use super::{DatabaseError, DatabaseResult, player_id, unix_timestamp, wallet};

/// 每位玩家每天最多获得奖励的决斗次数，与世界事件贡献口径一致。
const DAILY_DUEL_REWARD_LIMIT: i64 = 3;

pub fn record_battle(
    transaction: &Transaction<'_>,
    group_id: GroupId,
    date: &str,
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
                player_id(group_id.value())?,
                snapshot.seed.to_string(),
                snapshot.rule_version.value(),
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
        .filter_map(|combatant| {
            combatant
                .platform_user_id
                .map(|platform_user_id| (platform_user_id, combatant))
        })
        .try_for_each(|(platform_user_id, combatant)| {
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
                        player_id(platform_user_id.value())?,
                        combatant.team,
                        combatant.combatant_id.as_str(),
                        combatant.system_id.as_str(),
                        combatant.universal_tier,
                        combatant.power.value(),
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
        .filter_map(|combatant| combatant.platform_user_id);
    winners
        .map(|platform_user_id| {
            let user_id = platform_user_id.value();
            credit_daily_duel_reward(transaction, player_id(group_id.value())?, date, user_id)?;
            activity::increment_statistic(transaction, user_id, "wins", 1)
        })
        .collect::<DatabaseResult<Vec<_>>>()?;
    snapshot
        .combatants
        .iter()
        .filter(|combatant| combatant.team != outcome.winner_team)
        .filter_map(|combatant| combatant.platform_user_id)
        .try_for_each(|platform_user_id| {
            activity::increment_statistic(transaction, platform_user_id.value(), "losses", 1)
        })?;
    Ok(combat_id)
}

/// 按日发放决斗奖励。
///
/// 奖励幂等键为 `duel_reward:<群>:<日期>:<玩家>:<当日序号>`：重复决斗会
/// 重复记录战斗与贡献，但金币奖励在当日上限内按序号入账；达到上限后命中
/// 已存在的幂等键，不再重复发放。
fn credit_daily_duel_reward(
    transaction: &Transaction<'_>,
    group_id: i64,
    date: &str,
    user_id: u64,
) -> DatabaseResult<()> {
    let prefix = format!("duel_reward:{group_id}:{date}:{user_id}");
    let paid: i64 = transaction
        .query_row(
            "SELECT COUNT(*) FROM wallet_transactions WHERE idempotency_key LIKE ?1",
            [format!("{prefix}:%")],
            |row| row.get(0),
        )
        .map_err(DatabaseError::from_sqlite)?;
    let index = (paid + 1).min(DAILY_DUEL_REWARD_LIMIT);
    wallet::credit(
        transaction,
        user_id,
        "coins",
        500,
        "duel_reward",
        &format!("{prefix}:{index}"),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{DAILY_DUEL_REWARD_LIMIT, record_battle};
    use crate::combat::{
        CombatAttributes, CombatSnapshot, CombatantSnapshot, ResourceKind, ResourceSnapshot,
        Tactic, default_loadout, run_battle,
    };
    use crate::database::migrations;
    use crate::domain::shared::RuleVersion;
    use crate::domain::shared::{CombatantId, GroupId, PlatformUserId, PowerScore, SystemId};
    use rusqlite::Connection;

    fn memory_database() -> Connection {
        let mut connection = Connection::open_in_memory().expect("open in-memory database");
        migrations::apply(&mut connection).expect("apply migrations");
        for player in [10001, 10002] {
            connection
                .execute(
                    "INSERT INTO players(player_id, created_at, updated_at) VALUES(?1, 0, 0)",
                    [player],
                )
                .expect("insert player");
        }
        connection
    }

    fn combatant(id: &str, player: u64, team: u8) -> CombatantSnapshot {
        let (active, passive, domain) = default_loadout("sword", 3);
        CombatantSnapshot {
            combatant_id: CombatantId::new(id),
            platform_user_id: Some(PlatformUserId::new(player)),
            display_name: id.into(),
            character_id: "default".into(),
            system_id: SystemId::new("sword"),
            universal_tier: 3,
            team,
            position: team as i32,
            attributes: CombatAttributes {
                max_health: 1_000,
                attack: 140,
                physical_defense: 120,
                arcane_defense: 60,
                soul_defense: 60,
                speed: 40,
                critical_rate_basis_points: 2_000,
                critical_damage_basis_points: 20_000,
                recovery_power: 30,
                control_power: 30,
                tenacity: 500,
                domain_power: 20,
            },
            resource: ResourceSnapshot {
                kind: ResourceKind::SwordIntent,
                current: 100,
                maximum: 100,
                regeneration: 4,
            },
            active_skills: active,
            passive_skills: passive,
            domain_skill: domain,
            equipment_triggers: Vec::new(),
            tactic: Tactic::Aggressive,
            power: PowerScore::saturating_new(9_000),
        }
    }

    fn test_snapshot() -> CombatSnapshot {
        CombatSnapshot {
            rule_version: RuleVersion::INITIAL,
            seed: 20260903,
            rules: crate::combat::BattleRules::default(),
            combatants: vec![combatant("A", 10001, 0), combatant("B", 10002, 1)],
        }
    }

    #[test]
    fn battle_settlement_is_deterministic_and_reward_is_capped_daily() {
        let mut connection = memory_database();
        let transaction = connection.transaction().expect("begin transaction");
        let snapshot = test_snapshot();
        let first = run_battle(&snapshot).expect("first battle");
        let replay = run_battle(&snapshot).expect("replayed battle");
        assert_eq!(first, replay, "相同快照与种子必须产生完全相同的结果");

        let winner = if first.winner_team == 0 { 10001 } else { 10002 };
        for _ in 0..(DAILY_DUEL_REWARD_LIMIT as u32 + 1) {
            record_battle(
                &transaction,
                GroupId::new(30001),
                "2026-09-03",
                &snapshot,
                &first,
            )
            .expect("record battle");
        }
        transaction.commit().expect("commit");

        let balance: i64 = connection
            .query_row(
                "SELECT amount FROM player_balances WHERE player_id=?1 AND currency_code='coins'",
                [winner],
                |row| row.get(0),
            )
            .expect("winner balance");
        assert_eq!(balance, 500 * DAILY_DUEL_REWARD_LIMIT);

        let rewards: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM wallet_transactions
                 WHERE player_id=?1 AND reason_code='duel_reward'",
                [winner],
                |row| row.get(0),
            )
            .expect("reward count");
        assert_eq!(rewards, DAILY_DUEL_REWARD_LIMIT);

        let participants: i64 = connection
            .query_row("SELECT COUNT(*) FROM combat_participants", [], |row| {
                row.get(0)
            })
            .expect("participants");
        assert_eq!(participants, 2 * (DAILY_DUEL_REWARD_LIMIT + 1));

        let losses: i64 = connection
            .query_row(
                "SELECT metric_value FROM player_statistics
                 WHERE player_id=?1 AND metric_code='losses'",
                [if winner == 10001 { 10002 } else { 10001 }],
                |row| row.get(0),
            )
            .expect("loser statistics");
        assert_eq!(losses, DAILY_DUEL_REWARD_LIMIT + 1);
    }
}
