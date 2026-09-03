pub mod activity;
pub mod admin;
pub mod combat;
pub mod cultivation;
pub mod daily_state;
pub mod destiny;
pub mod error;
pub mod game_reward;
pub mod game_session;
pub mod group;
pub mod inventory;
pub mod player;
pub mod skills;
pub mod wallet;
pub mod world_event;

mod connection;
pub(crate) mod migrations;

pub use connection::Database;
pub use error::{DatabaseError, DatabaseResult};

pub(crate) fn player_id(value: u64) -> DatabaseResult<i64> {
    i64::try_from(value).map_err(|_| DatabaseError::InvalidIdentifier)
}

pub(crate) fn unix_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
