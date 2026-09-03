use std::path::{Path, PathBuf};

use crate::identity;

/// 插件根目录：优先取部署方显式声明的环境变量，否则退回进程工作目录。
pub fn plugin_root() -> PathBuf {
    [
        identity::PLUGIN_DIRECTORY_ENV,
        "LUO9_PLUGIN_DIR",
        "LUO9_PLUGIN_PATH",
        "PLUGIN_DIR",
    ]
    .into_iter()
    .find_map(std::env::var_os)
    .map(PathBuf::from)
    .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

/// 插件数据目录：所有运行期产物（数据库、战报、素材、配置）统一收纳于此。
pub fn data_directory(plugin_root: &Path) -> PathBuf {
    plugin_root.join(identity::DATA_DIRECTORY)
}

pub fn database_path() -> PathBuf {
    data_directory(&plugin_root()).join(identity::DATABASE_FILE)
}
