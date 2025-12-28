//! Path constants for configuration and cache files.

use std::path::PathBuf;

/// The name of the configuration directory under ~/.config/
pub const CONFIG_DIR_NAME: &str = "versualizer";

/// The name of the main configuration file
pub const CONFIG_FILE_NAME: &str = "config.toml";

/// The name of the lyrics cache database file
pub const LYRICS_CACHE_DB_FILE_NAME: &str = "lyrics_cache.db";

/// The name of the window state cache file (prefixed with . for hidden)
pub const WINDOW_STATE_FILE_NAME: &str = ".window_state.json";

/// Get the configuration directory path (~/.config/versualizer/)
#[must_use]
pub fn config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join(CONFIG_DIR_NAME)
}

/// Get the config file path (~/.config/versualizer/config.toml)
#[must_use]
pub fn config_path() -> PathBuf {
    config_dir().join(CONFIG_FILE_NAME)
}

/// Get the lyrics cache database path (`~/.config/versualizer/lyrics_cache.db`)
#[must_use]
pub fn lyrics_cache_db_path() -> PathBuf {
    config_dir().join(LYRICS_CACHE_DB_FILE_NAME)
}

/// Get the window state file path (`~/.config/versualizer/.window_state.json`)
#[must_use]
pub fn window_state_path() -> PathBuf {
    config_dir().join(WINDOW_STATE_FILE_NAME)
}
