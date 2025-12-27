//! Path constants for Spotify-specific files.

use std::path::PathBuf;

/// The name of the Spotify token cache file
pub const SPOTIFY_TOKEN_CACHE_FILE_NAME: &str = ".spotify_token_cache.json";

/// Get the Spotify token cache file path (~/.config/versualizer/.spotify_token_cache.json)
pub fn spotify_token_cache_path() -> PathBuf {
    versualizer_core::paths::config_dir().join(SPOTIFY_TOKEN_CACHE_FILE_NAME)
}
