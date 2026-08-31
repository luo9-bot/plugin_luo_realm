use std::{
    io::{Cursor, Read},
    path::PathBuf,
    sync::Arc,
};

use tiny_http::{Header, Method, Request, Response, StatusCode};

use crate::{config::RuntimePolicy, database::Database};

use super::{auth::AdminToken, handlers, ui};

const MAX_BODY_BYTES: usize = 1024 * 1024;

pub struct AdminState {
    pub plugin_root: PathBuf,
    pub database_path: PathBuf,
    pub token_path: PathBuf,
    pub token: AdminToken,
    pub policy: RuntimePolicy,
    pub port: u16,
}

pub type HttpResponse = Response<Cursor<Vec<u8>>>;

pub fn route(request: &mut Request, state: &Arc<AdminState>) -> HttpResponse {
    let method = request.method().clone();
    let url = request.url().to_owned();
    let path = url.split('?').next().unwrap_or(&url);

    if method == Method::Get && matches!(path, "/" | "/index.html") {
        return html(ui::HTML);
    }
    if method == Method::Get && path == "/api/health" {
        let database_ok = Database::open_request(&state.database_path).is_ok();
        return ok(serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"),
            "port": state.port,
            "database_ok": database_ok
        }));
    }

    let body = match read_body(request) {
        Ok(body) => body,
        Err(response) => return response,
    };
    if method == Method::Post && path == "/api/login" {
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

    handlers::dispatch(&method, &url, &body, state)
}

fn read_body(request: &mut Request) -> Result<Vec<u8>, HttpResponse> {
    if request.body_length().unwrap_or(0) > MAX_BODY_BYTES {
        return Err(error(413, "body_too_large", "请求体不能超过 1 MiB"));
    }
    let mut body = Vec::new();
    let mut reader = request.as_reader().take((MAX_BODY_BYTES + 1) as u64);
    if std::io::Read::read_to_end(&mut reader, &mut body).is_err() {
        return Err(error(400, "read_failed", "无法读取请求体"));
    }
    if body.len() > MAX_BODY_BYTES {
        return Err(error(413, "body_too_large", "请求体不能超过 1 MiB"));
    }
    Ok(body)
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
    secure_headers(
        Response::from_data(value.to_string().into_bytes())
            .with_status_code(StatusCode(status))
            .with_header(header("Content-Type", "application/json; charset=utf-8")),
    )
}

fn html(content: &str) -> HttpResponse {
    secure_headers(
        Response::from_data(content.as_bytes().to_vec())
            .with_header(header("Content-Type", "text/html; charset=utf-8"))
            .with_header(header(
                "Content-Security-Policy",
                "default-src 'self'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; connect-src 'self'; img-src 'self' data:",
            )),
    )
}

fn secure_headers(response: HttpResponse) -> HttpResponse {
    response
        .with_header(header("Cache-Control", "no-store"))
        .with_header(header("X-Content-Type-Options", "nosniff"))
        .with_header(header("X-Frame-Options", "DENY"))
        .with_header(header("Referrer-Policy", "no-referrer"))
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name, value).expect("static HTTP header must be valid")
}
