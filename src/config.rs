use std::{
    fs,
    path::Path,
    sync::{Arc, RwLock},
};

use serde::Deserialize;

use crate::identity;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct RuntimeConfig {
    pub command: CommandConfig,
    pub admin: AdminConfig,
}

impl RuntimeConfig {
    pub fn load(plugin_root: &Path) -> Result<Self, ConfigError> {
        let path = plugin_root.join("config").join("config.toml");
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
        if config.command.prefix.trim().is_empty() {
            return Err(ConfigError::EmptyPrefix(path));
        }
        if config.admin.bind.trim().is_empty() {
            return Err(ConfigError::InvalidAdmin("bind cannot be empty".into()));
        }
        if config.admin.port > 65_526 {
            return Err(ConfigError::InvalidAdmin(
                "port must leave room for ten attempts".into(),
            ));
        }

        Ok(config)
    }
}

#[derive(Clone, Debug, Deserialize)]
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

#[derive(Clone, Debug, Deserialize)]
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
    #[error("command prefix cannot be empty in {0}")]
    EmptyPrefix(std::path::PathBuf),
    #[error("invalid admin configuration: {0}")]
    InvalidAdmin(String),
}

#[cfg(test)]
mod tests {
    use super::CommandConfig;

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
}
