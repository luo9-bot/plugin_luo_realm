//! Cloudflare 推送通道：档案快照出站同步与页面会话令牌。
//!
//! 架构（设计方案书 20.3 的 Cloudflare 形态）：
//! - 插件**只发起出站 HTTPS**，把快照推送到 Worker（`sync_url`），源站地址
//!   永不暴露；推送凭据是高熵 `sync_token`。
//! - 页面持有 `page_token`（256 位随机、数据库可校验、有期限）读取 CF 上
//!   的快照；写操作由 Worker 以环境变量中隐藏的源站地址转发回插件。
//!
//! 鉴权铁律：`sync_token` 与 `page_token` 一律常量时间比较；令牌校验必须
//! 同时检查存在性与有效期；SQL 全部预编译。

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::Serialize;

use crate::config::PlayerWebConfig;
use crate::database::{Database, DatabaseError, DatabaseResult, player_id, unix_timestamp};
use crate::domain::error_code::StableErrorCode;
use crate::domain::shared::PlatformUserId;

const SESSION_TTL_SECONDS: i64 = 7_200;
const TOKEN_BYTES: usize = 32;
const PUSH_TIMEOUT_SECS: u64 = 5;

/// 页面会话：推送时签发，读写两侧共用。
#[derive(Clone, Debug)]
pub struct PageSession {
    pub platform_user_id: u64,
    pub expires_at: i64,
    pub token: String,
}

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("sync is not configured")]
    NotConfigured,
    #[error("push rejected by worker: {0}")]
    Rejected(String),
    #[error("push failed: {0}")]
    Transport(String),
    #[error(transparent)]
    Database(#[from] DatabaseError),
}

impl StableErrorCode for SyncError {
    fn error_code(&self) -> &'static str {
        match self {
            Self::NotConfigured => "player_web.sync_not_configured",
            Self::Rejected(_) => "player_web.sync_rejected",
            Self::Transport(_) => "player_web.sync_transport",
            Self::Database(error) => error.error_code(),
        }
    }
}

/// 常量时间字节比较：长度不同立即拒绝（长度本身不构成可利用信息），
/// 等长时按位异或折叠，避免逐字符短路泄漏前缀。
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0_u8, |difference, (left, right)| {
        difference | (left ^ right)
    }) == 0
}

/// 严格校验同步密钥：先做常规短路，再走常量时间比较。
pub fn verify_sync_token(config: &PlayerWebConfig, candidate: &str) -> bool {
    let expected = config.sync_token.trim();
    !expected.is_empty() && constant_time_eq(candidate.as_bytes(), expected.as_bytes())
}

/// 查询并校验页面会话：存在且未过期才有效。
pub fn verify_page_session(
    transaction: &Transaction<'_>,
    token: &str,
    now: i64,
) -> DatabaseResult<Option<PageSession>> {
    let row = transaction
        .query_row(
            "SELECT player_id, expires_at FROM player_page_sessions
             WHERE token=?1 AND expires_at > ?2",
            params![token, now],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()
        .map_err(DatabaseError::from_sqlite)?;
    let Some((player, expires_at)) = row else {
        return Ok(None);
    };
    let platform_user_id = u64::try_from(player).map_err(|_| DatabaseError::InvalidIdentifier)?;
    Ok(Some(PageSession {
        platform_user_id,
        expires_at,
        token: token.to_owned(),
    }))
}

/// 推送所需的完整快照载荷。
#[derive(Serialize)]
struct SnapshotPayload<'a> {
    token: &'a str,
    expires_at: i64,
    player_id: u64,
    state: StateBody<'a>,
}

#[derive(Serialize)]
struct StateBody<'a> {
    profile: &'a crate::player_web::views::ProfileView,
    wallet: &'a crate::player_web::views::WalletView,
    skills: &'a crate::player_web::views::SkillsView,
    equipment: &'a crate::player_web::views::EquipmentView,
    battles: &'a crate::player_web::views::BattlesView,
}

/// 生成页面会话令牌并把快照推送到 Cloudflare。
///
/// 数据库写入（会话与快照组装）在短事务内完成；出站 HTTP 严格位于事务
/// 提交之后，网络失败不污染本地状态，孤儿令牌由过期清理兜底。
pub fn push_snapshot(
    database: &mut Database,
    config: &PlayerWebConfig,
    user_id: u64,
) -> Result<PageSession, SyncError> {
    if !config.sync_enabled() {
        return Err(SyncError::NotConfigured);
    }
    let now = unix_timestamp();
    let expires_at = now + SESSION_TTL_SECONDS;
    let url = config.sync_url.trim().to_owned();
    let token_secret = config.sync_token.trim().to_owned();

    // 作用域把视图借用与出站请求圈在一起：事务在请求之前提交，
    // 网络失败只影响推送结果，不污染本地状态。
    let issued = {
        let transaction = database.immediate_transaction()?;
        let platform_user_id = PlatformUserId::new(user_id);
        // views::profile 内部查询 active 玩家：未注册或停用在此处统一拒绝。
        let profile = crate::player_web::views::profile(
            &transaction,
            platform_user_id,
            &now_date(&transaction)?,
        )?;
        let wallet = crate::player_web::views::wallet(&transaction, platform_user_id)?;
        let skills = crate::player_web::views::skills_view(&transaction, platform_user_id)?;
        let equipment = crate::player_web::views::equipment(&transaction, platform_user_id)?;
        let battles = crate::player_web::views::battles(&transaction, platform_user_id)?;

        transaction
            .execute(
                "DELETE FROM player_page_sessions WHERE expires_at < ?1",
                [now],
            )
            .map_err(DatabaseError::from_sqlite)?;
        let mut token_bytes = [0_u8; TOKEN_BYTES];
        getrandom::fill(&mut token_bytes)
            .map_err(|error| DatabaseError::InvalidData(error.to_string()))?;
        let token = URL_SAFE_NO_PAD.encode(token_bytes);
        let row = player_id(user_id)?;
        transaction
            .execute(
                "INSERT INTO player_page_sessions(token, player_id, scope, created_at, expires_at)
                 VALUES(?1, ?2, 'profile:read', ?3, ?4)",
                params![token, row, now, expires_at],
            )
            .map_err(DatabaseError::from_sqlite)?;
        transaction.commit().map_err(DatabaseError::from_sqlite)?;

        let payload = SnapshotPayload {
            token: &token,
            expires_at,
            player_id: platform_user_id.value(),
            state: StateBody {
                profile: &profile,
                wallet: &wallet,
                skills: &skills,
                equipment: &equipment,
                battles: &battles,
            },
        };
        let response = ureq::post(&url)
            .header("Authorization", &format!("Bearer {token_secret}"))
            .config()
            .timeout_global(Some(std::time::Duration::from_secs(PUSH_TIMEOUT_SECS)))
            .build()
            .send_json(&payload)
            .map_err(|error| SyncError::Transport(error.to_string()))?;
        if !response.status().is_success() {
            return Err(SyncError::Rejected(format!("HTTP {}", response.status())));
        }
        token
    };

    Ok(PageSession {
        platform_user_id: user_id,
        expires_at,
        token: issued,
    })
}

fn now_date(transaction: &Transaction<'_>) -> DatabaseResult<String> {
    transaction
        .query_row("SELECT date('now', 'localtime')", [], |row| row.get(0))
        .map_err(DatabaseError::from_sqlite)
}

#[cfg(test)]
mod tests;
