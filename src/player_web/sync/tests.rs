//! 同步与鉴权原语的验证。

use super::{constant_time_eq, verify_page_session, verify_sync_token};
use crate::config::PlayerWebConfig;
use crate::database::migrations;
use rusqlite::Connection;

const NOW: i64 = 1_000_000;

#[test]
fn constant_time_eq_matches_exact_bytes_only() {
    assert!(constant_time_eq(b"alpha", b"alpha"));
    assert!(!constant_time_eq(b"alpha", b"alphb"));
    assert!(!constant_time_eq(b"alpha", b"alph"));
    assert!(!constant_time_eq(b"", b"a"));
    assert!(constant_time_eq(b"", b""));
}

#[test]
fn sync_token_verification_requires_configuration() {
    let config = PlayerWebConfig {
        sync_url: "https://worker.example".into(),
        sync_token: "0123456789abcdef0123456789abcdef".into(),
        ..PlayerWebConfig::default()
    };
    assert!(verify_sync_token(
        &config,
        "0123456789abcdef0123456789abcdef"
    ));
    assert!(!verify_sync_token(
        &config,
        "0123456789abcdef0123456789abcdeX"
    ));
    assert!(!verify_sync_token(&config, "short"));

    let unconfigured = PlayerWebConfig::default();
    assert!(!verify_sync_token(&unconfigured, ""));
}

#[test]
fn page_session_expiry_is_enforced() {
    let mut connection = Connection::open_in_memory().expect("open in-memory database");
    migrations::apply(&mut connection).expect("apply migrations");
    connection
        .execute(
            "INSERT INTO players(player_id, created_at, updated_at) VALUES(10001, 0, 0)",
            [],
        )
        .expect("insert player");
    connection
        .execute(
            "INSERT INTO player_page_sessions(token, player_id, scope, created_at, expires_at)
             VALUES('tok-live', 10001, 'profile:read', 0, ?1),
                   ('tok-dead', 10001, 'profile:read', 0, 500)",
            [NOW + 600],
        )
        .expect("insert sessions");

    let transaction = connection.transaction().expect("begin transaction");
    let live = verify_page_session(&transaction, "tok-live", NOW).expect("live lookup");
    let dead = verify_page_session(&transaction, "tok-dead", NOW).expect("dead lookup");
    transaction.commit().expect("commit");

    assert_eq!(live.expect("live session").platform_user_id, 10001);
    assert!(dead.is_none(), "过期会话必须被拒绝");
}
