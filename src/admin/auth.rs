use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::RwLock,
};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};

const TOKEN_BYTES: usize = 32;
const MIN_TOKEN_LENGTH: usize = 32;

pub struct AdminToken {
    digest: RwLock<[u8; 32]>,
}

impl AdminToken {
    pub fn load_or_create(path: &Path) -> Result<Self, AuthError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(AuthError::Io)?;
        }
        recover_backup(path)?;
        let token = match fs::read_to_string(path) {
            Ok(value) => value,
            Err(error) if error.kind() == io::ErrorKind::NotFound => create_token(path)?,
            Err(error) => return Err(AuthError::Io(error)),
        };
        let token = token.trim();
        if token.len() < MIN_TOKEN_LENGTH {
            return Err(AuthError::InvalidToken);
        }

        Ok(Self {
            digest: RwLock::new(digest(token)),
        })
    }

    pub fn verify(&self, candidate: &str) -> bool {
        let expected = self
            .digest
            .read()
            .unwrap_or_else(|error| error.into_inner());
        let candidate = digest(candidate);
        let difference = expected
            .iter()
            .zip(candidate)
            .fold(0_u8, |difference, (left, right)| {
                difference | (left ^ right)
            });
        difference == 0
    }

    /// 玩家网页票据与会话的 HMAC 签名密钥。
    ///
    /// 复用管理 Token 的摘要作为根密钥并做域分离：轮换 Token 会立即失效
    /// 所有未使用的票据与网页会话，这是预期的安全行为。
    pub fn signing_key(&self) -> [u8; 32] {
        *self
            .digest
            .read()
            .unwrap_or_else(|error| error.into_inner())
    }

    pub fn rotate(&self, replacement: &str, path: &Path) -> Result<(), AuthError> {
        let replacement = replacement.trim();
        if replacement.len() < MIN_TOKEN_LENGTH {
            return Err(AuthError::InvalidToken);
        }
        replace_token(path, replacement)?;
        *self
            .digest
            .write()
            .unwrap_or_else(|error| error.into_inner()) = digest(replacement);
        Ok(())
    }
}

fn create_token(path: &Path) -> Result<String, AuthError> {
    let mut bytes = [0_u8; TOKEN_BYTES];
    getrandom::fill(&mut bytes).map_err(|error| AuthError::Random(error.to_string()))?;
    let token = URL_SAFE_NO_PAD.encode(bytes);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(path) {
        Ok(mut file) => {
            file.write_all(token.as_bytes()).map_err(AuthError::Io)?;
            file.sync_all().map_err(AuthError::Io)?;
            Ok(token)
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            fs::read_to_string(path).map_err(AuthError::Io)
        }
        Err(error) => Err(AuthError::Io(error)),
    }
}

fn replace_token(path: &Path, token: &str) -> Result<(), AuthError> {
    let temporary = sibling(path, "new");
    let backup = sibling(path, "bak");
    if temporary.exists() {
        fs::remove_file(&temporary).map_err(AuthError::Io)?;
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary).map_err(AuthError::Io)?;
    file.write_all(token.as_bytes()).map_err(AuthError::Io)?;
    file.sync_all().map_err(AuthError::Io)?;
    drop(file);

    if backup.exists() {
        fs::remove_file(&backup).map_err(AuthError::Io)?;
    }
    fs::rename(path, &backup).map_err(AuthError::Io)?;
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::rename(&backup, path);
        return Err(AuthError::Io(error));
    }
    fs::remove_file(backup).map_err(AuthError::Io)
}

fn recover_backup(path: &Path) -> Result<(), AuthError> {
    let backup = sibling(path, "bak");
    let temporary = sibling(path, "new");
    if !path.exists() && backup.exists() {
        fs::rename(&backup, path).map_err(AuthError::Io)?;
    } else if path.exists() && backup.exists() {
        fs::remove_file(backup).map_err(AuthError::Io)?;
    }
    if temporary.exists() {
        fs::remove_file(temporary).map_err(AuthError::Io)?;
    }
    Ok(())
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    path.with_extension(format!(
        "{}.{}",
        path.extension()
            .and_then(|value| value.to_str())
            .unwrap_or("token"),
        suffix
    ))
}

fn digest(value: &str) -> [u8; 32] {
    Sha256::digest(value.as_bytes()).into()
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("admin token I/O failed: {0}")]
    Io(#[source] io::Error),
    #[error("admin token must contain at least 32 characters")]
    InvalidToken,
    #[error("secure random generation failed: {0}")]
    Random(String),
}
