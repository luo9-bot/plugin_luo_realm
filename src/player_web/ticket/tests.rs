//! 一次性票据的单元测试。

use super::{ExchangeCredential, TicketError, exchange, issue};
use crate::database::migrations;
use crate::player_web::session::SCOPE_PROFILE_READ;
use rusqlite::Connection;

const KEY: [u8; 32] = [9_u8; 32];
const NOW: i64 = 1_000_000;
const TTL: i64 = 600;

fn memory_database() -> Connection {
    let mut connection = Connection::open_in_memory().expect("open in-memory database");
    migrations::apply(&mut connection).expect("apply migrations");
    connection
        .execute(
            "INSERT INTO players(player_id, created_at, updated_at) VALUES(10001, 0, 0)",
            [],
        )
        .expect("insert player");
    connection
}

fn exchange_of(
    connection: &mut Connection,
    token: &str,
) -> Result<ExchangeCredential, TicketError> {
    let transaction = connection.transaction().expect("begin transaction");
    let credential = exchange(&transaction, token, &KEY, NOW + 30);
    transaction.commit().expect("commit");
    credential
}

#[test]
fn issued_ticket_exchanges_exactly_once() {
    let mut connection = memory_database();
    let issue_transaction = connection.transaction().expect("begin transaction");
    let ticket = issue(
        &issue_transaction,
        10001,
        SCOPE_PROFILE_READ,
        &KEY,
        NOW,
        TTL,
    )
    .expect("issue");
    issue_transaction.commit().expect("commit");

    let first = exchange_of(&mut connection, &ticket.token).expect("first exchange");
    assert_eq!(first.platform_user_id, 10001);
    assert_eq!(first.scope, SCOPE_PROFILE_READ);
    assert!(matches!(
        exchange_of(&mut connection, &ticket.token),
        Err(TicketError::Unavailable)
    ));
}

#[test]
fn unsigned_unknown_and_expired_tickets_are_rejected() {
    let mut connection = memory_database();
    let issue_transaction = connection.transaction().expect("begin transaction");
    let ticket = issue(
        &issue_transaction,
        10001,
        SCOPE_PROFILE_READ,
        &KEY,
        NOW,
        TTL,
    )
    .expect("issue");
    let expired = issue(
        &issue_transaction,
        10001,
        SCOPE_PROFILE_READ,
        &KEY,
        NOW - TTL - 1,
        TTL,
    )
    .expect("expired issue");
    issue_transaction.commit().expect("commit");

    let (nonce, _) = ticket.token.split_once('.').expect("split");
    assert!(matches!(
        exchange_of(&mut connection, &format!("{nonce}.bm90LXRoZS1zaWduYXR1cmU")),
        Err(TicketError::BadSignature)
    ));
    assert!(matches!(
        exchange_of(&mut connection, "bm9uY2U.bm90LXRoZS1zaWduYXR1cmU"),
        Err(TicketError::BadSignature)
    ));
    assert!(matches!(
        exchange_of(&mut connection, "missing"),
        Err(TicketError::Malformed)
    ));

    let expired_transaction = connection.transaction().expect("begin transaction");
    assert!(matches!(
        exchange(&expired_transaction, &expired.token, &KEY, NOW),
        Err(TicketError::Unavailable)
    ));
    expired_transaction.commit().expect("commit");
}

#[test]
fn expired_tickets_are_cleaned_up_on_issue() {
    let mut connection = memory_database();
    let issue_transaction = connection.transaction().expect("begin transaction");
    let _ = issue(
        &issue_transaction,
        10001,
        SCOPE_PROFILE_READ,
        &KEY,
        NOW - TTL - 1,
        TTL,
    )
    .expect("expired issue");
    let _ = issue(
        &issue_transaction,
        10001,
        SCOPE_PROFILE_READ,
        &KEY,
        NOW,
        TTL,
    )
    .expect("fresh issue");
    issue_transaction.commit().expect("commit");

    let remaining: i64 = connection
        .query_row("SELECT COUNT(*) FROM player_web_tickets", [], |row| {
            row.get(0)
        })
        .expect("ticket count");
    assert_eq!(remaining, 1, "签发新票据时清理过期记录");
}
