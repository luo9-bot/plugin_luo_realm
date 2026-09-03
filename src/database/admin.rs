use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::Serialize;

use super::{DatabaseError, DatabaseResult, player_id, unix_timestamp, wallet};

const PAGE_LIMIT_MAX: usize = 100;

#[derive(Debug, Serialize)]
pub struct Overview {
    pub players: i64,
    pub pending_players: i64,
    pub active_players: i64,
    pub enabled_groups: i64,
    pub combats: i64,
    pub wallet_transactions: i64,
}

#[derive(Debug, Serialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub page: usize,
    pub limit: usize,
}

#[derive(Debug, Serialize)]
pub struct GroupRow {
    pub group_id: i64,
    pub enabled: bool,
    pub general: bool,
    pub event: bool,
    pub combat: bool,
    pub battle_report_mode: String,
    pub updated_at: i64,
}

#[derive(Debug, Serialize)]
pub struct PlayerRow {
    pub player_id: i64,
    pub display_name: String,
    pub character_id: String,
    pub status: String,
    pub registration_state: String,
    pub system_id: Option<String>,
    pub realm_index: Option<i64>,
    pub progress: Option<i64>,
    pub coins: i64,
    pub marks: i64,
    pub updated_at: i64,
}

#[derive(Debug, Serialize)]
pub struct ItemRow {
    pub item_instance_id: i64,
    pub slot_index: i64,
    pub definition_id: String,
    pub quantity: i64,
    pub quality: String,
    pub level: i64,
}

#[derive(Debug, Serialize)]
pub struct StatisticRow {
    pub metric_code: String,
    pub metric_value: i64,
}

/// 角色当前的基础战斗属性（由境界索引推导）。
#[derive(Debug, Serialize)]
pub struct PlayerDetail {
    pub player: PlayerRow,
    /// 已入道的玩家才有境界与属性；待选体系玩家为空。
    pub attributes: Option<crate::database::player::RealmAttributes>,
    pub items: Vec<ItemRow>,
    pub statistics: Vec<StatisticRow>,
}

#[derive(Debug, Serialize)]
pub struct AuditRow {
    pub audit_id: i64,
    pub operator: String,
    pub action_code: String,
    pub target_type: String,
    pub target_id: String,
    pub reason: String,
    pub before_json: Option<String>,
    pub after_json: Option<String>,
    pub result: String,
    pub created_at: i64,
}

pub struct AuditEntry<'a> {
    pub operator: &'a str,
    pub action: &'a str,
    pub target_type: &'a str,
    pub target_id: &'a str,
    pub reason: &'a str,
    pub before: Option<serde_json::Value>,
    pub after: Option<serde_json::Value>,
}

pub fn overview(connection: &Connection) -> DatabaseResult<Overview> {
    connection
        .query_row(
            "SELECT
                (SELECT COUNT(*) FROM players),
                (SELECT COUNT(*) FROM players WHERE registration_state='pending_system'),
                (SELECT COUNT(*) FROM players WHERE registration_state='active'),
                (SELECT COUNT(*) FROM groups WHERE enabled=1),
                (SELECT COUNT(*) FROM combat_records),
                (SELECT COUNT(*) FROM wallet_transactions)",
            [],
            |row| {
                Ok(Overview {
                    players: row.get(0)?,
                    pending_players: row.get(1)?,
                    active_players: row.get(2)?,
                    enabled_groups: row.get(3)?,
                    combats: row.get(4)?,
                    wallet_transactions: row.get(5)?,
                })
            },
        )
        .map_err(DatabaseError::from_sqlite)
}

