use std::{
    fs, io,
    path::{Path, PathBuf},
};

use crate::identity;

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

/// 旧版布局迁移：根目录下的 assets/、config/ 搬入插件数据目录。
/// 仅在旧目录存在且新目录不存在时搬移；跨卷移动失败时退化为复制并保留旧目录。
pub fn migrate_legacy_layout() -> io::Result<()> {
    let root = plugin_root();
    let data = data_directory(&root);
    ["assets", "config"]
        .into_iter()
        .try_for_each(|name| migrate_directory(&root.join(name), &data.join(name)))
}

fn migrate_directory(legacy: &Path, target: &Path) -> io::Result<()> {
    if !legacy.is_dir() || target.exists() {
        return Ok(());
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    if fs::rename(legacy, target).is_ok() {
        return Ok(());
    }
    copy_tree(legacy, target)
}

fn copy_tree(source: &Path, target: &Path) -> io::Result<()> {
    fs::create_dir_all(target)?;
    fs::read_dir(source)?.try_for_each(|entry| {
        let entry = entry?;
        let destination = target.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &destination)
        } else {
            fs::copy(entry.path(), destination).map(|_| ())
        }
    })
}
