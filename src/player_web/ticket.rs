//! 一次性网页票据的签发与兑换。
//!
//! 票据由群聊命令 `主页` 签发：随机 nonce 持久化到 `player_web_tickets`，
//! 对外只暴露 `nonce + HMAC 签名`。兑换必须在有效期内且未被使用；使用
//! 标记在同一事务中写入，保证重复请求、并发兑换和插件重启都不会让同一
//! 票据产生两个会话（设计方案书 20.3）。

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use rusqlite::{OptionalExtension, Transaction, params};
use sha2::Sha256;

use crate::database::{DatabaseError, DatabaseResult, player_id};
use crate::domain::error_code::StableErrorCode;

use super::session::SessionError;

const TICKET_DOMAIN: &str = "player-web-ticket:v1";
const NONCE_BYTES: usize = 32;

type HmacSha256 = Hmac<Sha256>;

/// 已签发的一次性票据。
#[derive(Clone, Debug)]
pub struct IssuedTicket {
    pub token: String,
    pub expires_at: i64,
}

/// 兑换成功后得到的授权上下文。
#[derive(Clone, Debug)]
pub struct ExchangeCredential {
    pub platform_user_id: u64,
    pub scope: String,
}

#[derive(Debug, thiserror::Error)]
pub enum TicketError {
    #[error("ticket is malformed")]
    Malformed,
    #[error("ticket signature is invalid")]
    BadSignature,
    #[error("ticket not found or expired")]
    Unavailable,
    #[error("ticket was already used")]
    AlreadyUsed,
    #[error("ticket references an invalid player")]
    CorruptPlayer,
    #[error("session minting failed: {0}")]
    Session(#[from] SessionError),
    #[error("player web storage failure: {0}")]
    Storage(#[from] DatabaseError),
}

impl StableErrorCode for TicketError {
    fn error_code(&self) -> &'static str {
        match self {
            Self::Malformed => "player_web.ticket_malformed",
            Self::BadSignature => "player_web.ticket_bad_signature",
            Self::Unavailable => "player_web.ticket_unavailable",
            Self::AlreadyUsed => "player_web.ticket_already_used",
            Self::CorruptPlayer => "player_web.ticket_corrupt_player",
            Self::Session(_) => "player_web.session_error",
            Self::Storage(error) => error.error_code(),
        }
    }
}

/// 签发一次性票据并顺带清理过期记录。
pub fn issue(
    transaction: &Transaction<'_>,
    platform_user_id: u64,
    scope: &str,
    key: &[u8; 32],
    now: i64,
    ttl_seconds: i64,
) -> DatabaseResult<IssuedTicket> {
    let player = player_id(platform_user_id)?;
    transaction
        .execute(
            "DELETE FROM player_web_tickets WHERE expires_at < ?1",
            [now],
        )
        .map_err(DatabaseError::from_sqlite)?;
    let mut nonce = [0_u8; NONCE_BYTES];
    getrandom::fill(&mut nonce).map_err(|error| DatabaseError::InvalidData(error.to_string()))?;
    let nonce = URL_SAFE_NO_PAD.encode(nonce);
    let expires_at = now + ttl_seconds;
    transaction
        .execute(
            "INSERT INTO player_web_tickets(nonce, player_id, scope, issued_at, expires_at)
             VALUES(?1, ?2, ?3, ?4, ?5)",
            params![nonce, player, scope, now, expires_at],
        )
        .map_err(DatabaseError::from_sqlite)?;
    Ok(IssuedTicket {
        token: format!(
            "{nonce}.{}",
            URL_SAFE_NO_PAD.encode(hmac_signature(key, &nonce))
        ),
        expires_at,
    })
}

/// 兑换票据：验签、检查有效期与使用标记，并在同一事务中写入使用时间。
pub fn exchange(
    transaction: &Transaction<'_>,
    token: &str,
    key: &[u8; 32],
    now: i64,
) -> Result<ExchangeCredential, TicketError> {
    let (nonce, signature) = token.split_once('.').ok_or(TicketError::Malformed)?;
    let expected = hmac_signature(key, nonce);
    let provided = URL_SAFE_NO_PAD
        .decode(signature)
        .map_err(|_| TicketError::Malformed)?;
    if provided.len() != expected.len()
        || provided
            .iter()
            .zip(expected)
            .fold(0_u8, |difference, (left, right)| {
                difference | (left ^ right)
            })
            != 0
    {
        return Err(TicketError::BadSignature);
    }

    let row = transaction
        .query_row(
            "SELECT player_id, scope, expires_at FROM player_web_tickets
             WHERE nonce=?1 AND used_at IS NULL AND expires_at > ?2",
            params![nonce, now],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .optional()
        .map_err(DatabaseError::from_sqlite)?;
    let Some((player, scope, expires_at)) = row else {
        // 已使用、已过期或不存在的票据对外表现得完全一致，避免探测。
        return Err(TicketError::Unavailable);
    };
    let used = transaction
        .execute(
            "UPDATE player_web_tickets SET used_at=?2
             WHERE nonce=?1 AND used_at IS NULL",
            params![nonce, now],
        )
        .map_err(DatabaseError::from_sqlite)?;
    if used == 0 {
        return Err(TicketError::AlreadyUsed);
    }
    let _ = expires_at;

    Ok(ExchangeCredential {
        platform_user_id: u64::try_from(player).map_err(|_| TicketError::CorruptPlayer)?,
        scope,
    })
}

fn hmac_signature(key: &[u8; 32], nonce: &str) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(key).expect("hmac accepts any key length");
    mac.update(TICKET_DOMAIN.as_bytes());
    mac.update(nonce.as_bytes());
    mac.finalize().into_bytes().into()
}

#[cfg(test)]
mod tests;