pub fn list_groups(
    connection: &Connection,
    search: &str,
    page: usize,
    limit: usize,
) -> DatabaseResult<Page<GroupRow>> {
    let (page, limit, offset) = pagination(page, limit);
    let pattern = format!("%{}%", search.trim());
    let total = connection
        .query_row(
            "SELECT COUNT(*) FROM groups WHERE CAST(group_id AS TEXT) LIKE ?1",
            [&pattern],
            |row| row.get(0),
        )
        .map_err(DatabaseError::from_sqlite)?;
    let mut statement = connection
        .prepare(
            "SELECT group_id, enabled,
                    COALESCE((SELECT enabled FROM group_features
                              WHERE group_id=g.group_id AND feature_code='general'), 1),
                    COALESCE((SELECT enabled FROM group_features
                              WHERE group_id=g.group_id AND feature_code='event'), 1),
                    COALESCE((SELECT enabled FROM group_features
                              WHERE group_id=g.group_id AND feature_code='combat'), 1),
                    battle_report_mode, updated_at
             FROM groups g WHERE CAST(group_id AS TEXT) LIKE ?1
             ORDER BY updated_at DESC LIMIT ?2 OFFSET ?3",
        )
        .map_err(DatabaseError::from_sqlite)?;
    let rows = statement
        .query_map(params![pattern, limit as i64, offset], |row| {
            Ok(GroupRow {
                group_id: row.get(0)?,
                enabled: row.get(1)?,
                general: row.get(2)?,
                event: row.get(3)?,
                combat: row.get(4)?,
                battle_report_mode: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })
        .map_err(DatabaseError::from_sqlite)?;

    Ok(Page {
        items: collect_rows(rows)?,
        total,
        page,
        limit,
    })
}

pub fn list_players(
    connection: &Connection,
    search: &str,
    page: usize,
    limit: usize,
) -> DatabaseResult<Page<PlayerRow>> {
    let (page, limit, offset) = pagination(page, limit);
    let pattern = format!("%{}%", search.trim());
    let total = connection
        .query_row(
            "SELECT COUNT(*) FROM players p JOIN player_profiles profile USING(player_id)
             WHERE CAST(p.player_id AS TEXT) LIKE ?1 OR profile.display_name LIKE ?1",
            [&pattern],
            |row| row.get(0),
        )
        .map_err(DatabaseError::from_sqlite)?;
    let mut statement = connection
        .prepare(
            "SELECT p.player_id, profile.display_name, p.status, p.registration_state,
                    cultivation.system_id, cultivation.realm_index, cultivation.progress,
                    COALESCE(coins.amount, 0), COALESCE(marks.amount, 0), p.updated_at,
                    profile.character_id
             FROM players p
             JOIN player_profiles profile USING(player_id)
             LEFT JOIN player_cultivation cultivation USING(player_id)
             LEFT JOIN player_balances coins
                    ON coins.player_id=p.player_id AND coins.currency_code='coins'
             LEFT JOIN player_balances marks
                    ON marks.player_id=p.player_id AND marks.currency_code='marks'
             WHERE CAST(p.player_id AS TEXT) LIKE ?1 OR profile.display_name LIKE ?1
             ORDER BY p.updated_at DESC LIMIT ?2 OFFSET ?3",
        )
        .map_err(DatabaseError::from_sqlite)?;
    let rows = statement
        .query_map(params![pattern, limit as i64, offset], player_from_row)
        .map_err(DatabaseError::from_sqlite)?;

    Ok(Page {
        items: collect_rows(rows)?,
        total,
        page,
        limit,
    })
}

pub fn player_detail(connection: &Connection, user_id: u64) -> DatabaseResult<PlayerDetail> {
    let id = player_id(user_id)?;
    let player = connection
        .query_row(
            "SELECT p.player_id, profile.display_name, p.status, p.registration_state,
                    cultivation.system_id, cultivation.realm_index, cultivation.progress,
                    COALESCE(coins.amount, 0), COALESCE(marks.amount, 0), p.updated_at,
                    profile.character_id
             FROM players p
             JOIN player_profiles profile USING(player_id)
             LEFT JOIN player_cultivation cultivation USING(player_id)
             LEFT JOIN player_balances coins
                    ON coins.player_id=p.player_id AND coins.currency_code='coins'
             LEFT JOIN player_balances marks
                    ON marks.player_id=p.player_id AND marks.currency_code='marks'
             WHERE p.player_id=?1",
            [id],
            player_from_row,
        )
        .optional()
        .map_err(DatabaseError::from_sqlite)?
        .ok_or(DatabaseError::NotFound)?;

    let items = query_items(connection, id)?;
    let statistics = query_statistics(connection, id)?;
    let attributes = player
        .realm_index
        .and_then(|realm_index| u32::try_from(realm_index).ok())
        .map(crate::database::player::realm_attributes);
    Ok(PlayerDetail {
        player,
        attributes,
        items,
        statistics,
    })
}

pub fn set_group(
    transaction: &Transaction<'_>,
    operator: &str,
    group_id: u64,
    enabled: bool,
    reason: &str,
) -> DatabaseResult<()> {
    let before = serde_json::json!({"enabled": super::group::is_enabled(transaction, group_id)?});
    super::group::set_enabled(transaction, group_id, enabled)?;
    audit_success(
        transaction,
        AuditEntry {
            operator,
            action: "group.update",
            target_type: "group",
            target_id: &group_id.to_string(),
            reason,
            before: Some(before),
            after: Some(serde_json::json!({"enabled": enabled})),
        },
    )
}

pub fn set_group_features(
    transaction: &Transaction<'_>,
    operator: &str,
    group_id: u64,
    features: &[(&str, bool)],
    battle_report_mode: super::group::BattleReportMode,
    reason: &str,
) -> DatabaseResult<()> {
    if features
        .iter()
        .any(|(feature, _)| !matches!(*feature, "general" | "event" | "combat"))
    {
        return Err(DatabaseError::InvalidData("unknown group feature".into()));
    }
    let exists: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM groups WHERE group_id=?1)",
            [player_id(group_id)?],
            |row| row.get(0),
        )
        .map_err(DatabaseError::from_sqlite)?;
    if !exists {
        return Err(DatabaseError::NotFound);
    }
    features.iter().try_for_each(|(feature, enabled)| {
        super::group::set_feature(transaction, group_id, feature, *enabled)
    })?;
    super::group::set_battle_report_mode(transaction, group_id, battle_report_mode)?;
    audit_success(
        transaction,
        AuditEntry {
            operator,
            action: "group.features.update",
            target_type: "group",
            target_id: &group_id.to_string(),
            reason,
            before: None,
            after: Some(serde_json::json!({
                "features": features,
                "battle_report_mode": battle_report_mode.code()
            })),
        },
    )
}

