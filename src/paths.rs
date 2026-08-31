use std::path::PathBuf;

use crate::identity;

pub fn plugin_root() -> PathBuf {
    for variable in [
        identity::PLUGIN_DIRECTORY_ENV,
        "LUO9_PLUGIN_DIR",
        "LUO9_PLUGIN_PATH",
        "PLUGIN_DIR",
    ] {
        if let Some(value) = std::env::var_os(variable) {
            return PathBuf::from(value);
        }
    }

    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub fn database_path() -> PathBuf {
    plugin_root()
        .join(identity::DATA_DIRECTORY)
        .join(identity::DATABASE_FILE)
}
