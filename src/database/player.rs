use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::core::Player;

use super::{DatabaseError, DatabaseResult, player_id, unix_timestamp};

const DISPLAY_NAME_MAX_CHARS: usize = 20;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegistrationState {
    Missing,
    PendingSystem,
    Active,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegisterResult {
    Created,
    PendingSystem,
    AlreadyActive,
    Unavailable,
}

/// 为既有维护工具创建可直接使用的默认角色。
///
/// 消息命令不得调用此函数；正常玩家必须依次完成 `register` 和 `activate_system`。
#[doc(hidden)]
pub fn find_or_create(transaction: &Transaction<'_>, user_id: u64) -> DatabaseResult<Player> {
    match register(transaction, user_id, "LR·旅者")? {
        RegisterResult::Created | RegisterResult::PendingSystem => {
            activate_system(transaction, user_id, "orthodox")?;
        }
        RegisterResult::AlreadyActive => {}
        RegisterResult::Unavailable => return Err(DatabaseError::NotFound),
    }
    get_active(transaction, user_id)?.ok_or(DatabaseError::NotFound)
}

pub fn registration_state(
    connection: &Connection,
    user_id: u64,
) -> DatabaseResult<RegistrationState> {
    let state = connection
        .query_row(
            "SELECT status, registration_state FROM players WHERE player_id=?1",
            [player_id(user_id)?],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(DatabaseError::from_sqlite)?;

    Ok(match state {
        None => RegistrationState::Missing,
        Some((status, _)) if status != "active" => RegistrationState::Unavailable,
        Some((_, registration)) if registration == "pending_system" => {
            RegistrationState::PendingSystem
        }
        Some((_, registration)) if registration == "active" => RegistrationState::Active,
        Some(_) => RegistrationState::Unavailable,
    })
}

pub fn register(
    transaction: &Transaction<'_>,
    user_id: u64,
    display_name: &str,
) -> DatabaseResult<RegisterResult> {
    let display_name = validated_display_name(display_name)?;
    match registration_state(transaction, user_id)? {
        RegistrationState::PendingSystem => return Ok(RegisterResult::PendingSystem),
        RegistrationState::Active => return Ok(RegisterResult::AlreadyActive),
        RegistrationState::Unavailable => return Ok(RegisterResult::Unavailable),
        RegistrationState::Missing => {}
    }

    let player_id = player_id(user_id)?;
    let now = unix_timestamp();
    transaction
        .execute(
            "INSERT INTO players(
                 player_id, status, registration_state, created_at, updated_at
             ) VALUES(?1, 'active', 'pending_system', ?2, ?2)",
            params![player_id, now],
        )
        .map_err(DatabaseError::from_sqlite)?;
    transaction
        .execute(
            "INSERT INTO player_profiles(player_id, display_name) VALUES(?1, ?2)",
            params![player_id, display_name],
        )
        .map_err(DatabaseError::from_sqlite)?;

    Ok(RegisterResult::Created)
}

pub fn activate_system(
    transaction: &Transaction<'_>,
    user_id: u64,
    system_id: &str,
) -> DatabaseResult<bool> {
    if system_id.trim().is_empty() {
        return Err(DatabaseError::InvalidData(
            "cultivation system must not be empty".into(),
        ));
    }
    if registration_state(transaction, user_id)? != RegistrationState::PendingSystem {
        return Ok(false);
    }

    let player_id = player_id(user_id)?;
    let now = unix_timestamp();
    transaction
        .execute(
            "INSERT INTO player_cultivation(player_id, system_id, updated_at)
             VALUES(?1, ?2, ?3)",
            params![player_id, system_id, now],
        )
        .map_err(DatabaseError::from_sqlite)?;
    let changed = transaction
        .execute(
            "UPDATE players SET registration_state='active', revision=revision+1, updated_at=?2
             WHERE player_id=?1 AND status='active' AND registration_state='pending_system'",
            params![player_id, now],
        )
        .map_err(DatabaseError::from_sqlite)?;

    Ok(changed == 1)
}

pub fn get_active(transaction: &Transaction<'_>, user_id: u64) -> DatabaseResult<Option<Player>> {
    let player_id = player_id(user_id)?;
    transaction
        .query_row(
            "SELECT p.player_id, profile.display_name, profile.character_id, cultivation.realm_index,
                    COALESCE(coins.amount, 0), COALESCE(marks.amount, 0),
                    COALESCE(wins.metric_value, 0), COALESCE(losses.metric_value, 0)
             FROM players p
             JOIN player_profiles profile USING(player_id)
             JOIN player_cultivation cultivation USING(player_id)
             LEFT JOIN player_balances coins
                    ON coins.player_id=p.player_id AND coins.currency_code='coins'
             LEFT JOIN player_balances marks
                    ON marks.player_id=p.player_id AND marks.currency_code='marks'
             LEFT JOIN player_statistics wins
                    ON wins.player_id=p.player_id AND wins.metric_code='wins'
             LEFT JOIN player_statistics losses
                    ON losses.player_id=p.player_id AND losses.metric_code='losses'
             WHERE p.player_id=?1 AND p.status='active' AND p.registration_state='active'",
            [player_id],
            |row| {
                let id: i64 = row.get(0)?;
                let realm_index: u32 = row.get(3)?;
                Ok(Player {
                    user_id: id.to_string(),
                    display_name: row.get(1)?,
                    character_id: row.get(2)?,
                    level: realm_index + 1,
                    experience: 0,
                    coins: row.get::<_, i64>(4)?.max(0) as u64,
                    marks: row.get::<_, i64>(5)?.max(0) as u64,
                    base_hp: 1000 + i64::from(realm_index) * 200,
                    base_attack: 100 + i64::from(realm_index) * 30,
                    base_defense: 50 + i64::from(realm_index) * 15,
                    critical_rate: 5.0,
                    critical_multiplier: 1.5,
                    speed: 10 + i64::from(realm_index),
                    wins: row.get::<_, i64>(6)?.max(0) as u64,
                    losses: row.get::<_, i64>(7)?.max(0) as u64,
                })
            },
        )
        .optional()
        .map_err(DatabaseError::from_sqlite)
}

pub fn rename(
    transaction: &Transaction<'_>,
    user_id: u64,
    display_name: &str,
) -> DatabaseResult<String> {
    let display_name = validated_display_name(display_name)?;
    let changed = transaction
        .execute(
            "UPDATE player_profiles SET display_name=?2
             WHERE player_id=?1 AND EXISTS(
                 SELECT 1 FROM players
                 WHERE player_id=?1 AND status!='deleted'
             )",
            params![player_id(user_id)?, display_name],
        )
        .map_err(DatabaseError::from_sqlite)?;
    if changed == 0 {
        return Err(DatabaseError::NotFound);
    }
    Ok(display_name)
}

fn validated_display_name(display_name: &str) -> DatabaseResult<String> {
    let normalized = display_name.trim();
    let char_count = normalized.chars().count();
    if char_count == 0
        || char_count > DISPLAY_NAME_MAX_CHARS
        || normalized.chars().any(char::is_control)
    {
        return Err(DatabaseError::InvalidData(
            "display name must contain 1 to 20 visible characters".into(),
        ));
    }
    Ok(normalized.to_owned())
}
