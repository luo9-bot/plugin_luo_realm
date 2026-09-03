use std::collections::HashMap;

use rusqlite::{OptionalExtension, Transaction, params};

use crate::combat::{
    SkillCategory, SkillDefinition, SkillVisualConfig, Tactic, active_slot_capacity,
    default_loadout, skill_by_id, skills_for_system,
};

use super::{DatabaseError, DatabaseResult, player_id, unix_timestamp};

pub struct PlayerSkill {
    pub definition: SkillDefinition,
    pub mastery: u8,
    pub branch_code: Option<String>,
}

pub struct BattleLoadout {
    pub active: Vec<SkillDefinition>,
    pub passive: Vec<SkillDefinition>,
    pub domain: Option<SkillDefinition>,
    pub tactic: Tactic,
}

#[derive(serde::Serialize)]
pub struct SkillConfig {
    pub definition: SkillDefinition,
    pub visual: SkillVisualConfig,
    pub enabled: bool,
}

pub fn list_configs(connection: &rusqlite::Connection) -> DatabaseResult<Vec<SkillConfig>> {
    let mut statement = connection
        .prepare("SELECT skill_id, definition_json, visual_json, enabled FROM combat_skill_configs ORDER BY skill_id")
        .map_err(DatabaseError::from_sqlite)?;
    let configured = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, bool>(3)?,
            ))
        })
        .map_err(DatabaseError::from_sqlite)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(DatabaseError::from_sqlite)?;
    let mut overrides = configured
        .into_iter()
        .map(|(_, definition, visual, enabled)| {
            let definition: SkillDefinition = serde_json::from_str(&definition)
                .map_err(|error| DatabaseError::InvalidData(error.to_string()))?;
            let visual: SkillVisualConfig = serde_json::from_str(&visual)
                .map_err(|error| DatabaseError::InvalidData(error.to_string()))?;
            Ok((
                definition.id.clone(),
                SkillConfig {
                    definition,
                    visual,
                    enabled,
                },
            ))
        })
        .collect::<DatabaseResult<HashMap<_, _>>>()?;
    let mut result = [
        "orthodox",
        "sword",
        "body",
        "mage",
        "soul",
        "qi",
        "blood_demon",
        "formation",
        "alchemy_artifact",
        "summoner",
        "music",
    ]
    .into_iter()
    .flat_map(skills_for_system)
    .map(|definition| {
        let id = definition.id.clone();
        overrides.remove(&id).unwrap_or(SkillConfig {
            definition,
            visual: SkillVisualConfig::default(),
            enabled: true,
        })
    })
    .collect::<Vec<_>>();
    result.extend(overrides.into_values());
    result.sort_unstable_by(|left, right| left.definition.id.cmp(&right.definition.id));
    Ok(result)
}

pub fn upsert_config(
    transaction: &Transaction<'_>,
    definition: &SkillDefinition,
    visual: &SkillVisualConfig,
    enabled: bool,
) -> DatabaseResult<()> {
    let definition_json = serde_json::to_string(definition)
        .map_err(|error| DatabaseError::InvalidData(error.to_string()))?;
    let visual_json = serde_json::to_string(visual)
        .map_err(|error| DatabaseError::InvalidData(error.to_string()))?;
    transaction
        .execute(
            "INSERT INTO combat_skill_configs(skill_id, definition_json, visual_json, enabled, updated_at)
             VALUES(?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(skill_id) DO UPDATE SET definition_json=excluded.definition_json,
                 visual_json=excluded.visual_json, enabled=excluded.enabled, updated_at=excluded.updated_at",
            params![
                definition.id.as_str(),
                definition_json,
                visual_json,
                enabled,
                unix_timestamp()
            ],
        )
        .map_err(DatabaseError::from_sqlite)?;
    Ok(())
}

pub fn delete_config(transaction: &Transaction<'_>, skill_id: &str) -> DatabaseResult<()> {
    let changed = transaction
        .execute(
            "DELETE FROM combat_skill_configs WHERE skill_id=?1",
            [skill_id],
        )
        .map_err(DatabaseError::from_sqlite)?;
    if changed == 0 {
        return Err(DatabaseError::NotFound);
    }
    Ok(())
}

fn configured_definition(
    transaction: &Transaction<'_>,
    skill_id: &str,
) -> DatabaseResult<Option<SkillDefinition>> {
    let value = transaction
        .query_row(
            "SELECT definition_json, enabled FROM combat_skill_configs WHERE skill_id=?1",
            [skill_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?)),
        )
        .optional()
        .map_err(DatabaseError::from_sqlite)?;
    value
        .filter(|(_, enabled)| *enabled)
        .map(|(definition, _)| {
            serde_json::from_str(&definition)
                .map_err(|error| DatabaseError::InvalidData(error.to_string()))
        })
        .transpose()
}

