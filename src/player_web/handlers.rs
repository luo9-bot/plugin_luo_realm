//! 玩家网页的 HTTP 路由与处理。
//!
//! 路由在管理鉴权之前由 `admin::router` 委托到这里：页面与静态资源公开，
//! 票据兑换公开（票据本身即凭据），只读接口要求 `Authorization: Bearer`
//! 携带玩家会话。全部接口都在安全响应头保护之下，跨域来源由配置白名单
//! 控制（设计方案书 27.2、27.4）。

use std::{io::Cursor, path::PathBuf, sync::Arc};

use serde::Deserialize;

use crate::admin::router::{AdminState, HttpResponse};
use crate::config::RuntimeConfig;
use crate::database::{Database, DatabaseError, unix_timestamp};
use crate::domain::error_code::StableErrorCode;
use crate::domain::shared::PlatformUserId;

use super::{
    session::{self, Session},
    ticket::{self, TicketError},
    views,
};

/// 委托玩家网页路由；返回 `None` 表示该请求与本模块无关。
pub fn route(
    method: tiny_http::Method,
    path: &str,
    request: &mut tiny_http::Request,
    state: &Arc<AdminState>,
) -> Option<HttpResponse> {
    let config = state.policy.snapshot();
    match (method.clone(), path) {
        (tiny_http::Method::Get, "/player") | (tiny_http::Method::Get, "/player/") => {
            Some(page_index(state, &config))
        }
        (tiny_http::Method::Get, file_path) if file_path.starts_with("/player/") => {
            Some(page_asset(state, &config, file_path, request))
        }
        (tiny_http::Method::Options, _) if path.starts_with("/api/player/") => {
            Some(preflight(&config, request))
        }
        (tiny_http::Method::Get, "/api/player/meta/rarity") => {
            Some(rarity_meta_endpoint(state, &config))
        }
        (tiny_http::Method::Get, "/api/player/portraits") => {
            Some(portraits_endpoint(request, state, &config))
        }
        (tiny_http::Method::Post, "/api/player/character") => {
            Some(set_character_endpoint(request, state, &config))
        }
        (tiny_http::Method::Post, "/api/player/session") => {
            Some(exchange_endpoint(request, state, &config))
        }
        (tiny_http::Method::Get, asset_path) if asset_path.starts_with("/api/player/asset/") => {
            Some(asset_endpoint(state, &config, asset_path, request))
        }
        (tiny_http::Method::Get, view_path) if view_path.starts_with("/api/player/") => {
            Some(read_endpoint(view_path, request, state, &config))
        }
        _ => None,
    }
}

/// 部署目录：把 `player_page/dist` 的构建产物复制到该目录即可启用 Vue 页面。
fn page_directory(state: &AdminState) -> PathBuf {
    crate::paths::data_directory(&state.plugin_root).join("player_page")
}

fn page_index(state: &Arc<AdminState>, config: &RuntimeConfig) -> HttpResponse {
    if let Ok(bytes) = std::fs::read(page_directory(state).join("index.html")) {
        return with_cors(crate::admin::router::html_bytes(bytes), config, None);
    }
    with_cors(
        crate::admin::router::html(include_str!("assets/index.html")),
        config,
        None,
    )
}

/// 伺服部署目录下的静态文件；旧内嵌资源在磁盘缺失时兜底。
fn page_asset(
    state: &Arc<AdminState>,
    config: &RuntimeConfig,
    path: &str,
    request: &tiny_http::Request,
) -> HttpResponse {
    let relative = path.trim_start_matches("/player/");
    if is_unsafe_relative(relative) {
        return crate::admin::router::error(404, "asset_not_found", "资源不存在");
    }
    if let Ok(bytes) = std::fs::read(page_directory(state).join(relative)) {
        let response = crate::admin::router::binary(bytes, content_type_of(relative));
        return with_cors(response, config, origin_of(request));
    }
    match relative {
        "app.js" => with_cors(
            crate::admin::router::static_text(
                include_str!("assets/app.js"),
                "text/javascript; charset=utf-8",
            ),
            config,
            origin_of(request),
        ),
        "style.css" => with_cors(
            crate::admin::router::static_text(
                include_str!("assets/style.css"),
                "text/css; charset=utf-8",
            ),
            config,
            origin_of(request),
        ),
        _ => crate::admin::router::error(404, "asset_not_found", "资源不存在"),
    }
}

fn is_unsafe_relative(relative: &str) -> bool {
    relative.is_empty()
        || relative.contains('\\')
        || relative.contains("..")
        || relative.starts_with('/')
        || relative.contains("//")
        || relative.chars().any(char::is_control)
}

