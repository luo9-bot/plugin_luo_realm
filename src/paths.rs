use std::path::PathBuf;

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

pub fn database_path() -> PathBuf {
    plugin_root()
        .join(identity::DATA_DIRECTORY)
        .join(identity::DATABASE_FILE)
}