pub fn ensure_unlocked(
    transaction: &Transaction<'_>,
    user_id: u64,
    system_id: &str,
    tier: u8,
) -> DatabaseResult<()> {
    let id = player_id(user_id)?;
    skills_for_system(system_id)
        .into_iter()
        .filter(|skill| skill.unlock_tier <= tier)
        .try_for_each(|skill| {
            transaction
                .execute(
                    "INSERT OR IGNORE INTO player_skills(
                         player_id, skill_id, mastery, acquired_at
                     ) VALUES(?1, ?2, 0, ?3)",
                    params![id, skill.id.as_str(), unix_timestamp()],
                )
                .map_err(DatabaseError::from_sqlite)?;
            Ok(())
        })?;
    ensure_default_loadout(transaction, user_id, system_id, tier)
}

pub fn list(transaction: &Transaction<'_>, user_id: u64) -> DatabaseResult<Vec<PlayerSkill>> {
    let mut statement = transaction
        .prepare(
            "SELECT skill_id, mastery, branch_code FROM player_skills
             WHERE player_id=?1 ORDER BY acquired_at, skill_id",
        )
        .map_err(DatabaseError::from_sqlite)?;
    let rows = statement
        .query_map([player_id(user_id)?], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, u8>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })
        .map_err(DatabaseError::from_sqlite)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(DatabaseError::from_sqlite)?;
    rows.into_iter()
        .map(|(skill_id, mastery, branch_code)| {
            let definition = configured_definition(transaction, &skill_id)?
                .or_else(|| skill_by_id(&skill_id))
                .ok_or_else(|| DatabaseError::InvalidData(format!("技能定义不存在：{skill_id}")))?;
            Ok(PlayerSkill {
                definition,
                mastery,
                branch_code,
            })
        })
        .collect()
}

pub fn loadout(
    transaction: &Transaction<'_>,
    user_id: u64,
    system_id: &str,
    tier: u8,
) -> DatabaseResult<BattleLoadout> {
    ensure_unlocked(transaction, user_id, system_id, tier)?;
    let id = player_id(user_id)?;
    let mut statement = transaction
        .prepare(
            "SELECT loadout.slot_type, loadout.slot_index, loadout.skill_id,
                    skills.mastery
             FROM player_skill_loadouts loadout
             JOIN player_skills skills
               ON skills.player_id=loadout.player_id AND skills.skill_id=loadout.skill_id
             WHERE loadout.player_id=?1 ORDER BY loadout.slot_type, loadout.slot_index",
        )
        .map_err(DatabaseError::from_sqlite)?;
    let rows = statement
        .query_map([id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, usize>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, u8>(3)?,
            ))
        })
        .map_err(DatabaseError::from_sqlite)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(DatabaseError::from_sqlite)?;
    let definitions = rows
        .into_iter()
        .map(|(slot_type, slot_index, skill_id, mastery)| {
            configured_definition(transaction, &skill_id)?
                .or_else(|| skill_by_id(&skill_id))
                .map(|mut definition| {
                    definition.mastery = mastery;
                    (slot_type, slot_index, definition)
                })
                .ok_or_else(|| DatabaseError::InvalidData(format!("技能定义不存在：{skill_id}")))
        })
        .collect::<DatabaseResult<Vec<_>>>()?;
    let active = definitions
        .iter()
        .filter(|(slot_type, _, _)| slot_type == "active")
        .map(|(_, _, definition)| definition.clone())
        .collect();
    let passive = definitions
        .iter()
        .filter(|(slot_type, _, _)| slot_type == "passive")
        .map(|(_, _, definition)| definition.clone())
        .collect();
    let domain = definitions
        .into_iter()
        .find(|(slot_type, _, _)| slot_type == "domain")
        .map(|(_, _, definition)| definition);
    Ok(BattleLoadout {
        active,
        passive,
        domain,
        tactic: tactic(transaction, user_id)?,
    })
}

