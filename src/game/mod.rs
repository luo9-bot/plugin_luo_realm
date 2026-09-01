mod session;
mod voucher;

pub use session::issue_ascii_fpv_url;
pub use voucher::{VerifiedVoucher, verify_reward_voucher};

pub const ASCII_FPV_GAME_ID: &str = "ascii-fpv";
pub const MIN_REWARD_COINS: i64 = 1;
pub const MAX_REWARD_COINS: i64 = 60;

#[derive(Debug, thiserror::Error)]
pub enum GameError {
    #[error("game feature is not configured")]
    NotConfigured,
    #[error("game session storage failed: {0}")]
    Storage(#[source] std::io::Error),
    #[error("secure random generation failed: {0}")]
    Random(String),
    #[error("game session key is invalid")]
    InvalidSessionKey,
    #[error("cloud game session activation failed: {0}")]
    Activation(String),
    #[error("game session creation is rate limited")]
    RateLimited,
    #[error("reward voucher is invalid")]
    InvalidVoucher,
    #[error("reward voucher has expired")]
    ExpiredVoucher,
    #[error("reward voucher belongs to another player")]
    WrongPlayer,
}
