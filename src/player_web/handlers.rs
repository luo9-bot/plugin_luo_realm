//! 玩家网页的 HTTP 路由与处理。
//!
//! 路由在管理鉴权之前由 `admin::router` 委托到这里：页面与静态资源公开，
//! 票据兑换公开（票据本身即凭据），只读接口要求 `Authorization: Bearer`
//! 携带玩家会话。全部接口都在安全响应头保护之下，跨域来源由配置白名单
//! 控制（设计方案书 27.2、27.4）。

use std::sync::Arc;

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
            Some(html_page(&config))
        }
        (tiny_http::Method::Get, "/player/app.js") => Some(static_asset(
            include_str!("assets/app.js"),
            "text/javascript; charset=utf-8",
            &config,
            request,
        )),
        (tiny_http::Method::Get, "/player/style.css") => Some(static_asset(
            include_str!("assets/style.css"),
            "text/css; charset=utf-8",
            &config,
            request,
        )),
        (tiny_http::Method::Options, _) if path.starts_with("/api/player/") => {
            Some(preflight(&config, request))
        }
        (tiny_http::Method::Post, "/api/player/session") => {
            Some(exchange_endpoint(request, state, &config))
        }
        (tiny_http::Method::Get, view_path) if view_path.starts_with("/api/player/") => {
            Some(read_endpoint(view_path, request, state, &config))
        }
        _ => None,
    }
}

fn html_page(config: &RuntimeConfig) -> HttpResponse {
    with_cors(
        crate::admin::router::html(include_str!("assets/index.html")),
        config,
        None,
    )
}

fn static_asset(
    content: &str,
    content_type: &str,
    config: &RuntimeConfig,
    request: &tiny_http::Request,
) -> HttpResponse {
    let response = crate::admin::router::static_text(content, content_type);
    with_cors(response, config, origin_of(request))
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
