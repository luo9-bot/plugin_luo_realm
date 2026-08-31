use std::{fs, sync::Arc};

use serde::Deserialize;
use tiny_http::Method;

use crate::{
    config::{AdminConfig, CommandConfig, GameConfig, GameplayConfig},
    cultivation,
    database::{Database, DatabaseError, admin},
    engine,
};

use super::{
    assets,
    router::{AdminState, HttpResponse, binary, download, error, ok},
    transfer,
};

pub fn dispatch(method: &Method, url: &str, body: &[u8], state: &Arc<AdminState>) -> HttpResponse {
    let path = url.split('?').next().unwrap_or(url);
    let segments = path
        .trim_matches('/')
        .split('/')
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    if segments.first() != Some(&"api") {
        return error(404, "not_found", "接口不存在");
    }
    let query = Query::parse(url);

    match (method, segments.as_slice()) {
        (&Method::Get, ["api", "overview"]) => read_database(state, |database| {
            admin::overview(database.connection()).and_then(to_json)
        }),
        (&Method::Get, ["api", "groups"]) => read_database(state, |database| {
            admin::list_groups(
                database.connection(),
                query.get("search").unwrap_or(""),
                query.number("page", 1),
                query.number("limit", 25),
            )
            .and_then(to_json)
        }),
        (&Method::Post, ["api", "groups"]) => set_group(body, state, None),
        (&Method::Put, ["api", "groups", group_id]) => match parse_id(group_id) {
            Some(group_id) => set_group(body, state, Some(group_id)),
            None => invalid("群号无效"),
        },
        (&Method::Get, ["api", "groups", group_id, "features"]) => group_features(state, group_id),
        (&Method::Put, ["api", "groups", group_id, "features"]) => {
            set_group_features(body, state, group_id)
        }
        (&Method::Get, ["api", "players"]) => read_database(state, |database| {
            admin::list_players(
                database.connection(),
                query.get("search").unwrap_or(""),
                query.number("page", 1),
                query.number("limit", 25),
            )
            .and_then(to_json)
        }),
        (&Method::Get, ["api", "players", player_id]) => {
            let Some(player_id) = parse_id(player_id) else {
                return invalid("玩家 ID 无效");
            };
            read_database(state, |database| {
                admin::player_detail(database.connection(), player_id).and_then(to_json)
            })
        }
        (&Method::Delete, ["api", "players", player_id]) => delete_player(body, state, player_id),
        (&Method::Put, ["api", "players", player_id, "profile"]) => {
            update_profile(body, state, player_id)
        }
        (&Method::Post, ["api", "players", player_id, "wallet"]) => {
            adjust_wallet(body, state, player_id)
        }
        (&Method::Put, ["api", "players", player_id, "cultivation"]) => {
            update_cultivation(body, state, player_id)
        }
        (&Method::Post, ["api", "players", player_id, "items"]) => {
            grant_item(body, state, player_id)
        }
        (&Method::Delete, ["api", "players", player_id, "items", item_id]) => {
            remove_item(body, state, player_id, item_id)
        }
        (&Method::Put, ["api", "players", player_id, "statistics"]) => {
            update_statistic(body, state, player_id)
        }
        (&Method::Get, ["api", "audit"]) => read_database(state, |database| {
            admin::list_audit(
                database.connection(),
                query.number("page", 1),
                query.number("limit", 50),
            )
            .and_then(to_json)
        }),
        (&Method::Get, ["api", "config"]) => json_value(state.policy.snapshot()),
        (&Method::Put, ["api", "config"]) => update_config(body, state),
        (&Method::Get, ["api", "definitions", "cultivation"]) => cultivation_definitions(),
        (&Method::Post, ["api", "backup"]) => create_backup(body, state),
        (&Method::Get, ["api", "assets"]) => list_assets(state, &query),
        (&Method::Get, ["api", "assets", "file"]) => read_asset(state, &query),
        (&Method::Post, ["api", "assets", "file"]) => write_asset(body, state, &query),
        (&Method::Delete, ["api", "assets", "file"]) => remove_asset(body, state, &query),
        (&Method::Get, ["api", "assets", "export"]) => export_assets(state, &query),
        (&Method::Post, ["api", "assets", "import"]) => import_assets(body, state, &query),
        (&Method::Get, ["api", "data", "export"]) => export_data(state, &query),
        (&Method::Post, ["api", "data", "import"]) => import_data(body, state, &query),
        (&Method::Post, ["api", "token", "rotate"]) => rotate_token(body, state),
        _ => error(404, "not_found", "接口不存在"),
    }
}

