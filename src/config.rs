use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    sync::{Arc, RwLock},
};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};

use crate::identity;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct RuntimeConfig {
    pub schema_version: u32,
    pub version_salt: String,
    pub command: CommandConfig,
    pub gameplay: GameplayConfig,
    pub game: GameConfig,
    pub admin: AdminConfig,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            schema_version: 1,
            version_salt: "luo-realm-v1".into(),
            command: CommandConfig::default(),
            gameplay: GameplayConfig::default(),
            game: GameConfig::default(),
            admin: AdminConfig::default(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct GameplayConfig {
    pub battle_report_enabled: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct GameConfig {
    pub ascii_fpv_enabled: bool,
    pub ascii_fpv_domain: String,
    pub reward_public_key: String,
    pub daily_redemption_limit: u32,
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            ascii_fpv_enabled: false,
            ascii_fpv_domain: "ascii-fpv.luo-realm.drluo.top".into(),
            reward_public_key: String::new(),
            daily_redemption_limit: 3,
        }
    }
}

impl RuntimeConfig {
    pub fn load(plugin_root: &Path) -> Result<Self, ConfigError> {
        let path = plugin_root.join("config").join("config.toml");
        recover_file(&path)?;
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&path).map_err(|source| ConfigError::Read {
            path: path.clone(),
            source,
        })?;
        let config: Self = toml::from_str(&content).map_err(|source| ConfigError::Parse {
            path: path.clone(),
            source: Box::new(source),
        })?;
        config.validate()?;
        Ok(config)
    }

    pub fn save(&self, plugin_root: &Path) -> Result<(), ConfigError> {
        self.validate()?;
        let path = plugin_root.join("config").join("config.toml");
        let temporary = path.with_extension("toml.new");
        let content = toml::to_string_pretty(self)
            .map_err(|error| ConfigError::Serialize(Box::new(error)))?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| ConfigError::Write {
                path: path.clone(),
                source,
            })?;
        }
        replace_file(&path, &temporary, content.as_bytes())
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.command.prefix.trim().is_empty() || self.command.prefix.chars().count() > 32 {
            return Err(ConfigError::InvalidAdmin(
                "command prefix must contain 1 to 32 characters".into(),
            ));
        }
        if self.admin.bind.trim().is_empty() {
            return Err(ConfigError::InvalidAdmin("bind cannot be empty".into()));
        }
        if !(1..=65_526).contains(&self.admin.port) {
            return Err(ConfigError::InvalidAdmin(
                "port must be between 1 and 65526".into(),
            ));
        }
        if self.admin.admin_ids.contains(&0) {
            return Err(ConfigError::InvalidAdmin(
                "administrator IDs must be positive".into(),
            ));
        }
        if !valid_domain(&self.game.ascii_fpv_domain) {
            return Err(ConfigError::InvalidGame(
                "ASCII FPV domain must be a valid hostname without a scheme".into(),
            ));
        }
        if !(1..=10).contains(&self.game.daily_redemption_limit) {
            return Err(ConfigError::InvalidGame(
                "daily redemption limit must be between 1 and 10".into(),
            ));
        }
        if !self.game.reward_public_key.trim().is_empty() {
            let key = URL_SAFE_NO_PAD
                .decode(self.game.reward_public_key.trim().as_bytes())
                .map_err(|_| ConfigError::InvalidGame("reward public key is invalid".into()))?;
            if key.len() != 32 {
                return Err(ConfigError::InvalidGame(
                    "reward public key must contain 32 bytes".into(),
                ));
            }
        }
        let unique = self
            .admin
            .admin_ids
            .iter()
            .collect::<std::collections::HashSet<_>>();
        if unique.len() != self.admin.admin_ids.len() {
            return Err(ConfigError::InvalidAdmin(
                "administrator IDs must not be duplicated".into(),
            ));
        }
        Ok(())
    }
}

