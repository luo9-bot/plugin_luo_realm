use std::collections::HashSet;

use rusqlite::{OptionalExtension, Transaction, params};

use super::{DatabaseError, DatabaseResult, player_id, unix_timestamp, wallet};

#[derive(Clone, Copy, Debug)]
pub enum ContributionKind {
    CheckIn,
    Destiny,
    Duel,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ContributionResult {
    pub accepted: bool,
    pub completed: bool,
}

struct EventSnapshot {
    name: String,
    description: String,
    coin_reward: i64,
    mark_reward: i64,
}

impl ContributionKind {
    fn code(self) -> &'static str {
        match self {
            Self::CheckIn => "checkin",
            Self::Destiny => "destiny",
            Self::Duel => "duel",
        }
    }

    fn daily_cap(self) -> i64 {
        match self {
            Self::CheckIn | Self::Destiny => 1,
            Self::Duel => 3,
        }
    }
}

pub fn contribute(
    transaction: &Transaction<'_>,
    group_id: u64,
    user_id: u64,
    date: &str,
    kind: ContributionKind,
) -> DatabaseResult<ContributionResult> {
    contribute_many(transaction, group_id, &[user_id], date, kind)
}

pub fn contribute_many(
    transaction: &Transaction<'_>,
    group_id: u64,
    user_ids: &[u64],
    date: &str,
    kind: ContributionKind,
) -> DatabaseResult<ContributionResult> {
    if group_id == 0 {
        return Ok(ContributionResult::default());
    }
    if !super::group::feature_enabled(transaction, group_id, "event")? {
        return Ok(ContributionResult::default());
    }
    let definition = ensure_event(transaction, group_id, date)?;
    let status: String = transaction
        .query_row(
            "SELECT status FROM group_daily_events WHERE group_id=?1 AND event_date=?2",
            params![player_id(group_id)?, date],
            |row| row.get(0),
        )
        .map_err(DatabaseError::from_sqlite)?;
    if status == "completed" {
        return Ok(ContributionResult::default());
    }
    let relevant: bool = transaction
        .query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM group_event_objectives
                 WHERE group_id=?1 AND event_date=?2 AND objective_type=?3
             )",
            params![player_id(group_id)?, date, kind.code()],
            |row| row.get(0),
        )
        .map_err(DatabaseError::from_sqlite)?;
    if !relevant {
        return Ok(ContributionResult::default());
    }

    let group = player_id(group_id)?;
    let mut unique_users = HashSet::with_capacity(user_ids.len());
    let accepted = user_ids
        .iter()
        .copied()
        .filter(|user_id| unique_users.insert(*user_id))
        .try_fold(false, |any_accepted, user_id| {
            record_contribution(transaction, group, user_id, date, kind)
                .map(|accepted| any_accepted || accepted)
        })?;
    if !accepted {
        return Ok(ContributionResult::default());
    }

    let completed = complete_and_reward(transaction, group_id, date, &definition)?;
    Ok(ContributionResult {
        accepted,
        completed,
    })
}

fn record_contribution(
    transaction: &Transaction<'_>,
    group_id: i64,
    user_id: u64,
    date: &str,
    kind: ContributionKind,
) -> DatabaseResult<bool> {
    let player = player_id(user_id)?;
    let current = transaction
        .query_row(
            "SELECT contribution_value FROM group_event_contributions
             WHERE group_id=?1 AND event_date=?2 AND player_id=?3 AND contribution_type=?4",
            params![group_id, date, player, kind.code()],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(DatabaseError::from_sqlite)?
        .unwrap_or(0);
    if current >= kind.daily_cap() {
        return Ok(false);
    }

    transaction
        .execute(
            "INSERT INTO group_event_contributions(
                 group_id, event_date, player_id, contribution_type,
                 contribution_value, updated_at
             ) VALUES(?1, ?2, ?3, ?4, 1, ?5)
             ON CONFLICT(group_id, event_date, player_id, contribution_type) DO UPDATE SET
                 contribution_value=contribution_value+1,
                 updated_at=excluded.updated_at",
            params![group_id, date, player, kind.code(), unix_timestamp()],
        )
        .map_err(DatabaseError::from_sqlite)?;
    transaction
        .execute(
            "UPDATE group_event_objectives
             SET current_value=MIN(target_value, current_value+1)
             WHERE group_id=?1 AND event_date=?2 AND objective_type=?3",
            params![group_id, date, kind.code()],
        )
        .map_err(DatabaseError::from_sqlite)?;
    Ok(true)
}

pub fn summary(transaction: &Transaction<'_>, group_id: u64, date: &str) -> DatabaseResult<String> {
    if group_id == 0 {
        return Ok("群世界事件仅在群聊中开放。".into());
    }
    let definition = ensure_event(transaction, group_id, date)?;
    let group = player_id(group_id)?;
    let status: String = transaction
        .query_row(
            "SELECT status FROM group_daily_events WHERE group_id=?1 AND event_date=?2",
            params![group, date],
            |row| row.get(0),
        )
        .map_err(DatabaseError::from_sqlite)?;
    let objectives = objective_progress(transaction, group, date)?;
    let contributors: i64 = transaction
        .query_row(
            "SELECT COUNT(DISTINCT player_id) FROM group_event_contributions
             WHERE group_id=?1 AND event_date=?2",
            params![group, date],
            |row| row.get(0),
        )
        .map_err(DatabaseError::from_sqlite)?;
    let lines = objectives
        .into_iter()
        .map(|(label, current, target)| format!("- {label}：{current}/{target}"))
        .collect::<Vec<_>>()
        .join("\n");
    let state = if status == "completed" {
        "已完成"
    } else {
        "进行中"
    };
    Ok(format!(
        "今日世界事件·{}（{state}）\n{}\n{}\n贡献者：{contributors} 人\n完成奖励：金币 {}、刻印 {}",
        definition.name,
        definition.description,
        lines,
        definition.coin_reward,
        definition.mark_reward,
    ))
}