fn read_database(
    state: &AdminState,
    operation: impl FnOnce(&Database) -> Result<serde_json::Value, DatabaseError>,
) -> HttpResponse {
    match Database::open_request(&state.database_path).and_then(|database| operation(&database)) {
        Ok(value) => ok(value),
        Err(error_value) => database_error(error_value),
    }
}

fn write_database(
    state: &AdminState,
    operation: impl FnOnce(&rusqlite::Transaction<'_>) -> Result<serde_json::Value, DatabaseError>,
) -> HttpResponse {
    let result = (|| {
        let mut database = Database::open_request(&state.database_path)?;
        let transaction = database.immediate_transaction()?;
        let value = operation(&transaction)?;
        transaction.commit().map_err(DatabaseError::from_sqlite)?;
        Ok(value)
    })();
    match result {
        Ok(value) => ok(value),
        Err(error_value) => database_error(error_value),
    }
}

#[derive(Deserialize)]
struct GroupRequest {
    group_id: Option<u64>,
    enabled: bool,
    reason: String,
    confirm: Option<String>,
}

fn set_group(body: &[u8], state: &AdminState, path_id: Option<u64>) -> HttpResponse {
    let request: GroupRequest = match parse(body) {
        Ok(request) => request,
        Err(response) => return response,
    };
    let Some(group_id) = path_id.or(request.group_id) else {
        return invalid("群号无效");
    };
    if !valid_reason(&request.reason) {
        return invalid("修改原因需为 2 至 200 个字符");
    }
    if !request.enabled && !confirmed(&request.confirm, &format!("group:{group_id}:disable")) {
        return confirmation_required(format!("group:{group_id}:disable"));
    }
    write_database(state, |transaction| {
        admin::set_group(
            transaction,
            "web",
            group_id,
            request.enabled,
            request.reason.trim(),
        )?;
        Ok(serde_json::json!({"group_id": group_id, "enabled": request.enabled}))
    })
}

#[derive(Deserialize)]
struct FeatureRequest {
    general: bool,
    event: bool,
    combat: bool,
    battle_report_mode: crate::database::group::BattleReportMode,
    reason: String,
}

fn group_features(state: &AdminState, group_id: &str) -> HttpResponse {
    let Some(group_id) = parse_id(group_id) else {
        return invalid("群号无效");
    };
    read_database(state, |database| {
        let connection = database.connection();
        Ok(serde_json::json!({
            "general": crate::database::group::feature_enabled(connection, group_id, "general")?,
            "event": crate::database::group::feature_enabled(connection, group_id, "event")?,
            "combat": crate::database::group::feature_enabled(connection, group_id, "combat")?,
            "battle_report_mode": crate::database::group::battle_report_mode(
                connection,
                group_id,
            )?.code()
        }))
    })
}

fn set_group_features(body: &[u8], state: &AdminState, group_id: &str) -> HttpResponse {
    let Some(group_id) = parse_id(group_id) else {
        return invalid("群号无效");
    };
    let request: FeatureRequest = match parse(body) {
        Ok(request) => request,
        Err(response) => return response,
    };
    if !valid_reason(&request.reason) {
        return invalid("修改原因需为 2 至 200 个字符");
    }
    write_database(state, |transaction| {
        admin::set_group_features(
            transaction,
            "web",
            group_id,
            &[
                ("general", request.general),
                ("event", request.event),
                ("combat", request.combat),
            ],
            request.battle_report_mode,
            request.reason.trim(),
        )?;
        Ok(serde_json::json!({"group_id": group_id}))
    })
}

#[derive(Deserialize)]
struct ProfileRequest {
    display_name: String,
    status: String,
    reason: String,
    confirm: Option<String>,
}

fn update_profile(body: &[u8], state: &AdminState, player_id: &str) -> HttpResponse {
    let Some(player_id) = parse_id(player_id) else {
        return invalid("玩家 ID 无效");
    };
    let request: ProfileRequest = match parse(body) {
        Ok(request) => request,
        Err(response) => return response,
    };
    if !(1..=20).contains(&request.display_name.trim().chars().count())
        || request.display_name.chars().any(char::is_control)
        || !matches!(request.status.as_str(), "active" | "disabled" | "deleted")
        || !valid_reason(&request.reason)
    {
        return invalid("玩家资料或修改原因不合法");
    }
    let summary = format!("player:{player_id}:{}", request.status);
    if request.status != "active" && !confirmed(&request.confirm, &summary) {
        return confirmation_required(summary);
    }
    write_database(state, |transaction| {
        admin::update_profile(
            transaction,
            "web",
            player_id,
            request.display_name.trim(),
            &request.status,
            request.reason.trim(),
        )?;
        Ok(serde_json::json!({"player_id": player_id}))
    })
}

fn delete_player(body: &[u8], state: &AdminState, player_id: &str) -> HttpResponse {
    let Some(player_id) = parse_id(player_id) else {
        return invalid("玩家 ID 无效");
    };
    let request: ReasonRequest = match parse(body) {
        Ok(request) => request,
        Err(response) => return response,
    };
    let summary = format!("player:{player_id}:delete");
    if !valid_reason(&request.reason) || !confirmed(&request.confirm, &summary) {
        return confirmation_required(summary);
    }
    write_database(state, |transaction| {
        admin::delete_player(transaction, "web", player_id, request.reason.trim())?;
        Ok(serde_json::json!({"deleted": player_id}))
    })
}

#[derive(Deserialize)]
struct WalletRequest {
    currency: String,
    delta: i64,
    reason: String,
    confirm: Option<String>,
}

fn adjust_wallet(body: &[u8], state: &AdminState, player_id: &str) -> HttpResponse {
    let Some(player_id) = parse_id(player_id) else {
        return invalid("玩家 ID 无效");
    };
    let request: WalletRequest = match parse(body) {
        Ok(request) => request,
        Err(response) => return response,
    };
    if !matches!(request.currency.as_str(), "coins" | "marks")
        || request.delta == 0
        || request.delta == i64::MIN
        || !valid_reason(&request.reason)
    {
        return invalid("货币、数额或修改原因不合法");
    }
    let summary = format!("wallet:{player_id}:{}:{}", request.currency, request.delta);
    if (request.delta < 0 || request.delta.abs() >= 10_000)
        && !confirmed(&request.confirm, &summary)
    {
        return confirmation_required(summary);
    }
    let idempotency_key = match wallet_idempotency_key(player_id, &request.currency) {
        Ok(key) => key,
        Err(response) => return response,
    };
    write_database(state, |transaction| {
        let balance = admin::adjust_wallet(
            transaction,
            "web",
            player_id,
            &request.currency,
            request.delta,
            request.reason.trim(),
            &idempotency_key,
        )?;
        Ok(serde_json::json!({"player_id": player_id, "balance": balance}))
    })
}

#[derive(Deserialize)]
struct CultivationRequest {
    system_id: String,
    realm_index: i64,
    progress: i64,
    reason: String,
    confirm: Option<String>,
}

fn update_cultivation(body: &[u8], state: &AdminState, player_id: &str) -> HttpResponse {
    let Some(player_id) = parse_id(player_id) else {
        return invalid("玩家 ID 无效");
    };
    let request: CultivationRequest = match parse(body) {
        Ok(request) => request,
        Err(response) => return response,
    };
    let Some(system) = engine::find_system(&request.system_id) else {
        return invalid("修行体系不存在");
    };
    if request.realm_index < 0
        || request.realm_index as usize >= system.realms().len()
        || request.progress < 0
        || !valid_reason(&request.reason)
    {
        return invalid("境界、进度或修改原因不合法");
    }
    let summary = format!(
        "cultivation:{player_id}:{}:{}:{}",
        request.system_id, request.realm_index, request.progress
    );
    if !confirmed(&request.confirm, &summary) {
        return confirmation_required(summary);
    }
    write_database(state, |transaction| {
        admin::update_cultivation(
            transaction,
            "web",
            player_id,
            &request.system_id,
            request.realm_index,
            request.progress,
            request.reason.trim(),
        )?;
        Ok(serde_json::json!({"player_id": player_id}))
    })
}

#[derive(Deserialize)]
struct ItemRequest {
    definition_id: String,
    quantity: i64,
    quality: String,
    reason: String,
}

fn grant_item(body: &[u8], state: &AdminState, player_id: &str) -> HttpResponse {
    let Some(player_id) = parse_id(player_id) else {
        return invalid("玩家 ID 无效");
    };
    let request: ItemRequest = match parse(body) {
        Ok(request) => request,
        Err(response) => return response,
    };
    if !valid_code(&request.definition_id)
        || !(1..=999_999).contains(&request.quantity)
        || !valid_code(&request.quality)
        || !valid_reason(&request.reason)
    {
        return invalid("物品参数或修改原因不合法");
    }
    write_database(state, |transaction| {
        let item_id = admin::grant_item(
            transaction,
            "web",
            player_id,
            &request.definition_id,
            request.quantity,
            &request.quality,
            request.reason.trim(),
        )?;
        Ok(serde_json::json!({"item_instance_id": item_id}))
    })
}

#[derive(Deserialize)]
struct ReasonRequest {
    reason: String,
    confirm: Option<String>,
}

fn remove_item(body: &[u8], state: &AdminState, player_id: &str, item_id: &str) -> HttpResponse {
    let (Some(player_id), Ok(item_id)) = (parse_id(player_id), item_id.parse::<i64>()) else {
        return invalid("玩家或物品 ID 无效");
    };
    let request: ReasonRequest = match parse(body) {
        Ok(request) => request,
        Err(response) => return response,
    };
    let summary = format!("item:{player_id}:{item_id}:remove");
    if !valid_reason(&request.reason) || !confirmed(&request.confirm, &summary) {
        return confirmation_required(summary);
    }
    write_database(state, |transaction| {
        admin::remove_item(
            transaction,
            "web",
            player_id,
            item_id,
            request.reason.trim(),
        )?;
        Ok(serde_json::json!({"removed": item_id}))
    })
}

#[derive(Deserialize)]
struct StatisticRequest {
    metric: String,
    value: i64,
    reason: String,
    confirm: Option<String>,
}

fn update_statistic(body: &[u8], state: &AdminState, player_id: &str) -> HttpResponse {
    let Some(player_id) = parse_id(player_id) else {
        return invalid("玩家 ID 无效");
    };
    let request: StatisticRequest = match parse(body) {
        Ok(request) => request,
        Err(response) => return response,
    };
    if !valid_code(&request.metric) || request.value < 0 || !valid_reason(&request.reason) {
        return invalid("统计参数或修改原因不合法");
    }
    let summary = format!("statistic:{player_id}:{}:{}", request.metric, request.value);
    if !confirmed(&request.confirm, &summary) {
        return confirmation_required(summary);
    }
    write_database(state, |transaction| {
        admin::update_statistic(
            transaction,
            "web",
            player_id,
            &request.metric,
            request.value,
            request.reason.trim(),
        )?;
        Ok(serde_json::json!({"player_id": player_id}))
    })
}

#[derive(Deserialize)]
struct ConfigRequest {
    command: CommandConfig,
    gameplay: GameplayConfig,
    game: GameConfig,
    admin: AdminConfig,
    reason: String,
    confirm: Option<String>,
}

fn update_config(body: &[u8], state: &AdminState) -> HttpResponse {
    let request: ConfigRequest = match parse(body) {
        Ok(request) => request,
        Err(response) => return response,
    };
    if !valid_reason(&request.reason) || !confirmed(&request.confirm, "config:update") {
        return confirmation_required("config:update".into());
    }
    let previous = state.policy.snapshot();
    let mut database = match Database::open_request(&state.database_path) {
        Ok(database) => database,
        Err(error_value) => return database_error(error_value),
    };
    let mut config = state.policy.snapshot();
    config.command = request.command;
    config.gameplay = request.gameplay;
    config.game = request.game;
    config.admin = request.admin;
    if let Err(config_error) = config.validate() {
        return error(400, "invalid_config", &config_error.to_string());
    }
    if config.save(&state.plugin_root).is_err() {
        return error(500, "config_write_failed", "无法安全写入运行配置");
    }
    let audit_result = (|| {
        let transaction = database.immediate_transaction()?;
        admin::audit_success(
            &transaction,
            admin::AuditEntry {
                operator: "web",
                action: "config.update",
                target_type: "runtime",
                target_id: "config",
                reason: request.reason.trim(),
                before: serde_json::to_value(&previous).ok(),
                after: serde_json::to_value(&config).ok(),
            },
        )?;
        transaction.commit().map_err(DatabaseError::from_sqlite)
    })();
    if let Err(error_value) = audit_result {
        let _ = previous.save(&state.plugin_root);
        return database_error(error_value);
    }
    state.policy.replace(config.clone());
    match serde_json::to_value(config) {
        Ok(value) => ok(value),
        Err(_) => error(500, "serialization_failed", "无法序列化配置"),
    }
}

fn cultivation_definitions() -> HttpResponse {
    let definitions = cultivation::registered_systems()
        .into_iter()
        .map(|system| {
            serde_json::json!({
                "id": system.id(),
                "name": system.name(),
                "realms": system.realms(),
                "skills": system.skills(),
                "tags": system.tags()
            })
        })
        .collect::<Vec<_>>();
    ok(serde_json::json!(definitions))
}

fn create_backup(body: &[u8], state: &AdminState) -> HttpResponse {
    let request: ReasonRequest = match parse(body) {
        Ok(request) => request,
        Err(response) => return response,
    };
    if !valid_reason(&request.reason) {
        return invalid("备份原因需为 2 至 200 个字符");
    }
    let backup_directory = state
        .plugin_root
        .join(crate::identity::DATA_DIRECTORY)
        .join("backups");
    if fs::create_dir_all(&backup_directory).is_err() {
        return error(500, "backup_failed", "无法创建备份目录");
    }
    let name = format!("luo-realm-{}.sqlite3", crate::database::unix_timestamp());
    let destination = backup_directory.join(&name);
    let result = (|| {
        let mut database = Database::open_request(&state.database_path)?;
        database.backup_to(&destination)?;
        let transaction = database.immediate_transaction()?;
        admin::audit_success(
            &transaction,
            admin::AuditEntry {
                operator: "web",
                action: "database.backup",
                target_type: "database",
                target_id: &name,
                reason: request.reason.trim(),
                before: None,
                after: Some(serde_json::json!({"file": name})),
            },
        )?;
        transaction.commit().map_err(DatabaseError::from_sqlite)
    })();
    match result {
        Ok(()) => ok(serde_json::json!({"file": name})),
        Err(error_value) => database_error(error_value),
    }
}

fn list_assets(state: &AdminState, query: &Query) -> HttpResponse {
    match assets::list(
        &state.plugin_root,
        query.get("category").unwrap_or(""),
        query.get("search").unwrap_or(""),
        query.number("page", 1),
        query.number("limit", 60),
    ) {
        Ok(page) => json_value(page),
        Err(error_value) => asset_error(error_value),
    }
}

fn read_asset(state: &AdminState, query: &Query) -> HttpResponse {
    let Some(path) = query.get("path") else {
        return invalid("缺少素材路径");
    };
    match assets::read(&state.plugin_root, path) {
        Ok((bytes, content_type)) => binary(bytes, content_type),
        Err(error_value) => asset_error(error_value),
    }
}

fn write_asset(body: &[u8], state: &AdminState, query: &Query) -> HttpResponse {
    let (Some(path), Some(reason), Some(confirm)) =
        (query.get("path"), query.get("reason"), query.get("confirm"))
    else {
        return invalid("素材路径、原因或确认摘要不完整");
    };
    let summary = format!("asset:{path}:write");
    if !valid_reason(reason) || confirm != summary {
        return confirmation_required(summary);
    }
    let previous = match assets::read(&state.plugin_root, path) {
        Ok((bytes, _)) => Some(bytes),
        Err(assets::AssetError::NotFound) => None,
        Err(error_value) => return asset_error(error_value),
    };
    match assets::write(&state.plugin_root, path, body) {
        Ok(replaced) => match audit_external(
            state,
            "asset.write",
            "asset",
            path,
            reason,
            None,
            Some(serde_json::json!({"replaced": replaced, "bytes": body.len()})),
        ) {
            Ok(()) => ok(serde_json::json!({"path": path, "replaced": replaced})),
            Err(error_value) => {
                let rollback = if let Some(previous) = previous {
                    assets::write(&state.plugin_root, path, &previous).map(|_| ())
                } else {
                    assets::remove(&state.plugin_root, path)
                };
                if let Err(rollback_error) = rollback {
                    return asset_rollback_failed("write", &error_value, &rollback_error);
                }
                database_error(error_value)
            }
        },
        Err(error_value) => asset_error(error_value),
    }
}

fn remove_asset(body: &[u8], state: &AdminState, query: &Query) -> HttpResponse {
    let Some(path) = query.get("path") else {
        return invalid("缺少素材路径");
    };
    let request: ReasonRequest = match parse(body) {
        Ok(request) => request,
        Err(response) => return response,
    };
    let summary = format!("asset:{path}:delete");
    if !valid_reason(&request.reason) || !confirmed(&request.confirm, &summary) {
        return confirmation_required(summary);
    }
    let previous = match assets::read(&state.plugin_root, path) {
        Ok((bytes, _)) => bytes,
        Err(error_value) => return asset_error(error_value),
    };
    match assets::remove(&state.plugin_root, path) {
        Ok(()) => match audit_external(
            state,
            "asset.delete",
            "asset",
            path,
            request.reason.trim(),
            Some(serde_json::json!({"path": path})),
            None,
        ) {
            Ok(()) => ok(serde_json::json!({"deleted": path})),
            Err(error_value) => {
                if let Err(rollback_error) = assets::write(&state.plugin_root, path, &previous) {
                    return asset_rollback_failed("delete", &error_value, &rollback_error);
                }
                database_error(error_value)
            }
        },
        Err(error_value) => asset_error(error_value),
    }
}

fn export_assets(state: &AdminState, query: &Query) -> HttpResponse {
    let reason = query.get("reason").unwrap_or("");
    if !valid_reason(reason) {
        return invalid("导出原因需为 2 至 200 个字符");
    }
    match assets::export(&state.plugin_root) {
        Ok(bytes) => {
            if let Err(error_value) = audit_external(
                state,
                "asset.export",
                "asset_bundle",
                "assets",
                reason,
                None,
                Some(serde_json::json!({"bytes": bytes.len()})),
            ) {
                return database_error(error_value);
            }
            download(bytes, "application/zip", "luo-realm-assets.zip")
        }
        Err(error_value) => asset_error(error_value),
    }
}

fn import_assets(body: &[u8], state: &AdminState, query: &Query) -> HttpResponse {
    let (Some(reason), Some(confirm)) = (query.get("reason"), query.get("confirm")) else {
        return invalid("导入原因或确认摘要不完整");
    };
    if !valid_reason(reason) || confirm != "assets:import" {
        return confirmation_required("assets:import".into());
    }
    match assets::import(&state.plugin_root, body) {
        Ok(summary) => {
            let after = serde_json::json!({
                "imported": summary.imported,
                "replaced": summary.replaced
            });
            if let Err(error_value) = audit_external(
                state,
                "asset.import",
                "asset_bundle",
                "assets",
                reason,
                None,
                Some(after.clone()),
            ) {
                if let Err(rollback_error) = assets::rollback_bundle_import(&state.plugin_root) {
                    return asset_rollback_failed("import", &error_value, &rollback_error);
                }
                return database_error(error_value);
            }
            match assets::finalize_bundle_import(&state.plugin_root) {
                Ok(()) => ok(after),
                Err(error_value) => asset_error(error_value),
            }
        }
        Err(error_value) => asset_error(error_value),
    }
}

fn export_data(state: &AdminState, query: &Query) -> HttpResponse {
    let reason = query.get("reason").unwrap_or("");
    if !valid_reason(reason) {
        return invalid("导出原因需为 2 至 200 个字符");
    }
    match transfer::export_database(&state.plugin_root, &state.database_path) {
        Ok((name, bytes)) => {
            if let Err(error_value) = audit_external(
                state,
                "database.export",
                "database",
                &name,
                reason,
                None,
                Some(serde_json::json!({"bytes": bytes.len()})),
            ) {
                return database_error(error_value);
            }
            download(bytes, "application/vnd.sqlite3", &name)
        }
        Err(error_value) => transfer_error(error_value),
    }
}

fn import_data(body: &[u8], state: &AdminState, query: &Query) -> HttpResponse {
    let (Some(reason), Some(confirm)) = (query.get("reason"), query.get("confirm")) else {
        return invalid("导入原因或确认摘要不完整");
    };
    if !valid_reason(reason) || confirm != "database:import" {
        return confirmation_required("database:import".into());
    }
    match transfer::import_database(&state.plugin_root, &state.database_path, body, reason) {
        Ok(result) => {
            let after = serde_json::json!({"backup": result.backup_name});
            ok(after)
        }
        Err(error_value) => transfer_error(error_value),
    }
}

fn audit_external(
    state: &AdminState,
    action: &str,
    target_type: &str,
    target_id: &str,
    reason: &str,
    before: Option<serde_json::Value>,
    after: Option<serde_json::Value>,
) -> Result<(), DatabaseError> {
    let mut database = Database::open_request(&state.database_path)?;
    let transaction = database.immediate_transaction()?;
    admin::audit_success(
        &transaction,
        admin::AuditEntry {
            operator: "web",
            action,
            target_type,
            target_id,
            reason,
            before,
            after,
        },
    )?;
    transaction.commit().map_err(DatabaseError::from_sqlite)
}

#[derive(Deserialize)]
struct TokenRequest {
    token: String,
    reason: String,
    confirm: Option<String>,
}

fn rotate_token(body: &[u8], state: &AdminState) -> HttpResponse {
    let request: TokenRequest = match parse(body) {
        Ok(request) => request,
        Err(response) => return response,
    };
    if !valid_reason(&request.reason) || !confirmed(&request.confirm, "token:rotate") {
        return confirmation_required("token:rotate".into());
    }
    let mut database = match Database::open_request(&state.database_path) {
        Ok(database) => database,
        Err(error_value) => return database_error(error_value),
    };
    if let Err(error_value) = state.token.rotate(&request.token, &state.token_path) {
        return match error_value {
            super::auth::AuthError::InvalidToken => invalid("新 Token 至少需要 32 个字符"),
            _ => error(500, "token_rotation_failed", "无法安全替换管理 Token"),
        };
    }
    let audit_result = (|| {
        let transaction = database.immediate_transaction()?;
        admin::audit_success(
            &transaction,
            admin::AuditEntry {
                operator: "web",
                action: "token.rotate",
                target_type: "admin",
                target_id: "token",
                reason: request.reason.trim(),
                before: None,
                after: Some(serde_json::json!({"rotated": true})),
            },
        )?;
        transaction.commit().map_err(DatabaseError::from_sqlite)
    })();
    match audit_result {
        Ok(()) => ok(serde_json::json!({"rotated": true})),
        Err(error_value) => database_error(error_value),
    }
}

fn json_value(value: impl serde::Serialize) -> HttpResponse {
    match serde_json::to_value(value) {
        Ok(value) => ok(value),
        Err(_) => error(500, "serialization_failed", "无法序列化数据"),
    }
}

fn to_json(value: impl serde::Serialize) -> Result<serde_json::Value, DatabaseError> {
    serde_json::to_value(value)
        .map_err(|error_value| DatabaseError::Migration(error_value.to_string()))
}

fn parse<T: serde::de::DeserializeOwned>(body: &[u8]) -> Result<T, HttpResponse> {
    serde_json::from_slice(body)
        .map_err(|_| error(400, "invalid_json", "请求必须是合法且字段完整的 JSON"))
}

fn parse_id(value: &str) -> Option<u64> {
    value.parse::<u64>().ok().filter(|value| *value > 0)
}

fn valid_reason(reason: &str) -> bool {
    (2..=200).contains(&reason.trim().chars().count())
}

fn valid_code(code: &str) -> bool {
    (1..=64).contains(&code.len())
        && code
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

fn wallet_idempotency_key(player_id: u64, currency: &str) -> Result<String, HttpResponse> {
    let mut random = [0_u8; 16];
    getrandom::fill(&mut random)
        .map_err(|_| error(500, "random_failed", "无法生成钱包操作标识，请稍后重试"))?;
    let suffix = random
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(format!("admin:{player_id}:{currency}:{suffix}"))
}

fn confirmed(value: &Option<String>, expected: &str) -> bool {
    value.as_deref() == Some(expected)
}

fn invalid(message: &str) -> HttpResponse {
    error(400, "invalid_request", message)
}

fn confirmation_required(summary: String) -> HttpResponse {
    error(
        409,
        "confirmation_required",
        &format!("请确认目标摘要：{summary}"),
    )
}

fn database_error(error_value: DatabaseError) -> HttpResponse {
    let (status, code) = match error_value {
        DatabaseError::NotFound => (404, "not_found"),
        DatabaseError::Busy => (503, "database_busy"),
        DatabaseError::InsufficientBalance => (409, "insufficient_balance"),
        DatabaseError::Constraint(_)
        | DatabaseError::InvalidIdentifier
        | DatabaseError::InvalidData(_) => (400, "invalid_data"),
        DatabaseError::Corrupt(_) => (503, "database_corrupt"),
        DatabaseError::Unavailable(_) | DatabaseError::Migration(_) => (500, "database_error"),
    };
    let message = match code {
        "not_found" => "目标不存在",
        "database_busy" => "数据库正忙，请稍后重试",
        "insufficient_balance" => "余额不足",
        "invalid_data" => "数据不符合约束",
        "database_corrupt" => "数据库完整性检查失败，写操作已停止",
        _ => "数据库操作失败",
    };
    error(status, code, message)
}

fn asset_error(error_value: assets::AssetError) -> HttpResponse {
    match error_value {
        assets::AssetError::NotFound => error(404, "asset_not_found", "素材不存在"),
        assets::AssetError::InvalidPath => error(400, "invalid_asset_path", "素材路径不合法"),
        assets::AssetError::UnsupportedType => {
            error(400, "unsupported_asset", "仅支持 PNG 素材和 font.ttf 字体")
        }
        assets::AssetError::TooLarge => error(413, "asset_too_large", "素材或压缩包过大"),
        assets::AssetError::InvalidImage => error(400, "invalid_image", "PNG 文件内容无效"),
        assets::AssetError::InvalidArchive(_) => {
            error(400, "invalid_asset_archive", "素材压缩包无效")
        }
        assets::AssetError::Storage(_) => error(500, "asset_storage_failed", "素材存储失败"),
    }
}

fn asset_rollback_failed(
    operation: &str,
    audit_error: &DatabaseError,
    rollback_error: &assets::AssetError,
) -> HttpResponse {
    eprintln!(
        "[Luo Realm] asset {operation} audit failed ({audit_error}); rollback failed ({rollback_error})"
    );
    error(
        500,
        "asset_rollback_failed",
        "素材操作与审计均未完成，且自动回滚失败；请立即检查素材目录",
    )
}

fn transfer_error(error_value: transfer::TransferError) -> HttpResponse {
    match error_value {
        transfer::TransferError::InvalidSnapshot => {
            error(400, "invalid_snapshot", "上传文件不是 SQLite 数据库快照")
        }
        transfer::TransferError::Storage(_) => {
            error(500, "snapshot_storage_failed", "数据库快照存储失败")
        }
        transfer::TransferError::Database(error_value) => database_error(error_value),
    }
}

struct Query(Vec<(String, String)>);

impl Query {
    fn parse(url: &str) -> Self {
        let values = url
            .split_once('?')
            .map(|(_, query)| {
                query
                    .split('&')
                    .filter_map(|part| part.split_once('='))
                    .map(|(key, value)| (decode(key), decode(value)))
                    .collect()
            })
            .unwrap_or_default();
        Self(values)
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(candidate, _)| candidate == key)
            .map(|(_, value)| value.as_str())
    }

    fn number(&self, key: &str, default: usize) -> usize {
        self.get(key)
            .and_then(|value| value.parse().ok())
            .unwrap_or(default)
    }
}

fn decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => decoded.push(b' '),
            b'%' if index + 2 < bytes.len() => {
                if let Some(byte) = decode_hex_pair(bytes[index + 1], bytes[index + 2]) {
                    decoded.push(byte);
                    index += 2;
                } else {
                    decoded.push(bytes[index]);
                }
            }
            byte => decoded.push(byte),
        }
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn decode_hex_pair(high: u8, low: u8) -> Option<u8> {
    Some(hex_value(high)? * 16 + hex_value(low)?)
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}