fn valid_domain(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty()
        && value.len() <= 253
        && !value.contains("://")
        && value.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

fn replace_file(path: &Path, temporary: &Path, content: &[u8]) -> Result<(), ConfigError> {
    let backup = path.with_extension("toml.bak");
    if temporary.exists() {
        fs::remove_file(temporary).map_err(|source| ConfigError::Write {
            path: temporary.to_path_buf(),
            source,
        })?;
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temporary)
        .map_err(|source| ConfigError::Write {
            path: temporary.to_path_buf(),
            source,
        })?;
    file.write_all(content)
        .and_then(|()| file.sync_all())
        .map_err(|source| ConfigError::Write {
            path: temporary.to_path_buf(),
            source,
        })?;
    drop(file);

    if backup.exists() {
        fs::remove_file(&backup).map_err(|source| ConfigError::Write {
            path: backup.clone(),
            source,
        })?;
    }
    if path.exists() {
        fs::rename(path, &backup).map_err(|source| ConfigError::Write {
            path: path.to_path_buf(),
            source,
        })?;
    }
    if let Err(source) = fs::rename(temporary, path) {
        let _ = fs::rename(&backup, path);
        return Err(ConfigError::Write {
            path: path.to_path_buf(),
            source,
        });
    }
    if backup.exists() {
        fs::remove_file(&backup).map_err(|source| ConfigError::Write {
            path: backup,
            source,
        })?;
    }
    Ok(())
}

fn recover_file(path: &Path) -> Result<(), ConfigError> {
    let temporary = path.with_extension("toml.new");
    let backup = path.with_extension("toml.bak");
    if !path.exists() && backup.exists() {
        fs::rename(&backup, path).map_err(|source| ConfigError::Write {
            path: path.to_path_buf(),
            source,
        })?;
    } else if path.exists() && backup.exists() {
        fs::remove_file(&backup).map_err(|source| ConfigError::Write {
            path: backup,
            source,
        })?;
    }
    if temporary.exists() {
        fs::remove_file(&temporary).map_err(|source| ConfigError::Write {
            path: temporary,
            source,
        })?;
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct AdminConfig {
    pub enabled: bool,
    pub bind: String,
    pub port: u16,
    pub admin_ids: Vec<u64>,
}

impl Default for AdminConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bind: "0.0.0.0".into(),
            port: 18_765,
            admin_ids: Vec::new(),
        }
    }
}

#[derive(Clone)]
pub struct RuntimePolicy {
    config: Arc<RwLock<RuntimeConfig>>,
}

impl RuntimePolicy {
    pub fn new(config: RuntimeConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
        }
    }

    pub fn snapshot(&self) -> RuntimeConfig {
        self.config
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    pub fn replace(&self, config: RuntimeConfig) {
        *self
            .config
            .write()
            .unwrap_or_else(|error| error.into_inner()) = config;
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct CommandConfig {
    pub prefix_enabled: bool,
    pub prefix: String,
}

impl Default for CommandConfig {
    fn default() -> Self {
        Self {
            prefix_enabled: false,
            prefix: identity::COMMAND_PREFIX.to_owned(),
        }
    }
}

impl CommandConfig {
    pub fn command_text<'a>(&self, message: &'a str) -> Option<&'a str> {
        let message = message.trim();
        match message.strip_prefix(&self.prefix) {
            Some(command) => Some(command.trim()),
            None if self.prefix_enabled => None,
            None => Some(message),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read config {path}: {source}")]
    Read {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse config {path}: {source}")]
    Parse {
        path: std::path::PathBuf,
        source: Box<toml::de::Error>,
    },
    #[error("failed to serialize configuration: {0}")]
    Serialize(Box<toml::ser::Error>),
    #[error("failed to write config {path}: {source}")]
    Write {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
    #[error("invalid admin configuration: {0}")]
    InvalidAdmin(String),
    #[error("invalid game configuration: {0}")]
    InvalidGame(String),
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{CommandConfig, RuntimeConfig};

    #[test]
    fn prefix_is_optional_by_default() {
        let config = CommandConfig::default();

        assert_eq!(config.command_text("签到"), Some("签到"));
        assert_eq!(config.command_text("/lr 签到"), Some("签到"));
    }

    #[test]
    fn enabled_prefix_rejects_unprefixed_commands() {
        let config = CommandConfig {
            prefix_enabled: true,
            prefix: "!realm".into(),
        };

        assert_eq!(config.command_text("签到"), None);
        assert_eq!(config.command_text("!realm 签到"), Some("签到"));
    }

    #[test]
    fn save_replaces_existing_config_and_recovers_interruption() {
        let directory = tempfile::tempdir().unwrap();
        let config = RuntimeConfig::default();
        config.save(directory.path()).unwrap();

        let mut changed = config.clone();
        changed.command.prefix = "!lr".into();
        changed.save(directory.path()).unwrap();
        assert_eq!(
            RuntimeConfig::load(directory.path())
                .unwrap()
                .command
                .prefix,
            "!lr"
        );

        let path = directory.path().join("config").join("config.toml");
        let backup = path.with_extension("toml.bak");
        let temporary = path.with_extension("toml.new");
        fs::rename(&path, &backup).unwrap();
        fs::write(&temporary, "incomplete").unwrap();

        let recovered = RuntimeConfig::load(directory.path()).unwrap();
        assert_eq!(recovered.command.prefix, "!lr");
        assert!(!backup.exists());
        assert!(!temporary.exists());
    }

    #[test]
    fn invalid_admin_port_is_rejected() {
        let mut config = RuntimeConfig::default();
        config.admin.port = 0;

        assert!(config.validate().is_err());
    }
}