pub fn update_profile(
    transaction: &Transaction<'_>,
    operator: &str,
    user_id: u64,
    display_name: &str,
    status: &str,
    reason: &str,
) -> DatabaseResult<()> {
    let id = player_id(user_id)?;
    let before: (String, String) = transaction
        .query_row(
            "SELECT profile.display_name, p.status FROM players p
             JOIN player_profiles profile USING(player_id) WHERE p.player_id=?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(DatabaseError::from_sqlite)?
        .ok_or(DatabaseError::NotFound)?;
    transaction
        .execute(
            "UPDATE player_profiles SET display_name=?2 WHERE player_id=?1",
            params![id, display_name],
        )
        .map_err(DatabaseError::from_sqlite)?;
    transaction
        .execute(
            "UPDATE players SET status=?2, revision=revision+1, updated_at=?3
             WHERE player_id=?1",
            params![id, status, unix_timestamp()],
        )
        .map_err(DatabaseError::from_sqlite)?;
    audit_success(
        transaction,
        AuditEntry {
            operator,
            action: "player.profile.update",
            target_type: "player",
            target_id: &user_id.to_string(),
            reason,
            before: Some(serde_json::json!({
                "display_name": before.0, "status": before.1
            })),
            after: Some(serde_json::json!({
                "display_name": display_name, "status": status
            })),
        },
    )
}

/// 设置玩家的角色形象 ID；空串表示跟随用户号的随机形象。
pub fn update_character(
    transaction: &Transaction<'_>,
    operator: &str,
    user_id: u64,
    character_id: &str,
    reason: &str,
) -> DatabaseResult<()> {
    let id = player_id(user_id)?;
    let normalized = character_id.trim();
    if !normalized.is_empty() && !crate::render::assets::portrait_id_is_safe(normalized) {
        return Err(DatabaseError::InvalidData(
            "character id contains forbidden characters".into(),
        ));
    }
    let before: String = transaction
        .query_row(
            "SELECT character_id FROM player_profiles WHERE player_id=?1",
            [id],
            |row| row.get(0),
        )
        .optional()
        .map_err(DatabaseError::from_sqlite)?
        .ok_or(DatabaseError::NotFound)?;
    transaction
        .execute(
            "UPDATE player_profiles SET character_id=?2 WHERE player_id=?1",
            params![id, normalized],
        )
        .map_err(DatabaseError::from_sqlite)?;
    transaction
        .execute(
            "UPDATE players SET revision=revision+1, updated_at=?2 WHERE player_id=?1",
            params![id, unix_timestamp()],
        )
        .map_err(DatabaseError::from_sqlite)?;
    audit_success(
        transaction,
        AuditEntry {
            operator,
            action: "player.character.update",
            target_type: "player",
            target_id: &user_id.to_string(),
            reason,
            before: Some(serde_json::json!({ "character_id": before })),
            after: Some(serde_json::json!({ "character_id": normalized })),
        },
    )
}

pub fn delete_player(
    transaction: &Transaction<'_>,
    operator: &str,
    user_id: u64,
    reason: &str,
) -> DatabaseResult<()> {
    let id = player_id(user_id)?;
    let before = serde_json::to_value(player_detail(transaction, user_id)?)
        .map_err(|error| DatabaseError::InvalidData(error.to_string()))?;

    transaction
        .execute(
            "DELETE FROM combat_records
             WHERE combat_id IN (
                 SELECT combat_id FROM combat_participants WHERE player_id=?1
             )",
            [id],
        )
        .map_err(DatabaseError::from_sqlite)?;
    [
        "DELETE FROM daily_checkins WHERE player_id=?1",
        "DELETE FROM wallet_transactions WHERE player_id=?1",
        "DELETE FROM breakthrough_history WHERE player_id=?1",
        "DELETE FROM destiny_events WHERE player_id=?1",
        "DELETE FROM players WHERE player_id=?1",
    ]
    .into_iter()
    .try_for_each(|statement| {
        transaction
            .execute(statement, [id])
            .map(|_| ())
            .map_err(DatabaseError::from_sqlite)
    })?;

    audit_success(
        transaction,
        AuditEntry {
            operator,
            action: "player.delete",
            target_type: "player",
            target_id: &user_id.to_string(),
            reason,
            before: Some(before),
            after: None,
        },
    )
}

pub fn adjust_wallet(
    transaction: &Transaction<'_>,
    operator: &str,
    user_id: u64,
    currency: &str,
    delta: i64,
    reason: &str,
    idempotency_key: &str,
) -> DatabaseResult<i64> {
    if delta == 0 || delta == i64::MIN || !matches!(currency, "coins" | "marks") {
        return Err(DatabaseError::InvalidData(
            "invalid wallet adjustment".into(),
        ));
    }
    ensure_active_player(transaction, user_id)?;
    let before = wallet::balance(transaction, user_id, currency)?;
    let entry = if delta > 0 {
        wallet::credit(
            transaction,
            user_id,
            currency,
            delta,
            "admin_adjustment",
            idempotency_key,
        )?
    } else {
        wallet::debit(
            transaction,
            user_id,
            currency,
            delta.saturating_abs(),
            "admin_adjustment",
            idempotency_key,
        )?
    };
    audit_success(
        transaction,
        AuditEntry {
            operator,
            action: "wallet.adjust",
            target_type: "player",
            target_id: &user_id.to_string(),
            reason,
            before: Some(serde_json::json!({"currency": currency, "balance": before})),
            after: Some(serde_json::json!({
                "currency": currency, "balance": entry.balance_after
            })),
        },
    )?;
    Ok(entry.balance_after)
}

#[allow(clippy::too_many_arguments)]
pub fn update_cultivation(
    transaction: &Transaction<'_>,
    operator: &str,
    user_id: u64,
    system_id: &str,
    realm_index: i64,
    progress: i64,
    reason: &str,
) -> DatabaseResult<()> {
    let Some(system) = crate::engine::find_system(system_id) else {
        return Err(DatabaseError::InvalidData(
            "unknown cultivation system".into(),
        ));
    };
    if realm_index < 0 || realm_index as usize >= system.realms().len() || progress < 0 {
        return Err(DatabaseError::InvalidData(
            "invalid cultivation state".into(),
        ));
    }
    let id = player_id(user_id)?;
    let before: Option<(String, i64, i64)> = transaction
        .query_row(
            "SELECT system_id, realm_index, progress FROM player_cultivation WHERE player_id=?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(DatabaseError::from_sqlite)?;
    let registration_state: String = transaction
        .query_row(
            "SELECT registration_state FROM players WHERE player_id=?1",
            [id],
            |row| row.get(0),
        )
        .optional()
        .map_err(DatabaseError::from_sqlite)?
        .ok_or(DatabaseError::NotFound)?;
    transaction
        .execute(
            "INSERT INTO player_cultivation(
                 player_id, system_id, realm_index, progress, updated_at
             ) VALUES(?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(player_id) DO UPDATE SET
                 system_id=excluded.system_id,
                 realm_index=excluded.realm_index,
                 progress=excluded.progress,
                 updated_at=excluded.updated_at",
            params![id, system_id, realm_index, progress, unix_timestamp()],
        )
        .map_err(DatabaseError::from_sqlite)?;
    transaction
        .execute(
            "UPDATE players SET registration_state='active', revision=revision+1, updated_at=?2
             WHERE player_id=?1",
            params![id, unix_timestamp()],
        )
        .map_err(DatabaseError::from_sqlite)?;
    audit_success(
        transaction,
        AuditEntry {
            operator,
            action: "cultivation.update",
            target_type: "player",
            target_id: &user_id.to_string(),
            reason,
            before: Some(serde_json::json!({
                "cultivation": before, "registration_state": registration_state
            })),
            after: Some(serde_json::json!({
                "system_id": system_id, "realm_index": realm_index, "progress": progress,
                "registration_state": "active"
            })),
        },
    )
}

#[allow(clippy::too_many_arguments)]
pub fn grant_item(
    transaction: &Transaction<'_>,
    operator: &str,
    user_id: u64,
    definition_id: &str,
    quantity: i64,
    quality: &str,
    reason: &str,
) -> DatabaseResult<i64> {
    ensure_active_player(transaction, user_id)?;
    let id = player_id(user_id)?;
    let slot: i64 = transaction
        .query_row(
            "SELECT COALESCE(MAX(slot_index), -1) + 1 FROM inventory_slots WHERE player_id=?1",
            [id],
            |row| row.get(0),
        )
        .map_err(DatabaseError::from_sqlite)?;
    transaction
        .execute(
            "INSERT INTO item_instances(
                player_id, definition_id, quantity, quality, created_at
             ) VALUES(?1, ?2, ?3, ?4, ?5)",
            params![id, definition_id, quantity, quality, unix_timestamp()],
        )
        .map_err(DatabaseError::from_sqlite)?;
    let item_id = transaction.last_insert_rowid();
    transaction
        .execute(
            "INSERT INTO inventory_slots(player_id, slot_index, item_instance_id)
             VALUES(?1, ?2, ?3)",
            params![id, slot, item_id],
        )
        .map_err(DatabaseError::from_sqlite)?;
    audit_success(
        transaction,
        AuditEntry {
            operator,
            action: "item.grant",
            target_type: "item",
            target_id: &item_id.to_string(),
            reason,
            before: None,
            after: Some(serde_json::json!({
                "player_id": user_id, "definition_id": definition_id,
                "quantity": quantity, "quality": quality
            })),
        },
    )?;
    Ok(item_id)
}

pub fn remove_item(
    transaction: &Transaction<'_>,
    operator: &str,
    user_id: u64,
    item_id: i64,
    reason: &str,
) -> DatabaseResult<()> {
    ensure_active_player(transaction, user_id)?;
    let before: (String, i64) = transaction
        .query_row(
            "SELECT definition_id, quantity FROM item_instances
             WHERE item_instance_id=?1 AND player_id=?2",
            params![item_id, player_id(user_id)?],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(DatabaseError::from_sqlite)?
        .ok_or(DatabaseError::NotFound)?;
    transaction
        .execute(
            "DELETE FROM item_instances WHERE item_instance_id=?1",
            [item_id],
        )
        .map_err(DatabaseError::from_sqlite)?;
    audit_success(
        transaction,
        AuditEntry {
            operator,
            action: "item.remove",
            target_type: "item",
            target_id: &item_id.to_string(),
            reason,
            before: Some(serde_json::json!({
                "definition_id": before.0, "quantity": before.1
            })),
            after: None,
        },
    )
}

/// 调整单件物品的品阶，返回调整前的品质供审计展示。
pub fn set_item_quality(
    transaction: &Transaction<'_>,
    operator: &str,
    user_id: u64,
    item_id: i64,
    quality: &str,
    reason: &str,
) -> DatabaseResult<String> {
    ensure_active_player(transaction, user_id)?;
    let id = player_id(user_id)?;
    let before: String = transaction
        .query_row(
            "SELECT quality FROM item_instances
             WHERE item_instance_id=?1 AND player_id=?2",
            params![item_id, id],
            |row| row.get(0),
        )
        .optional()
        .map_err(DatabaseError::from_sqlite)?
        .ok_or(DatabaseError::NotFound)?;
    transaction
        .execute(
            "UPDATE item_instances SET quality=?3
             WHERE item_instance_id=?1 AND player_id=?2",
            params![item_id, id, quality],
        )
        .map_err(DatabaseError::from_sqlite)?;
    audit_success(
        transaction,
        AuditEntry {
            operator,
            action: "item.set_quality",
            target_type: "item",
            target_id: &item_id.to_string(),
            reason,
            before: Some(serde_json::json!({ "quality": before })),
            after: Some(serde_json::json!({ "quality": quality })),
        },
    )?;
    Ok(before)
}

/// 列出玩家要批量调整品阶的物品：`equipped` 只取已装备，`all` 取全部。
pub fn list_item_ids(
    transaction: &Transaction<'_>,
    user_id: u64,
    scope: &str,
) -> DatabaseResult<Vec<i64>> {
    let id = player_id(user_id)?;
    let sql = match scope {
        "equipped" => {
            "SELECT item.item_instance_id
             FROM item_instances item
             JOIN equipment_loadouts equipped USING(item_instance_id)
             WHERE item.player_id=?1 ORDER BY item.item_instance_id"
        }
        "all" => {
            "SELECT item_instance_id FROM item_instances
             WHERE player_id=?1 ORDER BY item_instance_id"
        }
        _ => return Err(DatabaseError::InvalidData("未知物品范围".into())),
    };
    let mut statement = transaction
        .prepare(sql)
        .map_err(DatabaseError::from_sqlite)?;
    statement
        .query_map([id], |row| row.get(0))
        .map_err(DatabaseError::from_sqlite)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(DatabaseError::from_sqlite)
}

pub fn update_statistic(
    transaction: &Transaction<'_>,
    operator: &str,
    user_id: u64,
    metric: &str,
    value: i64,
    reason: &str,
) -> DatabaseResult<()> {
    ensure_active_player(transaction, user_id)?;
    let id = player_id(user_id)?;
    let before: Option<i64> = transaction
        .query_row(
            "SELECT metric_value FROM player_statistics WHERE player_id=?1 AND metric_code=?2",
            params![id, metric],
            |row| row.get(0),
        )
        .optional()
        .map_err(DatabaseError::from_sqlite)?;
    transaction
        .execute(
            "INSERT INTO player_statistics(player_id, metric_code, metric_value, updated_at)
             VALUES(?1, ?2, ?3, ?4) ON CONFLICT(player_id, metric_code) DO UPDATE SET
             metric_value=excluded.metric_value, updated_at=excluded.updated_at",
            params![id, metric, value, unix_timestamp()],
        )
        .map_err(DatabaseError::from_sqlite)?;
    audit_success(
        transaction,
        AuditEntry {
            operator,
            action: "statistic.update",
            target_type: "player",
            target_id: &user_id.to_string(),
            reason,
            before: Some(serde_json::json!({"metric": metric, "value": before})),
            after: Some(serde_json::json!({"metric": metric, "value": value})),
        },
    )
}

pub fn list_audit(
    connection: &Connection,
    page: usize,
    limit: usize,
) -> DatabaseResult<Page<AuditRow>> {
    let (page, limit, offset) = pagination(page, limit);
    let total = connection
        .query_row("SELECT COUNT(*) FROM admin_audit_log", [], |row| row.get(0))
        .map_err(DatabaseError::from_sqlite)?;
    let mut statement = connection
        .prepare(
            "SELECT audit_id, operator, action_code, target_type, target_id, reason,
                    before_json, after_json, result, created_at
             FROM admin_audit_log ORDER BY audit_id DESC LIMIT ?1 OFFSET ?2",
        )
        .map_err(DatabaseError::from_sqlite)?;
    let rows = statement
        .query_map(params![limit as i64, offset], |row| {
            Ok(AuditRow {
                audit_id: row.get(0)?,
                operator: row.get(1)?,
                action_code: row.get(2)?,
                target_type: row.get(3)?,
                target_id: row.get(4)?,
                reason: row.get(5)?,
                before_json: row.get(6)?,
                after_json: row.get(7)?,
                result: row.get(8)?,
                created_at: row.get(9)?,
            })
        })
        .map_err(DatabaseError::from_sqlite)?;

    Ok(Page {
        items: collect_rows(rows)?,
        total,
        page,
        limit,
    })
}

pub fn audit_success(transaction: &Transaction<'_>, entry: AuditEntry<'_>) -> DatabaseResult<()> {
    transaction
        .execute(
            "INSERT INTO admin_audit_log(
                operator, action_code, target_type, target_id, reason,
                before_json, after_json, result, created_at
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, 'success', ?8)",
            params![
                entry.operator,
                entry.action,
                entry.target_type,
                entry.target_id,
                entry.reason,
                entry.before.map(|value| value.to_string()),
                entry.after.map(|value| value.to_string()),
                unix_timestamp()
            ],
        )
        .map_err(DatabaseError::from_sqlite)?;
    Ok(())
}

fn pagination(page: usize, limit: usize) -> (usize, usize, i64) {
    let page = page.max(1);
    let limit = limit.clamp(1, PAGE_LIMIT_MAX);
    let offset = page.saturating_sub(1).saturating_mul(limit) as i64;
    (page, limit, offset)
}

fn ensure_active_player(transaction: &Transaction<'_>, user_id: u64) -> DatabaseResult<()> {
    let active: bool = transaction
        .query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM players
                 WHERE player_id=?1 AND status='active' AND registration_state='active'
             )",
            [player_id(user_id)?],
            |row| row.get(0),
        )
        .map_err(DatabaseError::from_sqlite)?;
    if !active {
        return Err(DatabaseError::InvalidData(
            "player has not completed registration".into(),
        ));
    }
    Ok(())
}

