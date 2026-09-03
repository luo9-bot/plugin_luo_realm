use rusqlite::{OptionalExtension, Transaction, params};

use crate::domain::shared::RuleVersion;
use crate::engine::daily_state::{DailyModifiers, DailyState, DailyStateInput};

use super::{DatabaseError, DatabaseResult, player_id, unix_timestamp};

pub fn get_or_create(
    transaction: &Transaction<'_>,
    user_id: u64,
    date: &str,
) -> DatabaseResult<DailyState> {
    if let Some(state) = find(transaction, user_id, date)? {
        return Ok(state);
    }

    let input = load_input(transaction, user_id)?;
    let state = crate::engine::daily_state::generate(date, &input);
    let source_json = serde_json::to_string(&input)
        .map_err(|error| DatabaseError::InvalidData(error.to_string()))?;
    let id = player_id(user_id)?;
    transaction
        .execute(
            "INSERT INTO player_daily_states(
                 player_id, state_date, state_id, state_name, description,
                 hp_modifier, attack_modifier, defense_modifier, speed_modifier,
                 critical_modifier, destiny_modifier, source_json, seed,
                 rule_version, created_at
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                id,
                date,
                state.id,
                state.name,
                state.description,
                state.modifiers.hp,
                state.modifiers.attack,
                state.modifiers.defense,
                state.modifiers.speed,
                state.modifiers.critical,
                state.modifiers.destiny,
                source_json,
                state.seed.to_string(),
                state.rule_version.value(),
                unix_timestamp(),
            ],
        )
        .map_err(DatabaseError::from_sqlite)?;
    Ok(state)
}

fn find(
    transaction: &Transaction<'_>,
    user_id: u64,
    date: &str,
) -> DatabaseResult<Option<DailyState>> {
    transaction
        .query_row(
            "SELECT state_id, state_name, description, hp_modifier, attack_modifier,
                    defense_modifier, speed_modifier, critical_modifier,
                    destiny_modifier, seed, rule_version
             FROM player_daily_states WHERE player_id=?1 AND state_date=?2",
            params![player_id(user_id)?, date],
            |row| {
                let version = row.get::<_, u32>(10)?;
                let rule_version = RuleVersion::new(version).ok_or_else(|| {
                    rusqlite::Error::FromSqlConversionFailure(
                        10,
                        rusqlite::types::Type::Integer,
                        "rule_version 必须为正数".into(),
                    )
                })?;
                Ok(DailyState {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    modifiers: DailyModifiers {
                        hp: row.get(3)?,
                        attack: row.get(4)?,
                        defense: row.get(5)?,
                        speed: row.get(6)?,
                        critical: row.get(7)?,
                        destiny: row.get(8)?,
                    },
                    seed: row.get::<_, String>(9)?.parse().map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            9,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?,
                    rule_version,
                })
            },
        )
        .optional()
        .map_err(DatabaseError::from_sqlite)
}

fn load_input(transaction: &Transaction<'_>, user_id: u64) -> DatabaseResult<DailyStateInput> {
    let id = player_id(user_id)?;
    let cultivation = transaction
        .query_row(
            "SELECT system_id, realm_index, progress, foundation, comprehension, deviation
             FROM player_cultivation WHERE player_id=?1",
            [id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .map_err(DatabaseError::from_sqlite)?;
    let checkin_streak = transaction
        .query_row(
            "SELECT streak FROM daily_checkins WHERE player_id=?1
             ORDER BY checkin_date DESC LIMIT 1",
            [id],
            |row| row.get(0),
        )
        .optional()
        .map_err(DatabaseError::from_sqlite)?
        .unwrap_or(0);
    let (recent_wins, recent_losses) = recent_combat(transaction, id)?;
    let recent_destinies = transaction
        .query_row(
            "SELECT COUNT(*) FROM (
                 SELECT event_id FROM destiny_events WHERE player_id=?1
                 ORDER BY event_date DESC LIMIT 3
             )",
            [id],
            |row| row.get(0),
        )
        .map_err(DatabaseError::from_sqlite)?;
    let previous_states = previous_states(transaction, id)?;

    Ok(DailyStateInput {
        user_id,
        system_id: cultivation.0,
        realm_index: cultivation.1,
        progress: cultivation.2,
        foundation: cultivation.3,
        comprehension: cultivation.4,
        deviation: cultivation.5,
        checkin_streak,
        recent_wins,
        recent_losses,
        recent_destinies,
        previous_states,
    })
}

fn recent_combat(transaction: &Transaction<'_>, id: i64) -> DatabaseResult<(u32, u32)> {
    transaction
        .query_row(
            "SELECT COALESCE(SUM(won), 0), COALESCE(SUM(1-won), 0) FROM (
                 SELECT CASE WHEN participant.team=record.winner_team THEN 1 ELSE 0 END AS won
                 FROM combat_participants participant
                 JOIN combat_records record USING(combat_id)
                 WHERE participant.player_id=?1
                 ORDER BY record.finished_at DESC LIMIT 5
             )",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(DatabaseError::from_sqlite)
}

fn previous_states(transaction: &Transaction<'_>, id: i64) -> DatabaseResult<Vec<String>> {
    let mut statement = transaction
        .prepare(
            "SELECT state_id FROM player_daily_states WHERE player_id=?1
             ORDER BY state_date DESC LIMIT 3",
        )
        .map_err(DatabaseError::from_sqlite)?;
    statement
        .query_map([id], |row| row.get(0))
        .map_err(DatabaseError::from_sqlite)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(DatabaseError::from_sqlite)
}

#[cfg(test)]
mod tests {
    use super::get_or_create;
    use crate::database::migrations;
    use crate::domain::rule_versions;
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
            .execute(
                "INSERT INTO player_cultivation(player_id, system_id, updated_at)
                 VALUES(10001, 'sword', 0)",
                [],
            )
            .expect("insert cultivation");
        connection
    }

    #[test]
    fn daily_state_records_rule_version_and_is_idempotent() {
        let mut connection = memory_database();
        let transaction = connection.transaction().expect("begin transaction");
        let first = get_or_create(&transaction, 10001, "2026-09-03").expect("first state");
        let second = get_or_create(&transaction, 10001, "2026-09-03").expect("second state");
        transaction.commit().expect("commit");

        assert_eq!(first.id, second.id);
        assert_eq!(first.seed, second.seed);
        assert_eq!(first.rule_version, rule_versions::DAILY_STATE);
        let stored: u32 = connection
            .query_row(
                "SELECT rule_version FROM player_daily_states WHERE player_id=10001",
                [],
                |row| row.get(0),
            )
            .expect("stored rule version");
        assert_eq!(stored, rule_versions::DAILY_STATE.value());
    }
}
