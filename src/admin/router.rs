use std::{
    io::{Cursor, Read},
    path::PathBuf,
    sync::{Arc, Mutex, TryLockError},
};

use tiny_http::{Header, Method, Request, Response, StatusCode};

use crate::{config::RuntimePolicy, database::Database};

use super::{auth::AdminToken, handlers, ui};

const MAX_BODY_BYTES: usize = 1024 * 1024;
const MAX_ASSET_BODY_BYTES: usize = 16 * 1024 * 1024;
const MAX_IMPORT_BODY_BYTES: usize = 128 * 1024 * 1024;

pub struct AdminState {
    pub plugin_root: PathBuf,
    pub database_path: PathBuf,
    pub token_path: PathBuf,
    pub token: AdminToken,
    pub policy: RuntimePolicy,
    pub port: u16,
    pub operation_lock: Mutex<()>,
    pub upload_lock: Mutex<()>,
}

pub type HttpResponse = Response<Cursor<Vec<u8>>>;

pub fn route(request: &mut Request, state: &Arc<AdminState>) -> HttpResponse {
    let method = request.method().clone();
    let url = request.url().to_owned();
    let path = url.split('?').next().unwrap_or(&url);

    if method == Method::Get && matches!(path, "/" | "/index.html") {
        return html(ui::HTML);
    }
    if method == Method::Get && path == "/admin.css" {
        return static_text(ui::CSS, "text/css; charset=utf-8");
    }
    if method == Method::Get && path == "/admin.js" {
        return static_text(ui::JAVASCRIPT, "text/javascript; charset=utf-8");
    }
    if method == Method::Get && path == "/api/health" {
        let database_ok = Database::open_request(&state.database_path).is_ok();
        return ok(serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"),
            "port": state.port,
            "database_ok": database_ok
        }));
    }

    if method == Method::Post && path == "/api/login" {
        let body = match read_body(request, MAX_BODY_BYTES) {
            Ok(body) => body,
            Err(response) => return response,
        };
        let value: serde_json::Value = match serde_json::from_slice(&body) {
            Ok(value) => value,
            Err(_) => return error(400, "invalid_json", "请求必须是合法 JSON"),
        };
        let candidate = value
            .get("token")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        return if state.token.verify(candidate) {
            ok(serde_json::json!({"authenticated": true}))
        } else {
            error(403, "invalid_token", "管理 Token 不正确")
        };
    }
    if !authenticated(request, &state.token) {
        return error(401, "unauthorized", "缺少或无效的管理 Token");
    }

    let _upload = if requires_upload_slot(&method, path) {
        match state.upload_lock.try_lock() {
            Ok(guard) => Some(guard),
            Err(TryLockError::WouldBlock) => {
                return error(429, "upload_busy", "已有上传任务正在处理，请稍后重试");
            }
            Err(TryLockError::Poisoned(_)) => {
                return error(503, "admin_unavailable", "上传操作锁不可用，请重启插件");
            }
        }
    } else {
        None
    };
    let body = match read_body(request, body_limit(path)) {
        Ok(body) => body,
        Err(response) => return response,
    };

    let _operation = match state.operation_lock.lock() {
        Ok(guard) => guard,
        Err(_) => return error(503, "admin_unavailable", "后台操作锁不可用，请重启插件"),
    };
    handlers::dispatch(&method, &url, &body, state)
}

fn requires_upload_slot(method: &Method, path: &str) -> bool {
    method == &Method::Post
        && matches!(
            path,
            "/api/assets/file" | "/api/assets/import" | "/api/data/import"
        )
}

fn read_body(request: &mut Request, limit: usize) -> Result<Vec<u8>, HttpResponse> {
    if request.body_length().unwrap_or(0) > limit {
        return Err(error(413, "body_too_large", "请求体超过该接口允许的大小"));
    }
    let mut body = Vec::new();
    let mut reader = request.as_reader().take((limit + 1) as u64);
    if std::io::Read::read_to_end(&mut reader, &mut body).is_err() {
        return Err(error(400, "read_failed", "无法读取请求体"));
    }
    if body.len() > limit {
        return Err(error(413, "body_too_large", "请求体超过该接口允许的大小"));
    }
    Ok(body)
}

fn body_limit(path: &str) -> usize {
    match path {
        "/api/assets/import" | "/api/data/import" => MAX_IMPORT_BODY_BYTES,
        "/api/assets/file" => MAX_ASSET_BODY_BYTES,
        _ => MAX_BODY_BYTES,
    }
}

fn authenticated(request: &Request, token: &AdminToken) -> bool {
    request.headers().iter().any(|header| {
        header.field.equiv("Authorization")
            && header
                .value
                .as_str()
                .strip_prefix("Bearer ")
                .is_some_and(|candidate| token.verify(candidate))
    })
}

pub fn ok(data: serde_json::Value) -> HttpResponse {
    json(200, serde_json::json!({"ok": true, "data": data}))
}

pub fn error(status: u16, code: &str, message: &str) -> HttpResponse {
    json(
        status,
        serde_json::json!({"ok": false, "error": {"code": code, "message": message}}),
    )
}

fn json(status: u16, value: serde_json::Value) -> HttpResponse {
    let response =
        Response::from_data(value.to_string().into_bytes()).with_status_code(StatusCode(status));
    secure_headers(with_header(
        response,
        "Content-Type",
        "application/json; charset=utf-8",
    ))
}

fn html(content: &str) -> HttpResponse {
    let response = Response::from_data(content.as_bytes().to_vec());
    let response = with_header(response, "Content-Type", "text/html; charset=utf-8");
    secure_headers(with_header(
        response,
        "Content-Security-Policy",
        "default-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self' blob: data:; object-src 'none'; base-uri 'none'",
    ))
}

fn static_text(content: &str, content_type: &str) -> HttpResponse {
    secure_headers(with_header(
        Response::from_data(content.as_bytes().to_vec()),
        "Content-Type",
        content_type,
    ))
}

pub fn binary(bytes: Vec<u8>, content_type: &str) -> HttpResponse {
    secure_headers(with_header(
        Response::from_data(bytes),
        "Content-Type",
        content_type,
    ))
}

pub fn download(bytes: Vec<u8>, content_type: &str, filename: &str) -> HttpResponse {
    let safe_name = filename
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_')
        })
        .collect::<String>();
    let response = with_header(Response::from_data(bytes), "Content-Type", content_type);
    secure_headers(with_header(
        response,
        "Content-Disposition",
        &format!("attachment; filename=\"{safe_name}\""),
    ))
}

fn secure_headers(response: HttpResponse) -> HttpResponse {
    let response = with_header(response, "Cache-Control", "no-store");
    let response = with_header(response, "X-Content-Type-Options", "nosniff");
    let response = with_header(response, "X-Frame-Options", "DENY");
    with_header(response, "Referrer-Policy", "no-referrer")
}

fn with_header(response: HttpResponse, name: &str, value: &str) -> HttpResponse {
    match Header::from_bytes(name, value) {
        Ok(header) => response.with_header(header),
        Err(_) => response,
    }
}
