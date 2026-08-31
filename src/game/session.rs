use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::Path,
};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use data_encoding::BASE32_NOPAD;
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::{config::GameConfig, identity};

use super::GameError;

const SESSION_VERSION: u8 = 1;
const SESSION_LIFETIME_SECONDS: u64 = 2 * 60 * 60;
const SESSION_KEY_BYTES: usize = 32;
const SESSION_TAG_BYTES: usize = 16;

type HmacSha256 = Hmac<Sha256>;

pub fn issue_ascii_fpv_url(
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
    let expires_at =
        crate::database::unix_timestamp().saturating_add(SESSION_LIFETIME_SECONDS as i64);
    let expires_at = u32::try_from(expires_at).map_err(|_| GameError::InvalidSessionKey)?;
    let mut nonce = [0_u8; 8];
    getrandom::fill(&mut nonce).map_err(|error| GameError::Random(error.to_string()))?;

    let mut payload = Vec::with_capacity(21);
    payload.push(SESSION_VERSION);
    payload.extend_from_slice(&user_id.to_be_bytes());
    payload.extend_from_slice(&expires_at.to_be_bytes());
    payload.extend_from_slice(&nonce);

    let mut mac = HmacSha256::new_from_slice(&key).map_err(|_| GameError::InvalidSessionKey)?;
    mac.update(&payload);
    payload.extend_from_slice(&mac.finalize().into_bytes()[..SESSION_TAG_BYTES]);

    let ticket = BASE32_NOPAD.encode(&payload).to_ascii_lowercase();
    Ok(format!(
        "https://{ticket}.{}",
        config.ascii_fpv_domain.trim().to_ascii_lowercase()
    ))
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
