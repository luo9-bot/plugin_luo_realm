use rusqlite::{OptionalExtension, Transaction, params};

use crate::domain::rule_versions;

use super::{DatabaseError, DatabaseResult, player_id};

pub struct DailyEventResult {
    pub definition_id: String,
    pub created: bool,
}

pub fn daily_event(
    transaction: &Transaction<'_>,
    user_id: u64,
    date: &str,
    definition_id: &str,
    seed: &str,
) -> DatabaseResult<DailyEventResult> {
    let player_id = player_id(user_id)?;
    if let Some(existing) = transaction
        .query_row(
            "SELECT definition_id FROM destiny_events
             WHERE player_id=?1 AND event_date=?2 LIMIT 1",
            params![player_id, date],
            |row| row.get(0),
        )
        .optional()
        .map_err(DatabaseError::from_sqlite)?
    {
        return Ok(DailyEventResult {
            definition_id: existing,
            created: false,
        });
    }
    transaction
        .execute(
            "INSERT INTO destiny_events(player_id, event_date, definition_id, seed, rule_version)
             VALUES(?1, ?2, ?3, ?4, ?5)",
            params![
                player_id,
                date,
                definition_id,
                seed,
                rule_versions::DESTINY.value()
            ],
        )
        .map_err(DatabaseError::from_sqlite)?;
    Ok(DailyEventResult {
        definition_id: definition_id.to_owned(),
        created: true,
    })
}

#[cfg(test)]
mod tests {
    use super::daily_event;
    use crate::database::migrations;
    use crate::domain::rule_versions;
    use rusqlite::Connection;

    #[test]
    fn daily_event_records_rule_version_and_is_idempotent() {
        let mut connection = Connection::open_in_memory().expect("open in-memory database");
        migrations::apply(&mut connection).expect("apply migrations");
        connection
            .execute(
                "INSERT INTO players(player_id, created_at, updated_at) VALUES(10001, 0, 0)",
                [],
            )
            .expect("insert player");
        let transaction = connection.transaction().expect("begin transaction");
        let first = daily_event(&transaction, 10001, "2026-09-03", "平静的一日", "123")
            .expect("first destiny");
        let second = daily_event(&transaction, 10001, "2026-09-03", "资源潮汐", "456")
            .expect("second destiny");
        transaction.commit().expect("commit");

        assert!(first.created);
        assert!(!second.created);
        assert_eq!(second.definition_id, "平静的一日");
        let stored: u32 = connection
            .query_row(
                "SELECT rule_version FROM destiny_events WHERE player_id=10001",
                [],
                |row| row.get(0),
            )
            .expect("stored rule version");
        assert_eq!(stored, rule_versions::DESTINY.value());
    }
}
