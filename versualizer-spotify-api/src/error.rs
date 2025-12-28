use thiserror::Error;

/// Unified error type for all Spotify-related operations.
///
/// This consolidates errors from OAuth, polling, and lyrics fetching
/// into a single error type owned by the spotify crate.
#[derive(Debug, Error)]
pub enum SpotifyError {
    /// Authentication failed during OAuth flow or token exchange.
    #[error("Spotify authentication failed: {reason}")]
    AuthFailed { reason: String },

    /// Token has expired and could not be refreshed.
    #[error("Spotify token expired and refresh failed")]
    TokenExpired,

    /// Spotify API returned a rate limit response.
    #[error("Spotify API rate limited, retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u32 },

    /// No active Spotify playback on any device.
    #[error("Spotify playback not active on any device")]
    NoActivePlayback,

    /// Error from the Spotify API client.
    #[error("Spotify API error: {0}")]
    Api(#[from] rspotify::ClientError),

    /// Failed to read the token cache file or perform I/O.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Failed to parse or serialize JSON data.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Poller was stopped.
    #[error("Spotify poller stopped")]
    PollerStopped,
}

/// Convenience type alias for Results with `SpotifyError`.
pub type Result<T> = std::result::Result<T, SpotifyError>;