fn ensure_event(
    transaction: &Transaction<'_>,
    group_id: u64,
    date: &str,
) -> DatabaseResult<EventSnapshot> {
    super::group::ensure(transaction, group_id)?;
    let group = player_id(group_id)?;
    let existing = transaction
        .query_row(
            "SELECT event_name, description, coin_reward, mark_reward
             FROM group_daily_events WHERE group_id=?1 AND event_date=?2",
            params![group, date],
            |row| {
                Ok(EventSnapshot {
                    name: row.get(0)?,
                    description: row.get(1)?,
                    coin_reward: row.get(2)?,
                    mark_reward: row.get(3)?,
                })
            },
        )
        .optional()
        .map_err(DatabaseError::from_sqlite)?;
    if let Some(snapshot) = existing {
        return Ok(snapshot);
    }

    let definition = crate::engine::world_event::select(date, group_id);
    transaction
        .execute(
            "INSERT INTO group_daily_events(
                 group_id, event_date, definition_id, event_name, description,
                 coin_reward, mark_reward, seed, created_at
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                group,
                date,
                definition.id,
                definition.name,
                definition.description,
                definition.coin_reward,
                definition.mark_reward,
                crate::core::stable_seed(
                    date,
                    "group-world-event",
                    &group_id.to_string(),
                    crate::identity::VERSION_SALT,
                )
                .to_string(),
                unix_timestamp(),
            ],
        )
        .map_err(DatabaseError::from_sqlite)?;
    definition.objectives.iter().try_for_each(|objective| {
        transaction
            .execute(
                "INSERT INTO group_event_objectives(
                     group_id, event_date, objective_id, objective_type,
                     objective_label, target_value
                 ) VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    group,
                    date,
                    objective.id,
                    objective.kind,
                    objective.label,
                    objective.target
                ],
            )
            .map(|_| ())
            .map_err(DatabaseError::from_sqlite)
    })?;
    Ok(EventSnapshot {
        name: definition.name.into(),
        description: definition.description.into(),
        coin_reward: definition.coin_reward,
        mark_reward: definition.mark_reward,
    })
}

fn objective_progress(
    transaction: &Transaction<'_>,
    group_id: i64,
    date: &str,
) -> DatabaseResult<Vec<(String, i64, i64)>> {
    let mut statement = transaction
        .prepare(
            "SELECT objective_label, current_value, target_value
             FROM group_event_objectives
             WHERE group_id=?1 AND event_date=?2 ORDER BY rowid",
        )
        .map_err(DatabaseError::from_sqlite)?;
    statement
        .query_map(params![group_id, date], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .map_err(DatabaseError::from_sqlite)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(DatabaseError::from_sqlite)
}

fn complete_and_reward(
    transaction: &Transaction<'_>,
    group_id: u64,
    date: &str,
    definition: &EventSnapshot,
) -> DatabaseResult<bool> {
    let group = player_id(group_id)?;
    let remaining: i64 = transaction
        .query_row(
            "SELECT COUNT(*) FROM group_event_objectives
             WHERE group_id=?1 AND event_date=?2 AND current_value<target_value",
            params![group, date],
            |row| row.get(0),
        )
        .map_err(DatabaseError::from_sqlite)?;
    if remaining != 0 {
        return Ok(false);
    }
    let completed = transaction
        .execute(
            "UPDATE group_daily_events SET status='completed', completed_at=?3
             WHERE group_id=?1 AND event_date=?2 AND status='active'",
            params![group, date, unix_timestamp()],
        )
        .map_err(DatabaseError::from_sqlite)?
        > 0;

    contributors(transaction, group, date)?
        .into_iter()
        .try_for_each(|user_id| {
            [
                ("coins", definition.coin_reward),
                ("marks", definition.mark_reward),
            ]
            .into_iter()
            .try_for_each(|(currency, amount)| {
                let key = format!("world:{group_id}:{date}:{user_id}:{currency}");
                let entry = wallet::credit(
                    transaction,
                    user_id,
                    currency,
                    amount,
                    "group_world_event",
                    &key,
                )?;
                transaction
                    .execute(
                        "INSERT OR IGNORE INTO group_event_rewards(
                         group_id, event_date, player_id, reward_code, transaction_id, created_at
                     ) VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
                        params![
                            group,
                            date,
                            player_id(user_id)?,
                            currency,
                            entry.transaction_id,
                            unix_timestamp(),
                        ],
                    )
                    .map(|_| ())
                    .map_err(DatabaseError::from_sqlite)
            })
        })?;
    Ok(completed)
}

fn contributors(
    transaction: &Transaction<'_>,
    group_id: i64,
    date: &str,
) -> DatabaseResult<Vec<u64>> {
    let mut statement = transaction
        .prepare(
            "SELECT DISTINCT contribution.player_id
             FROM group_event_contributions contribution
             JOIN players player USING(player_id)
             WHERE contribution.group_id=?1 AND contribution.event_date=?2
               AND player.status='active' AND player.registration_state='active'",
        )
        .map_err(DatabaseError::from_sqlite)?;
    statement
        .query_map(params![group_id, date], |row| row.get::<_, i64>(0))
        .map_err(DatabaseError::from_sqlite)?
        .map(|result| {
            result.and_then(|id| {
                u64::try_from(id).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Integer,
                        Box::new(error),
                    )
                })
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(DatabaseError::from_sqlite)
}