fn content_type_of(path: &str) -> &'static str {
    if path.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if path.ends_with(".js") || path.ends_with(".mjs") {
        "text/javascript; charset=utf-8"
    } else if path.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if path.ends_with(".png") {
        "image/png"
    } else if path.ends_with(".svg") {
        "image/svg+xml"
    } else if path.ends_with(".woff2") {
        "font/woff2"
    } else {
        "application/octet-stream"
    }
}

/// 群内卡片与网页共用的素材路由：图标按定义名、形象按角色 ID。
fn asset_endpoint(
    state: &Arc<AdminState>,
    config: &RuntimeConfig,
    path: &str,
    request: &tiny_http::Request,
) -> HttpResponse {
    let origin = origin_of(request);
    let rest = path.trim_start_matches("/api/player/asset/");
    let Some((kind, file)) = rest.split_once('/') else {
        return finish(
            crate::admin::router::error(404, "asset_not_found", "资源不存在"),
            config,
            origin,
        );
    };
    let Some(name) = file.strip_suffix(".png") else {
        return finish(
            crate::admin::router::error(404, "asset_not_found", "资源不存在"),
            config,
            origin,
        );
    };
    let Some(name) = percent_decode(name) else {
        return finish(
            crate::admin::router::error(400, "invalid_asset_name", "资源名称不合法"),
            config,
            origin,
        );
    };
    if name.is_empty()
        || name.contains('/')
        || name.contains('\\')
        || name.contains("..")
        || name.chars().any(char::is_control)
    {
        return finish(
            crate::admin::router::error(400, "invalid_asset_name", "资源名称不合法"),
            config,
            origin,
        );
    }
    let assets = crate::render::assets::RealmAssets::discover(&state.plugin_root);
    let icon = match kind {
        "icon" => assets.equipment_icon(&name),
        "portrait" => assets.portrait_by_id(&name),
        _ => None,
    };
    let response = match icon {
        Some(image) => {
            let mut bytes = Cursor::new(Vec::new());
            match image.write_to(&mut bytes, image::ImageFormat::Png) {
                Ok(()) => crate::admin::router::binary(bytes.into_inner(), "image/png"),
                Err(_) => crate::admin::router::error(500, "asset_encode_failed", "素材编码失败"),
            }
        }
        None => crate::admin::router::error(404, "asset_not_found", "素材不存在"),
    };
    finish(response, config, origin)
}

/// 解码路径段中的百分号编码（`encodeURIComponent` 产物，不处理 `+`）。
fn percent_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' => {
                let hex = bytes.get(index + 1..index + 3)?;
                let value = u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?;
                output.push(value);
                index += 3;
            }
            byte => {
                output.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(output).ok()
}

fn preflight(config: &RuntimeConfig, request: &tiny_http::Request) -> HttpResponse {
    let Some(origin) = origin_of(request) else {
        return crate::admin::router::error(400, "missing_origin", "缺少 Origin 头");
    };
    if !origin_allowed(config, &origin) {
        return crate::admin::router::error(403, "origin_forbidden", "该来源不在白名单内");
    }
    let response =
        tiny_http::Response::from_data(Vec::new()).with_status_code(tiny_http::StatusCode(204));
    let response = with_header(response, "Access-Control-Allow-Origin", &origin);
    let response = with_header(
        response,
        "Access-Control-Allow-Methods",
        "GET, POST, OPTIONS",
    );
    let response = with_header(
        response,
        "Access-Control-Allow-Headers",
        "Authorization, Content-Type",
    );
    let response = with_header(response, "Access-Control-Max-Age", "600");
    with_header(response, "Vary", "Origin")
}

fn exchange_endpoint(
    request: &mut tiny_http::Request,
    state: &Arc<AdminState>,
    config: &RuntimeConfig,
) -> HttpResponse {
    let origin = origin_of(request);
    let body = match crate::admin::router::read_body(request, 4_096) {
        Ok(body) => body,
        Err(response) => return response,
    };
    let token = serde_json::from_slice::<serde_json::Value>(&body)
        .ok()
        .and_then(|value| {
            value
                .get("ticket")
                .and_then(|value| value.as_str())
                .map(str::to_owned)
        });
    let Some(token) = token else {
        return finish(
            crate::admin::router::error(400, "invalid_ticket_request", "请求必须包含 ticket 字段"),
            config,
            origin,
        );
    };

    let key = state.token.signing_key();
    let now = unix_timestamp();
    let outcome = Database::open_request(&state.database_path)
        .map_err(TicketError::Storage)
        .and_then(|mut database| {
            let db_transaction = database.read_transaction()?;
            let credential = ticket::exchange(&db_transaction, &token, &key, now)?;
            let session = session::mint(
                &key,
                credential.platform_user_id,
                &credential.scope,
                now,
                i64::from(config.player_web.session_ttl_minutes) * 60,
            )?;
            db_transaction
                .commit()
                .map_err(DatabaseError::from_sqlite)?;
            Ok(serde_json::json!({
                "session_token": session,
                "scope": credential.scope,
                "expires_in_minutes": config.player_web.session_ttl_minutes,
            }))
        });

    let response = match outcome {
        Ok(value) => crate::admin::router::ok(value),
        Err(error) => ticket_error_response(&error),
    };
    finish(response, config, origin)
}