pub fn configure(
    transaction: &Transaction<'_>,
    user_id: u64,
    tier: u8,
    slot_type: &str,
    slot_index: usize,
    skill_id: &str,
) -> DatabaseResult<()> {
    let category = match slot_type {
        "active" => SkillCategory::Active,
        "passive" => SkillCategory::Passive,
        "domain" => SkillCategory::Domain,
        _ => return Err(DatabaseError::InvalidData("未知技能槽类型".into())),
    };
    let capacity = match category {
        SkillCategory::Active => active_slot_capacity(tier),
        SkillCategory::Passive => crate::combat::passive_slot_capacity(tier),
        SkillCategory::Domain => usize::from(tier >= 3),
    };
    if slot_index >= capacity {
        return Err(DatabaseError::InvalidData("技能槽尚未解锁".into()));
    }
    let definition = configured_definition(transaction, skill_id)?
        .or_else(|| skill_by_id(skill_id))
        .filter(|skill| skill.category == category && skill.unlock_tier <= tier)
        .ok_or_else(|| DatabaseError::InvalidData("技能与槽位不匹配或尚未解锁".into()))?;
    let id = player_id(user_id)?;
    let owned = transaction
        .query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM player_skills WHERE player_id=?1 AND skill_id=?2
             )",
            params![id, definition.id.as_str()],
            |row| row.get::<_, bool>(0),
        )
        .map_err(DatabaseError::from_sqlite)?;
    if !owned {
        return Err(DatabaseError::InvalidData("尚未掌握该技能".into()));
    }
    transaction
        .execute(
            "DELETE FROM player_skill_loadouts WHERE player_id=?1 AND skill_id=?2",
            params![id, definition.id.as_str()],
        )
        .map_err(DatabaseError::from_sqlite)?;
    transaction
        .execute(
            "INSERT INTO player_skill_loadouts(player_id, slot_type, slot_index, skill_id)
             VALUES(?1, ?2, ?3, ?4)
             ON CONFLICT(player_id, slot_type, slot_index) DO UPDATE SET
                 skill_id=excluded.skill_id",
            params![id, slot_type, slot_index, definition.id.as_str()],
        )
        .map_err(DatabaseError::from_sqlite)?;
    Ok(())
}

pub fn set_tactic(
    transaction: &Transaction<'_>,
    user_id: u64,
    tactic: Tactic,
) -> DatabaseResult<()> {
    transaction
        .execute(
            "INSERT INTO player_battle_tactics(player_id, tactic_code, updated_at)
             VALUES(?1, ?2, ?3)
             ON CONFLICT(player_id) DO UPDATE SET
                 tactic_code=excluded.tactic_code, updated_at=excluded.updated_at",
            params![player_id(user_id)?, tactic.code(), unix_timestamp()],
        )
        .map_err(DatabaseError::from_sqlite)?;
    Ok(())
}

pub fn train(transaction: &Transaction<'_>, user_id: u64, skill_id: &str) -> DatabaseResult<u8> {
    let changed = transaction
        .execute(
            "UPDATE player_skills SET mastery=MIN(3, mastery+1)
             WHERE player_id=?1 AND skill_id=?2",
            params![player_id(user_id)?, skill_id],
        )
        .map_err(DatabaseError::from_sqlite)?;
    if changed != 1 {
        return Err(DatabaseError::NotFound);
    }
    transaction
        .query_row(
            "SELECT mastery FROM player_skills WHERE player_id=?1 AND skill_id=?2",
            params![player_id(user_id)?, skill_id],
            |row| row.get(0),
        )
        .map_err(DatabaseError::from_sqlite)
}

fn ensure_default_loadout(
    transaction: &Transaction<'_>,
    user_id: u64,
    system_id: &str,
    tier: u8,
) -> DatabaseResult<()> {
    let id = player_id(user_id)?;
    let (active, passive, domain) = default_loadout(system_id, tier);
    active
        .into_iter()
        .enumerate()
        .map(|(index, skill)| ("active", index, skill))
        .chain(
            passive
                .into_iter()
                .enumerate()
                .map(|(index, skill)| ("passive", index, skill)),
        )
        .chain(domain.into_iter().map(|skill| ("domain", 0, skill)))
        .try_for_each(|(slot_type, index, skill)| {
            transaction
                .execute(
                    "INSERT OR IGNORE INTO player_skill_loadouts(
                         player_id, slot_type, slot_index, skill_id
                     ) VALUES(?1, ?2, ?3, ?4)",
                    params![id, slot_type, index, skill.id.as_str()],
                )
                .map_err(DatabaseError::from_sqlite)?;
            Ok(())
        })
}

/// 只读查询当前战术预设；未设置时返回默认 `balanced`。
pub fn current_tactic(transaction: &Transaction<'_>, user_id: u64) -> DatabaseResult<Tactic> {
    tactic(transaction, user_id)
}

