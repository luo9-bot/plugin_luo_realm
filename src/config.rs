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
    pub player_web: PlayerWebConfig,
    pub profile_card: ProfileCardConfig,
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
            player_web: PlayerWebConfig::default(),
            profile_card: ProfileCardConfig::default(),
        }
    }
}

/// 角色卡头像的外框形状。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PortraitShape {
    /// 圆形裁剪，双细金圈包边（默认）。
    #[default]
    Circle,
    /// 方形裁剪，细线相框。
    Square,
    /// 无框直出，仅按填充方式取景。
    Plain,
}

/// 角色卡立绘的填充方式。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PortraitFill {
    /// 等比缩放铺满并居中裁剪（默认；任何比例的立绘都不会被压扁）。
    #[default]
    Cover,
    /// 等比缩放完整置入，不足处回填暗色。
    Contain,
    /// 强制拉伸铺满（旧行为，会改变宽高比，仅建议方形立绘使用）。
    Stretch,
}

/// 角色卡头像呈现样式。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct ProfileCardConfig {
    pub portrait_shape: PortraitShape,
    pub portrait_fill: PortraitFill,
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
        let path = crate::paths::data_directory(plugin_root)
            .join("config")
            .join("config.toml");
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
        let path = crate::paths::data_directory(plugin_root)
            .join("config")
            .join("config.toml");
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
        self.player_web.validate()?;
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

/// 玩家网页（只读档案页）配置。
///
/// 两种部署形态：
/// - **直连**（本地）：不配置 `sync_url`，`主页` 签发一次性票据，页面直接
///   调用插件接口，此时 `base_url` 指向插件自带的 `/player` 页面。
/// - **Cloudflare**（可选增强）：配置 `sync_url` 与 `sync_token` 后，插件把
///   档案快照主动推送到 Cloudflare Worker（出站请求，源站不暴露公网），
///   `主页` 返回 `{base_url}?token=...` 指向 Pages 静态站点；页面写操作由
///   Worker 以服务端环境变量中隐藏的源站地址转发回来。
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct PlayerWebConfig {
    pub enabled: bool,
    /// 玩家页面基地址，不得以 `/` 结尾、不得携带查询或片段。
    pub base_url: String,
    /// 允许跨域访问玩家 API 的页面来源；为空时仅同源页面可用。
    pub allowed_origins: Vec<String>,
    /// 一次性票据有效期（分钟）。
    pub ticket_ttl_minutes: u32,
    /// 网页会话有效期（分钟）。
    pub session_ttl_minutes: u32,
    /// Cloudflare Worker 的快照接收地址；为空表示直连模式。
    pub sync_url: String,
    /// 与 Worker 共享的推送/回传密钥，至少 32 字符。
    pub sync_token: String,
}

impl Default for PlayerWebConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: "http://127.0.0.1:18765/player".into(),
            allowed_origins: Vec::new(),
            ticket_ttl_minutes: 10,
            session_ttl_minutes: 120,
            sync_url: String::new(),
            sync_token: String::new(),
        }
    }
}

impl PlayerWebConfig {
    /// 是否处于 Cloudflare 推送模式。
    pub fn sync_enabled(&self) -> bool {
        !self.sync_url.trim().is_empty() || !self.sync_token.trim().is_empty()
    }

    fn validate(&self) -> Result<(), ConfigError> {
        if !self.enabled {
            return Ok(());
        }
        if !valid_web_url(&self.base_url) {
            return Err(ConfigError::InvalidPlayerWeb(
                "player web base_url must be an http(s) URL without query, fragment or trailing slash"
                    .into(),
            ));
        }
        for origin in &self.allowed_origins {
            if !valid_web_url(origin) {
                return Err(ConfigError::InvalidPlayerWeb(format!(
                    "allowed origin {origin} must be an http(s) origin without a path"
                )));
            }
        }
        if !(1..=60).contains(&self.ticket_ttl_minutes) {
            return Err(ConfigError::InvalidPlayerWeb(
                "ticket ttl must be between 1 and 60 minutes".into(),
            ));
        }
        if !(5..=1_440).contains(&self.session_ttl_minutes) {
            return Err(ConfigError::InvalidPlayerWeb(
                "session ttl must be between 5 and 1440 minutes".into(),
            ));
        }
        if self.sync_enabled()
            && (!valid_web_url(&self.sync_url) || self.sync_token.trim().len() < 32)
        {
            return Err(ConfigError::InvalidPlayerWeb(
                "sync_url must be an http(s) URL and sync_token at least 32 characters".into(),
            ));
        }
        if !self.sync_enabled() && !self.sync_token.trim().is_empty() {
            return Err(ConfigError::InvalidPlayerWeb(
                "sync_token is set but sync_url is empty".into(),
            ));
        }
        Ok(())
    }
}

/// 校验玩家网页使用的 http(s) 地址：无空白、无查询、无片段、不以 `/` 结尾。
fn valid_web_url(value: &str) -> bool {
    let value = value.trim();
    (value.starts_with("http://") || value.starts_with("https://"))
        && !value.contains(char::is_whitespace)
        && !value.contains('?')
        && !value.contains('#')
        && !value.ends_with('/')
        && value.len() <= 2_048
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
    #[error("invalid player web configuration: {0}")]
    InvalidPlayerWeb(String),
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

        let path = crate::paths::data_directory(directory.path())
            .join("config")
            .join("config.toml");
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

    #[test]
    fn profile_card_parses_shape_and_fill() {
        let config = RuntimeConfig::default();
        assert_eq!(
            config.profile_card.portrait_shape,
            super::PortraitShape::Circle
        );
        assert_eq!(
            config.profile_card.portrait_fill,
            super::PortraitFill::Cover
        );

        let parsed: RuntimeConfig = toml::from_str(
            "[profile_card]\nportrait_shape = \"square\"\nportrait_fill = \"contain\"\n",
        )
        .unwrap();
        assert_eq!(
            parsed.profile_card.portrait_shape,
            super::PortraitShape::Square
        );
        assert_eq!(
            parsed.profile_card.portrait_fill,
            super::PortraitFill::Contain
        );
    }
}
