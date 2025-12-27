use std::path::PathBuf;
use thiserror::Error;

/// Core library error type for versualizer-core.
///
/// This error type covers configuration, lyrics, caching, and infrastructure errors.
/// Spotify-specific errors are defined in the versualizer-spotify crate.
/// UI-specific errors are defined in the application crate.
#[derive(Debug, Error)]
pub enum CoreError {
    // Configuration errors
    #[error("Config file not found at {path}. A template has been created - please edit it with your Spotify credentials and restart.")]
    ConfigNotFound { path: PathBuf },

    #[error("Invalid config: {message}")]
    ConfigInvalid { message: String },

    #[error("Missing required config field: {field}")]
    ConfigMissingField { field: String },

    #[error("Failed to parse config file: {0}")]
    ConfigParseError(#[from] toml::de::Error),

    // Lyrics errors
    #[error("Lyrics not found for track: {track} by {artist}")]
    LyricsNotFound { track: String, artist: String },

    #[error("Lyrics provider {provider} failed: {reason}")]
    LyricsProviderFailed { provider: String, reason: String },

    #[error("Failed to parse LRC: {reason}")]
    LrcParseError { reason: String },

    // Cache errors
    #[error("Cache database error: {0}")]
    CacheError(#[from] tokio_rusqlite::Error),

    #[error("SQLite error: {0}")]
    SqliteError(#[from] rusqlite::Error),

    // Network errors
    #[error("Network request failed: {0}")]
    NetworkError(#[from] reqwest::Error),

    // HTTP middleware errors
    #[error("HTTP middleware error: {0}")]
    MiddlewareError(#[from] reqwest_middleware::Error),

    // IO errors
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Convenience type alias for Results with `CoreError`.
pub type Result<T> = std::result::Result<T, CoreError>;

// Backwards compatibility alias - deprecated, will be removed in future version
#[deprecated(since = "0.2.0", note = "Use CoreError instead")]
pub type VersualizerError = CoreError;
