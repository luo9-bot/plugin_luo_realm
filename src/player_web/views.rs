//! 玩家网页的只读视图模型。
//!
//! 全部函数只执行 `SELECT`，不触发生成逻辑：网页永远不会替玩家创建每日
//! 状态或解锁技能（设计方案书 27.2“每个请求从会话重新解析身份”）。

use serde::Serialize;

use crate::database::{DatabaseError, DatabaseResult, daily_state, inventory, player, skills};
use crate::domain::shared::PlatformUserId;
use crate::engine;

/// 档案视图：身份、体系、境界、战力与当日状态。
#[derive(Serialize)]
pub struct ProfileView {
    pub display_name: String,
    pub character_id: String,
    pub biography: String,
    pub system_id: String,
    pub system_name: String,
    pub realm_index: u32,
    pub realm_name: String,
    pub power: f64,
    pub daily_state: Option<DailyStateView>,
    pub registered_at: i64,
}

#[derive(Serialize)]
pub struct DailyStateView {
    pub id: String,
    pub name: String,
    pub description: String,
    pub rule_version: u32,
}

/// 钱包视图：余额与最近流水。
#[derive(Serialize)]
pub struct WalletView {
    pub balances: Vec<BalanceView>,
    pub transactions: Vec<TransactionView>,
}

#[derive(Serialize)]
pub struct BalanceView {
    pub currency: String,
    pub amount: i64,
}

#[derive(Serialize)]
pub struct TransactionView {
    pub reason: String,
    pub delta: i64,
    pub balance_after: i64,
    pub created_at: i64,
}

/// 技能视图：已掌握技能与当前战术。
#[derive(Serialize)]
pub struct SkillsView {
    pub skills: Vec<SkillView>,
    pub tactic: String,
}

#[derive(Serialize)]
pub struct SkillView {
    pub id: String,
    pub name: String,
    pub mastery: u8,
}

/// 背包与装备视图。
#[derive(Serialize)]
pub struct EquipmentView {
    pub items: Vec<ItemView>,
}

#[derive(Serialize)]
pub struct ItemView {
    pub item_id: i64,
    pub definition_id: String,
    pub quantity: i64,
    pub quality: String,
    pub level: u32,
    pub equipped_slot: Option<String>,
}

/// 最近战斗视图。
#[derive(Serialize)]
pub struct BattlesView {
    pub battles: Vec<BattleView>,
}

#[derive(Serialize)]
pub struct BattleView {
    pub combat_id: i64,
    pub started_at: i64,
    pub team: u8,
    pub winner_team: u8,
    pub end_reason: String,
    pub rule_version: u32,
    pub power: i64,
}

/// 组装档案视图；`date` 只用于战力修正展示，不触发任何写入。
pub fn profile(
    transaction: &rusqlite::Transaction<'_>,
    platform_user_id: PlatformUserId,
    date: &str,
) -> DatabaseResult<ProfileView> {
    let user_id = platform_user_id.value();
    let player = player::get_active(transaction, user_id)?.ok_or(DatabaseError::NotFound)?;
    let cultivation = crate::database::cultivation::get(transaction, user_id)?;
    let system = engine::find_system(&cultivation.system_id);
    let realm_name = system
        .as_ref()
        .and_then(|system| system.realms().get(cultivation.realm_index as usize))
        .map(|realm| realm.name.to_owned())
        .unwrap_or_else(|| "未知境界".into());
    let daily = daily_state::existing(transaction, user_id, date)?;
    let combat_profile = engine::build_combat_profile_with_state(
        &player,
        &cultivation.system_id,
        cultivation.realm_index,
        date,
        daily.as_ref(),
    );
    let (display_name, character_id, biography, registered_at) = transaction
        .query_row(
            "SELECT profile.display_name, profile.character_id, profile.biography,
                    player.created_at
             FROM player_profiles profile JOIN players player USING(player_id)
             WHERE player.player_id=?1",
            [crate::database::player_id(user_id)?],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            },
        )
        .map_err(DatabaseError::from_sqlite)?;

    Ok(ProfileView {
        display_name,
        character_id,
        biography,
        system_id: cultivation.system_id.clone(),
        system_name: system
            .map(|system| system.name().to_owned())
            .unwrap_or_else(|| cultivation.system_id.clone()),
        realm_index: cultivation.realm_index,
        realm_name,
        power: combat_profile.power,
        daily_state: daily.map(|state| DailyStateView {
            id: state.id,
            name: state.name,
            description: state.description,
            rule_version: state.rule_version.value(),
        }),
        registered_at,
    })
}

