//! 会话令牌的单元测试。

use super::{SCOPE_PROFILE_READ, SessionError, mint, verify};

const KEY: [u8; 32] = [7_u8; 32];

#[test]
fn minted_session_verifies_roundtrip() {
    let token = mint(&KEY, 10001, SCOPE_PROFILE_READ, 1_000, 600).expect("mint");

    let session = verify(&KEY, &token, 1_200).expect("verify");
    assert_eq!(session.platform_user_id, 10001);
    assert_eq!(session.scope, SCOPE_PROFILE_READ);
    assert_eq!(session.expires_at, 1_600);
}

#[test]
fn expired_tampered_and_unknown_scope_are_rejected() {
    let token = mint(&KEY, 10001, SCOPE_PROFILE_READ, 1_000, 600).expect("mint");

    assert_eq!(
        verify(&KEY, &token, 1_601).expect_err("expired"),
        SessionError::Expired
    );
    let mut tampered = token.clone();
    tampered.replace_range(3..4, if &token[3..4] == "A" { "B" } else { "A" });
    assert!(matches!(
        verify(&KEY, &tampered, 1_200),
        Err(SessionError::Malformed) | Err(SessionError::BadSignature)
    ));
    assert_eq!(
        verify(&[8_u8; 32], &token, 1_200).expect_err("wrong key"),
        SessionError::BadSignature
    );

    assert!(matches!(
        mint(&KEY, 10001, "profile:write", 1_000, 600),
        Err(SessionError::UnknownScope)
    ));
}

#[test]
fn malformed_tokens_never_verify() {
    for token in ["", "v1", "v1.a", "v2.a.b", "v1.!!!.bbb"] {
        assert!(
            verify(&KEY, token, 1_200).is_err(),
            "token {token:?} 必须被拒绝"
        );
    }
}
