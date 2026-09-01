use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::Path,
    time::Duration,
};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use data_encoding::BASE32_NOPAD;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::{
    config::GameConfig,
    database::{self, Database, DatabaseError},
    identity,
};

use super::GameError;

const SESSION_VERSION: u8 = 2;
const SESSION_KEY_BYTES: usize = 32;
const SESSION_TAG_BYTES: usize = 16;
const ACTIVATION_TIMEOUT: Duration = Duration::from_secs(2);
const ISSUANCE_COOLDOWN_SECONDS: i64 = 15;

type HmacSha256 = Hmac<Sha256>;

#[derive(Serialize)]
struct ActivationRequest<'a> {
    session: &'a str,
}

#[derive(Deserialize)]
struct ActivationResponse {
    ok: bool,
    error: Option<String>,
}

pub fn issue_ascii_fpv_url(
    database: &mut Database,
    plugin_root: &Path,
    user_id: u64,
    config: &GameConfig,
) -> Result<String, GameError> {
    if !config.ascii_fpv_enabled {
        return Err(GameError::NotConfigured);
    }

    let key_path = plugin_root
        .join(identity::DATA_DIRECTORY)
        .join("ascii_fpv.session.key");
    let key = load_or_create_key(&key_path)?;
    let issued_at = reserve_issued_at(database, user_id)?;
    let ticket = create_ticket(user_id, issued_at, &key)?;
    activate_session(&ticket, config)?;

    Ok(format!(
        "https://{ticket}.{}",
        config.ascii_fpv_domain.trim().to_ascii_lowercase()
    ))
}

fn reserve_issued_at(database: &mut Database, user_id: u64) -> Result<u32, GameError> {
    let current_time = database::unix_timestamp();
    let transaction = database.immediate_transaction().map_err(database_error)?;
    let issued_at = database::game_session::reserve_issued_at(
        &transaction,
        user_id,
        current_time,
        ISSUANCE_COOLDOWN_SECONDS,
    )
    .map_err(database_error)?
    .ok_or(GameError::RateLimited)?;
    transaction
        .commit()
        .map_err(DatabaseError::from_sqlite)
        .map_err(database_error)?;
    Ok(issued_at)
}

fn create_ticket(
    user_id: u64,
    issued_at: u32,
    key: &[u8; SESSION_KEY_BYTES],
) -> Result<String, GameError> {
    let mut nonce = [0_u8; 8];
    getrandom::fill(&mut nonce).map_err(|error| GameError::Random(error.to_string()))?;

    let mut payload = Vec::with_capacity(21 + SESSION_TAG_BYTES);
    payload.push(SESSION_VERSION);
    payload.extend_from_slice(&user_id.to_be_bytes());
    payload.extend_from_slice(&issued_at.to_be_bytes());
    payload.extend_from_slice(&nonce);

    let mut mac = HmacSha256::new_from_slice(key).map_err(|_| GameError::InvalidSessionKey)?;
    mac.update(&payload);
    payload.extend_from_slice(&mac.finalize().into_bytes()[..SESSION_TAG_BYTES]);
    Ok(BASE32_NOPAD.encode(&payload).to_ascii_lowercase())
}

fn activate_session(ticket: &str, config: &GameConfig) -> Result<(), GameError> {
    let url = format!(
        "https://{}/api/session/activate",
        config.ascii_fpv_domain.trim().to_ascii_lowercase()
    );
    let response = ureq::post(&url)
        .config()
        .timeout_global(Some(ACTIVATION_TIMEOUT))
        .build()
        .send_json(&ActivationRequest { session: ticket })
        .map_err(|error| GameError::Activation(error.to_string()))?;
    let status = response.status();
    let body = response
        .into_body()
        .read_json::<ActivationResponse>()
        .map_err(|error| GameError::Activation(error.to_string()))?;
    if status.is_success() && body.ok {
        Ok(())
    } else {
        Err(GameError::Activation(
            body.error.unwrap_or_else(|| format!("HTTP {status}")),
        ))
    }
}

fn database_error(error: DatabaseError) -> GameError {
    GameError::Activation(error.to_string())
}

fn load_or_create_key(path: &Path) -> Result<[u8; SESSION_KEY_BYTES], GameError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(GameError::Storage)?;
    }
    match fs::read_to_string(path) {
        Ok(value) => decode_key(value.trim()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => create_key(path),
        Err(error) => Err(GameError::Storage(error)),
    }
}

fn create_key(path: &Path) -> Result<[u8; SESSION_KEY_BYTES], GameError> {
    let mut key = [0_u8; SESSION_KEY_BYTES];
    getrandom::fill(&mut key).map_err(|error| GameError::Random(error.to_string()))?;
    let encoded = URL_SAFE_NO_PAD.encode(key);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(path) {
        Ok(mut file) => {
            file.write_all(encoded.as_bytes())
                .and_then(|()| file.sync_all())
                .map_err(GameError::Storage)?;
            Ok(key)
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => fs::read_to_string(path)
            .map_err(GameError::Storage)
            .and_then(|value| decode_key(value.trim())),
        Err(error) => Err(GameError::Storage(error)),
    }
}

fn decode_key(value: &str) -> Result<[u8; SESSION_KEY_BYTES], GameError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(value.as_bytes())
        .map_err(|_| GameError::InvalidSessionKey)?;
    bytes.try_into().map_err(|_| GameError::InvalidSessionKey)
}