fn read_endpoint(
    view_path: &str,
    request: &mut tiny_http::Request,
    state: &Arc<AdminState>,
    config: &RuntimeConfig,
) -> HttpResponse {
    let origin = origin_of(request);
    let Some(session) = authorize(request, state) else {
        return finish(
            crate::admin::router::error(401, "session_invalid", "会话缺失、无效或已过期"),
            config,
            origin,
        );
    };
    let platform_user_id = PlatformUserId::new(session.platform_user_id);
    let outcome = Database::open_request(&state.database_path)
        .map_err(ReadFailure::Storage)
        .and_then(|mut database| {
            let date = database.local_date().map_err(ReadFailure::Storage)?;
            let db_transaction = database.read_transaction().map_err(ReadFailure::Storage)?;
            let view: Result<serde_json::Value, ReadFailure> = match view_path {
                "/api/player/profile" => views::profile(&db_transaction, platform_user_id, &date)
                    .map_err(ReadFailure::Storage)
                    .and_then(serialize_view),
                "/api/player/wallet" => views::wallet(&db_transaction, platform_user_id)
                    .map_err(ReadFailure::Storage)
                    .and_then(serialize_view),
                "/api/player/skills" => views::skills_view(&db_transaction, platform_user_id)
                    .map_err(ReadFailure::Storage)
                    .and_then(serialize_view),
                "/api/player/equipment" => views::equipment(&db_transaction, platform_user_id)
                    .map_err(ReadFailure::Storage)
                    .and_then(serialize_view),
                "/api/player/battles" => views::battles(&db_transaction, platform_user_id)
                    .map_err(ReadFailure::Storage)
                    .and_then(serialize_view),
                _ => Err(ReadFailure::UnknownView),
            };
            db_transaction
                .commit()
                .map_err(DatabaseError::from_sqlite)
                .map_err(ReadFailure::Storage)?;
            view
        });

    let response = match outcome {
        Ok(value) => crate::admin::router::ok(value),
        Err(failure) => failure.into_response(),
    };
    finish(response, config, origin)
}

fn serialize_view<T: serde::Serialize>(view: T) -> Result<serde_json::Value, ReadFailure> {
    serde_json::to_value(view).map_err(ReadFailure::Serialization)
}

fn authorize(request: &tiny_http::Request, state: &Arc<AdminState>) -> Option<Session> {
    let header = request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Authorization"))?;
    let token = header.value.as_str().strip_prefix("Bearer ")?;
    let key = state.token.signing_key();
    session::verify(&key, token, unix_timestamp()).ok()
}

/// 品阶元数据：卡片、详情卡与网页共用的同一份注册表。
fn rarity_meta_endpoint(state: &Arc<AdminState>, config: &RuntimeConfig) -> HttpResponse {
    let tiers = crate::domain::rules::rarity_tiers(&state.plugin_root);
    finish(
        crate::admin::router::ok(serde_json::json!({ "tiers": tiers })),
        config,
        None,
    )
}

/// 可选角色形象列表（需要会话）。
fn portraits_endpoint(
    request: &tiny_http::Request,
    state: &Arc<AdminState>,
    config: &RuntimeConfig,
) -> HttpResponse {
    let origin = origin_of(request);
    if authorize(request, state).is_none() {
        return finish(
            crate::admin::router::error(401, "session_invalid", "会话缺失、无效或已过期"),
            config,
            origin,
        );
    }
    let ids = crate::render::assets::RealmAssets::discover(&state.plugin_root).portrait_ids();
    finish(
        crate::admin::router::ok(serde_json::json!({ "portraits": ids })),
        config,
        origin,
    )
}

#[derive(Deserialize)]
struct CharacterRequest {
    character_id: String,
}