fn player_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PlayerRow> {
    Ok(PlayerRow {
        player_id: row.get(0)?,
        display_name: row.get(1)?,
        character_id: row.get(10)?,
        status: row.get(2)?,
        registration_state: row.get(3)?,
        system_id: row.get(4)?,
        realm_index: row.get(5)?,
        progress: row.get(6)?,
        coins: row.get(7)?,
        marks: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

fn query_items(connection: &Connection, id: i64) -> DatabaseResult<Vec<ItemRow>> {
    let mut statement = connection
        .prepare(
            "SELECT item.item_instance_id, slot.slot_index, item.definition_id,
                    item.quantity, item.quality, item.level
             FROM item_instances item JOIN inventory_slots slot USING(item_instance_id)
             WHERE item.player_id=?1 ORDER BY slot.slot_index",
        )
        .map_err(DatabaseError::from_sqlite)?;
    let rows = statement
        .query_map([id], |row| {
            Ok(ItemRow {
                item_instance_id: row.get(0)?,
                slot_index: row.get(1)?,
                definition_id: row.get(2)?,
                quantity: row.get(3)?,
                quality: row.get(4)?,
                level: row.get(5)?,
            })
        })
        .map_err(DatabaseError::from_sqlite)?;
    collect_rows(rows)
}

fn query_statistics(connection: &Connection, id: i64) -> DatabaseResult<Vec<StatisticRow>> {
    let mut statement = connection
        .prepare(
            "SELECT metric_code, metric_value FROM player_statistics
             WHERE player_id=?1 ORDER BY metric_code",
        )
        .map_err(DatabaseError::from_sqlite)?;
    let rows = statement
        .query_map([id], |row| {
            Ok(StatisticRow {
                metric_code: row.get(0)?,
                metric_value: row.get(1)?,
            })
        })
        .map_err(DatabaseError::from_sqlite)?;
    collect_rows(rows)
}

fn collect_rows<T>(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>>,
) -> DatabaseResult<Vec<T>> {
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(DatabaseError::from_sqlite)
}