/// 组装钱包视图。
pub fn wallet(
    transaction: &rusqlite::Transaction<'_>,
    platform_user_id: PlatformUserId,
) -> DatabaseResult<WalletView> {
    let id = crate::database::player_id(platform_user_id.value())?;
    let mut balances = transaction
        .prepare(
            "SELECT currency_code, amount FROM player_balances
             WHERE player_id=?1 ORDER BY currency_code",
        )
        .map_err(DatabaseError::from_sqlite)?
        .query_map([id], |row| {
            Ok(BalanceView {
                currency: row.get(0)?,
                amount: row.get(1)?,
            })
        })
        .map_err(DatabaseError::from_sqlite)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(DatabaseError::from_sqlite)?;
    let transactions = transaction
        .prepare(
            "SELECT reason_code, delta, balance_after, created_at FROM wallet_transactions
             WHERE player_id=?1 ORDER BY created_at DESC, transaction_id DESC LIMIT 12",
        )
        .map_err(DatabaseError::from_sqlite)?
        .query_map([id], |row| {
            Ok(TransactionView {
                reason: row.get(0)?,
                delta: row.get(1)?,
                balance_after: row.get(2)?,
                created_at: row.get(3)?,
            })
        })
        .map_err(DatabaseError::from_sqlite)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(DatabaseError::from_sqlite)?;
    if balances.is_empty() {
        balances.push(BalanceView {
            currency: "coins".into(),
            amount: 0,
        });
    }
    Ok(WalletView {
        balances,
        transactions,
    })
}

/// 组装技能视图；不触发解锁，只展示已掌握内容。
pub fn skills_view(
    transaction: &rusqlite::Transaction<'_>,
    platform_user_id: PlatformUserId,
) -> DatabaseResult<SkillsView> {
    let user_id = platform_user_id.value();
    let skill_list = skills::list(transaction, user_id)?;
    let tactic = skills::current_tactic(transaction, user_id)?;
    Ok(SkillsView {
        skills: skill_list
            .into_iter()
            .map(|skill| SkillView {
                id: skill.definition.id.to_string(),
                name: skill.definition.name,
                mastery: skill.mastery,
            })
            .collect(),
        tactic: tactic.code().to_owned(),
    })
}

/// 组装背包视图。
pub fn equipment(
    transaction: &rusqlite::Transaction<'_>,
    platform_user_id: PlatformUserId,
) -> DatabaseResult<EquipmentView> {
    let items = inventory::list(transaction, platform_user_id.value())?;
    Ok(EquipmentView {
        items: items
            .into_iter()
            .map(|item| ItemView {
                item_id: item.item_id,
                definition_id: item.definition_id,
                quantity: item.quantity,
                quality: item.quality,
                level: item.level,
                equipped_slot: item.equipped_slot,
            })
            .collect(),
    })
}

/// 组装最近战斗视图。
pub fn battles(
    transaction: &rusqlite::Transaction<'_>,
    platform_user_id: PlatformUserId,
) -> DatabaseResult<BattlesView> {
    let id = crate::database::player_id(platform_user_id.value())?;
    let battles = transaction
        .prepare(
            "SELECT record.combat_id, record.started_at, participant.team,
                    record.winner_team, record.end_reason, record.rule_version,
                    participant.power_before
             FROM combat_participants participant
             JOIN combat_records record USING(combat_id)
             WHERE participant.player_id=?1
             ORDER BY record.started_at DESC LIMIT 10",
        )
        .map_err(DatabaseError::from_sqlite)?
        .query_map([id], |row| {
            Ok(BattleView {
                combat_id: row.get(0)?,
                started_at: row.get(1)?,
                team: row.get(2)?,
                winner_team: row.get(3)?,
                end_reason: row.get(4)?,
                rule_version: row.get(5)?,
                power: row.get(6)?,
            })
        })
        .map_err(DatabaseError::from_sqlite)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(DatabaseError::from_sqlite)?;
    Ok(BattlesView { battles })
}