/// 设置角色形象：网页上唯一的外观类写入，不触及任何数值资产。
fn set_character_endpoint(
    request: &mut tiny_http::Request,
    state: &Arc<AdminState>,
    config: &RuntimeConfig,
) -> HttpResponse {
    let origin = origin_of(request);
    let Some(session) = authorize(request, state) else {
        return finish(
            crate::admin::router::error(401, "session_invalid", "会话缺失、无效或已过期"),
            config,
            origin,
        );
    };
    let body = match crate::admin::router::read_body(request, 1_024) {
        Ok(body) => body,
        Err(response) => return response,
    };
    let Some(character_id) = serde_json::from_slice::<CharacterRequest>(&body)
        .ok()
        .map(|request| request.character_id)
    else {
        return finish(
            crate::admin::router::error(400, "invalid_request", "请求必须包含 character_id"),
            config,
            origin,
        );
    };
    let assets = crate::render::assets::RealmAssets::discover(&state.plugin_root);
    if assets.portrait_by_id(&character_id).is_none() {
        return finish(
            crate::admin::router::error(404, "portrait_not_found", "该形象不存在"),
            config,
            origin,
        );
    }
    let outcome = Database::open_request(&state.database_path).and_then(|mut database| {
        let db_transaction = database.immediate_transaction()?;
        let row_id = i64::try_from(session.platform_user_id)
            .map_err(|_| DatabaseError::InvalidIdentifier)?;
        let updated = db_transaction
            .execute(
                "UPDATE player_profiles SET character_id=?2 WHERE player_id=?1",
                rusqlite::params![row_id, character_id],
            )
            .map_err(DatabaseError::from_sqlite)?;
        if updated == 0 {
            return Err(DatabaseError::NotFound);
        }
        crate::database::admin::audit_success(
            &db_transaction,
            crate::database::admin::AuditEntry {
                operator: &format!("player:{}", session.platform_user_id),
                action: "player.set_character",
                target_type: "player",
                target_id: &session.platform_user_id.to_string(),
                reason: "网页档案页更换形象",
                before: None,
                after: Some(serde_json::json!({ "character_id": character_id })),
            },
        )?;
        db_transaction
            .commit()
            .map_err(DatabaseError::from_sqlite)?;
        Ok(serde_json::json!({ "character_id": character_id }))
    });
    let response = match outcome {
        Ok(value) => crate::admin::router::ok(value),
        Err(DatabaseError::NotFound) => {
            crate::admin::router::error(404, "player_not_found", "档案不存在")
        }
        Err(error) => crate::admin::router::error(500, error.error_code(), "设置形象失败"),
    };
    finish(response, config, origin)
}

fn ticket_error_response(error: &TicketError) -> HttpResponse {
    let (status, message) = match error {
        TicketError::Malformed => (400, "票据格式不正确"),
        TicketError::BadSignature => (403, "票据签名无效"),
        TicketError::Unavailable => (401, "票据不存在、已过期或已使用"),
        TicketError::AlreadyUsed => (401, "票据已被使用"),
        TicketError::CorruptPlayer | TicketError::Session(_) | TicketError::Storage(_) => {
            (500, "玩家网页内部错误")
        }
    };
    crate::admin::router::error(status, error.error_code(), message)
}

/// 只读视图链路的全部失败：未知视图、存储错误与序列化错误。
enum ReadFailure {
    UnknownView,
    Storage(DatabaseError),
    Serialization(serde_json::Error),
}

impl ReadFailure {
    fn into_response(self) -> HttpResponse {
        match self {
            Self::UnknownView => crate::admin::router::error(404, "unknown_view", "未知的数据视图"),
            Self::Storage(error) => {
                crate::admin::router::error(500, error.error_code(), "玩家网页内部错误")
            }
            Self::Serialization(error) => crate::admin::router::error(
                500,
                "view_serialization_failed",
                &format!("视图序列化失败：{error}"),
            ),
        }
    }
}

fn origin_of(request: &tiny_http::Request) -> Option<String> {
    request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Origin"))
        .map(|header| header.value.as_str().to_owned())
}

fn origin_allowed(config: &RuntimeConfig, origin: &str) -> bool {
    config
        .player_web
        .allowed_origins
        .iter()
        .any(|allowed| allowed == origin)
}

fn with_cors(
    response: HttpResponse,
    config: &RuntimeConfig,
    origin: Option<String>,
) -> HttpResponse {
    match origin.filter(|origin| origin_allowed(config, origin)) {
        Some(origin) => with_header(response, "Access-Control-Allow-Origin", &origin),
        None => response,
    }
}

fn finish(response: HttpResponse, config: &RuntimeConfig, origin: Option<String>) -> HttpResponse {
    with_cors(response, config, origin)
}

fn with_header(response: HttpResponse, name: &str, value: &str) -> HttpResponse {
    match tiny_http::Header::from_bytes(name, value) {
        Ok(header) => response.with_header(header),
        Err(_) => response,
    }
}