fn tactic(transaction: &Transaction<'_>, user_id: u64) -> DatabaseResult<Tactic> {
    let code = transaction
        .query_row(
            "SELECT tactic_code FROM player_battle_tactics WHERE player_id=?1",
            [player_id(user_id)?],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(DatabaseError::from_sqlite)?
        .unwrap_or_else(|| "balanced".into());
    Tactic::from_code(&code).ok_or_else(|| DatabaseError::InvalidData(format!("未知战术：{code}")))
}

#[cfg(test)]
mod tests {
    use super::{configure, ensure_unlocked, list, loadout, set_tactic, train};
    use crate::combat::Tactic;
    use crate::database::DatabaseError;
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
            .execute(
                "INSERT INTO player_cultivation(player_id, system_id, realm_index, updated_at)
                 VALUES(10001, 'sword', 3, 0)",
                [],
            )
            .expect("insert cultivation");
        connection
    }

    #[test]
    fn configure_enforces_ownership_capacity_and_uniqueness() {
        let mut connection = memory_database();
        let transaction = connection.transaction().expect("begin transaction");
        ensure_unlocked(&transaction, 10001, "sword", 3).expect("unlock skills");
        let owned = list(&transaction, 10001).expect("skill list");
        let skill_id = owned[0].definition.id.as_str().to_owned();

        configure(&transaction, 10001, 3, "active", 0, &skill_id).expect("first configure");
        // 同一技能重复配置是合法的“移动”语义：先删除旧槽位，再写入新槽位。
        configure(&transaction, 10001, 3, "active", 1, &skill_id).expect("relocate");
        let loadout_rows: i64 = transaction
            .query_row(
                "SELECT COUNT(*) FROM player_skill_loadouts
                 WHERE player_id=10001 AND skill_id=?1",
                [&skill_id],
                |row| row.get(0),
            )
            .expect("loadout rows");
        assert_eq!(loadout_rows, 1, "同一技能只能占据一个配置槽位");

        transaction
            .execute(
                "DELETE FROM player_skills WHERE player_id=10001 AND skill_id=?1",
                [&skill_id],
            )
            .expect("revoke skill");
        let unowned =
            configure(&transaction, 10001, 3, "active", 1, &skill_id).expect_err("unowned skill");
        assert!(matches!(unowned, DatabaseError::InvalidData(_)));

        let unknown_slot =
            configure(&transaction, 10001, 0, "active", 50, &skill_id).expect_err("locked slot");
        assert!(matches!(unknown_slot, DatabaseError::InvalidData(_)));
        let bad_type = configure(&transaction, 10001, 3, "ultimate", 0, &skill_id)
            .expect_err("unknown slot type");
        assert!(matches!(bad_type, DatabaseError::InvalidData(_)));
        transaction.commit().expect("commit");

        let verify = connection.transaction().expect("begin transaction");
        let loadout = loadout(&verify, 10001, "sword", 3).expect("loadout");
        let occurrences = loadout
            .active
            .iter()
            .filter(|skill| skill.id.as_str() == skill_id)
            .count();
        assert_eq!(occurrences, 1, "目标技能只出现在移动后的槽位");
    }

    #[test]
    fn train_caps_mastery_at_three_and_tactic_persists() {
        let mut connection = memory_database();
        let transaction = connection.transaction().expect("begin transaction");
        ensure_unlocked(&transaction, 10001, "sword", 3).expect("unlock skills");
        let owned = list(&transaction, 10001).expect("skill list");
        let skill_id = owned[0].definition.id.as_str().to_owned();

        let first = train(&transaction, 10001, &skill_id).expect("first train");
        let second = train(&transaction, 10001, &skill_id).expect("second train");
        let third = train(&transaction, 10001, &skill_id).expect("third train");
        let fourth = train(&transaction, 10001, &skill_id).expect("capped train");
        let unknown = train(&transaction, 10001, "sword.does_not_exist").expect_err("unknown");
        assert!(matches!(unknown, DatabaseError::NotFound));

        set_tactic(&transaction, 10001, Tactic::Aggressive).expect("set tactic");
        transaction.commit().expect("commit");

        assert_eq!((first, second, third), (1, 2, 3));
        assert_eq!(fourth, 3, "熟练度到顶后重复研习保持 3");

        let verify = connection.transaction().expect("begin transaction");
        let loadout = loadout(&verify, 10001, "sword", 3).expect("loadout");
        assert_eq!(loadout.tactic, Tactic::Aggressive);
        let mastery = list(&verify, 10001)
            .expect("list")
            .into_iter()
            .find(|skill| skill.definition.id.as_str() == skill_id)
            .expect("trained skill")
            .mastery;
        assert_eq!(mastery, 3);
    }
}
