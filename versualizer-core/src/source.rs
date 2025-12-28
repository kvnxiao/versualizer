//! Music source identification and provider trait.

use crate::error::Result;
use crate::SyncEngine;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Identifies a music source (e.g., Spotify, MPRIS, Windows `MediaPlayer`).
///
/// Music sources are the applications or services that provide playback state
/// and track information. Each source may have its own track ID format and
/// metadata structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MusicSource {
    /// Spotify streaming service
    Spotify,
    /// MPRIS (Media Player Remote Interfacing Specification) on Linux
    Mpris,
    /// Windows Media Player / System Media Transport Controls
    WindowsMedia,
    /// `YouTube` Music streaming service
    YouTubeMusic,
}

impl MusicSource {
    /// Get the string identifier used in database/cache lookups.
    ///
    /// This identifier is stable and used for persistence, so it should not
    /// change once established.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Spotify => "spotify",
            Self::Mpris => "mpris",
            Self::WindowsMedia => "windows_media",
            Self::YouTubeMusic => "youtube_music",
        }
    }
}

impl std::fmt::Display for MusicSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Trait for music source providers that supply playback state.
///
/// Music source providers monitor a music player (Spotify, MPRIS, etc.) and
/// report playback state changes to the [`SyncEngine`]. Implementations should:
///
/// - Poll or subscribe to playback state from their source
/// - Update the `SyncEngine` with [`PlaybackState`](crate::PlaybackState) changes
/// - Handle authentication and connection errors gracefully
/// - Support graceful shutdown via cancellation token
///
/// # Example
///
/// ```ignore
/// // In your app:
/// let provider = SpotifySourceProvider::new(config, sync_engine, cancel_token)?;
/// provider.start().await?;
/// ```
#[async_trait]
pub trait MusicSourceProvider: Send + Sync {
    /// Returns the type of music source this provider handles.
    fn source(&self) -> MusicSource;

    /// Returns a human-readable name for this provider.
    fn name(&self) -> &'static str;

    /// Start the provider, running until cancelled or an unrecoverable error occurs.
    ///
    /// This method should:
    /// - Begin monitoring the music source for playback state
    /// - Update the `SyncEngine` whenever playback state changes
    /// - Handle transient errors with retries/backoff
    /// - Return when the cancellation token is triggered or on fatal error
    ///
    /// # Errors
    ///
    /// Returns an error if the provider fails to start or encounters an
    /// unrecoverable error during operation.
    async fn run(&self) -> Result<()>;

    /// Get the cancellation token for this provider.
    ///
    /// Used to signal graceful shutdown.
    fn cancel_token(&self) -> CancellationToken;

    /// Signal the provider to stop.
    fn stop(&self) {
        self.cancel_token().cancel();
    }
}

/// Builder for creating music source providers.
///
/// This trait allows providers to be constructed with common dependencies.
#[async_trait]
pub trait MusicSourceProviderBuilder: Send + Sync {
    /// The provider type this builder creates.
    type Provider: MusicSourceProvider;

    /// Build a new music source provider.
    ///
    /// # Arguments
    ///
    /// * `sync_engine` - The sync engine to update with playback state
    /// * `cancel_token` - Token to signal graceful shutdown
    ///
    /// # Errors
    ///
    /// Returns an error if the provider cannot be constructed (e.g., missing config).
    async fn build(
        self,
        sync_engine: Arc<SyncEngine>,
        cancel_token: CancellationToken,
    ) -> Result<Self::Provider>;
}
