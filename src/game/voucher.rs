use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::Deserialize;

use super::{ASCII_FPV_GAME_ID, GameError, MAX_REWARD_COINS, MIN_REWARD_COINS};

const MAX_VOUCHER_LENGTH: usize = 2_048;
const MAX_VOUCHER_LIFETIME_SECONDS: i64 = 24 * 60 * 60;
const CLOCK_SKEW_SECONDS: i64 = 5 * 60;

#[derive(Debug)]
pub struct VerifiedVoucher {
    pub nonce: String,
    pub player_id: u64,
    pub game_id: String,
    pub score: u32,
    pub reward: i64,
    pub issued_at: i64,
}

#[derive(Deserialize)]
struct VoucherHeader {
    alg: String,
    typ: String,
    kid: String,
}

#[derive(Deserialize)]
struct VoucherPayload {
    version: u8,
    game_id: String,
    player_id: String,
    nonce: String,
    score: u32,
    reward: i64,
    issued_at: i64,
    expires_at: i64,
}

pub fn verify_reward_voucher(
    code: &str,
    public_key: &str,
    expected_player_id: u64,
    now: i64,
) -> Result<VerifiedVoucher, GameError> {
    let code = code.trim();
    if code.is_empty() || code.len() > MAX_VOUCHER_LENGTH {
        return Err(GameError::InvalidVoucher);
    }
    let segments = code.split('.').collect::<Vec<_>>();
    let [header_segment, payload_segment, signature_segment] = segments.as_slice() else {
        return Err(GameError::InvalidVoucher);
    };
    let header: VoucherHeader = decode_json(header_segment)?;
    if header.alg != "EdDSA" || header.typ != "LRV" || header.kid != "ascii-fpv-1" {
        return Err(GameError::InvalidVoucher);
    }

    let payload: VoucherPayload = decode_json(payload_segment)?;
    let verifying_key = decode_public_key(public_key)?;
    let signature = URL_SAFE_NO_PAD
        .decode(signature_segment.as_bytes())
        .map_err(|_| GameError::InvalidVoucher)?;
    let signature = Signature::from_slice(&signature).map_err(|_| GameError::InvalidVoucher)?;
    let signing_input = format!("{header_segment}.{payload_segment}");
    verifying_key
        .verify_strict(signing_input.as_bytes(), &signature)
        .map_err(|_| GameError::InvalidVoucher)?;
    validate_payload(&payload, expected_player_id, now)?;

    Ok(VerifiedVoucher {
        nonce: payload.nonce,
        player_id: expected_player_id,
        game_id: payload.game_id,
        score: payload.score,
        reward: payload.reward,
        issued_at: payload.issued_at,
    })
}

fn validate_payload(
    payload: &VoucherPayload,
    expected_player_id: u64,
    now: i64,
) -> Result<(), GameError> {
    let player_id = payload
        .player_id
        .parse::<u64>()
        .map_err(|_| GameError::InvalidVoucher)?;
    if player_id != expected_player_id {
        return Err(GameError::WrongPlayer);
    }
    if payload.version != 1
        || payload.game_id != ASCII_FPV_GAME_ID
        || payload.score > 100_000
        || !(MIN_REWARD_COINS..=MAX_REWARD_COINS).contains(&payload.reward)
        || payload.nonce.len() != 16
        || !payload
            .nonce
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        || payload.issued_at > now.saturating_add(CLOCK_SKEW_SECONDS)
        || payload.expires_at < payload.issued_at
        || payload.expires_at.saturating_sub(payload.issued_at) > MAX_VOUCHER_LIFETIME_SECONDS
    {
        return Err(GameError::InvalidVoucher);
    }
    if payload.expires_at < now {
        return Err(GameError::ExpiredVoucher);
    }
    Ok(())
}

fn decode_json<T: for<'de> Deserialize<'de>>(segment: &str) -> Result<T, GameError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(segment.as_bytes())
        .map_err(|_| GameError::InvalidVoucher)?;
    serde_json::from_slice(&bytes).map_err(|_| GameError::InvalidVoucher)
}

fn decode_public_key(value: &str) -> Result<VerifyingKey, GameError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(value.trim().as_bytes())
        .map_err(|_| GameError::NotConfigured)?;
    let key: [u8; 32] = bytes.try_into().map_err(|_| GameError::NotConfigured)?;
    VerifyingKey::from_bytes(&key).map_err(|_| GameError::NotConfigured)
}
