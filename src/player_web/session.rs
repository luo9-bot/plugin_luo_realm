//! 玩家网页的短期会话令牌。
//!
//! 会话是无状态的 HMAC 签名值：不需要服务端存储，插件重启后仍然有效；
//! 失效手段是有效期与根密钥轮换（设计方案书 20.3“短期会话”）。当前唯一
//! 授权范围是 `profile:read`，写入型操作始终由群聊命令完成。

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use serde::Serialize;
use sha2::Sha256;

/// 当前唯一授权范围：只读档案。
pub const SCOPE_PROFILE_READ: &str = "profile:read";

const SESSION_DOMAIN: &str = "player-web-session:v1";
const SESSION_VERSION: &str = "v1";
const NONCE_BYTES: usize = 16;

type HmacSha256 = Hmac<Sha256>;

/// 已验证的网页会话。
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Session {
    pub platform_user_id: u64,
    pub scope: String,
    pub expires_at: i64,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SessionError {
    #[error("session token is malformed")]
    Malformed,
    #[error("session token signature is invalid")]
    BadSignature,
    #[error("session token has expired")]
    Expired,
    #[error("session token scope is unknown")]
    UnknownScope,
}

/// 签发一个短期会话令牌。
pub fn mint(
    key: &[u8; 32],
    platform_user_id: u64,
    scope: &str,
    now: i64,
    ttl_seconds: i64,
) -> Result<String, SessionError> {
    if !matches!(scope, SCOPE_PROFILE_READ) {
        return Err(SessionError::UnknownScope);
    }
    if ttl_seconds <= 0 {
        return Err(SessionError::Malformed);
    }
    let mut nonce = [0_u8; NONCE_BYTES];
    getrandom::fill(&mut nonce).map_err(|_| SessionError::Malformed)?;
    let expires_at = now + ttl_seconds;
    let payload = format!(
        "{SESSION_VERSION}|{platform_user_id}|{scope}|{expires_at}|{}",
        URL_SAFE_NO_PAD.encode(nonce)
    );
    Ok(format!(
        "{SESSION_VERSION}.{}.{}",
        URL_SAFE_NO_PAD.encode(payload.as_bytes()),
        URL_SAFE_NO_PAD.encode(hmac_signature(key, &payload))
    ))
}

/// 校验会话令牌；过期、签名错误和未知范围都会被拒绝。
pub fn verify(key: &[u8; 32], token: &str, now: i64) -> Result<Session, SessionError> {
    let parts = token.split('.').collect::<Vec<_>>();
    let [version, payload, signature] = parts.as_slice() else {
        return Err(SessionError::Malformed);
    };
    if *version != SESSION_VERSION {
        return Err(SessionError::Malformed);
    }
    let payload_bytes = URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|_| SessionError::Malformed)?;
    let payload = String::from_utf8(payload_bytes).map_err(|_| SessionError::Malformed)?;
    let expected = hmac_signature(key, &payload);
    let provided = URL_SAFE_NO_PAD
        .decode(signature)
        .map_err(|_| SessionError::Malformed)?;
    if provided.len() != expected.len()
        || provided
            .iter()
            .zip(expected)
            .fold(0_u8, |difference, (left, right)| {
                difference | (left ^ right)
            })
            != 0
    {
        return Err(SessionError::BadSignature);
    }

    let mut fields = payload.split('|');
    let version = fields.next().ok_or(SessionError::Malformed)?;
    let platform_user_id = fields.next().ok_or(SessionError::Malformed)?;
    let scope = fields.next().ok_or(SessionError::Malformed)?;
    let expires_at = fields.next().ok_or(SessionError::Malformed)?;
    let nonce = fields.next().ok_or(SessionError::Malformed)?;
    if nonce.is_empty() || fields.next().is_some() || version != SESSION_VERSION {
        return Err(SessionError::Malformed);
    }
    let platform_user_id: u64 = platform_user_id
        .parse()
        .map_err(|_| SessionError::Malformed)?;
    let expires_at: i64 = expires_at.parse().map_err(|_| SessionError::Malformed)?;
    if scope != SCOPE_PROFILE_READ {
        return Err(SessionError::UnknownScope);
    }
    if expires_at <= now {
        return Err(SessionError::Expired);
    }
    Ok(Session {
        platform_user_id,
        scope: scope.to_owned(),
        expires_at,
    })
}

fn hmac_signature(key: &[u8; 32], payload: &str) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(key).expect("hmac accepts any key length");
    mac.update(SESSION_DOMAIN.as_bytes());
    mac.update(payload.as_bytes());
    mac.finalize().into_bytes().into()
}

#[cfg(test)]
mod tests;
