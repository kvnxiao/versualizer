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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_music_source_as_str() {
        assert_eq!(MusicSource::Spotify.as_str(), "spotify");
        assert_eq!(MusicSource::Mpris.as_str(), "mpris");
        assert_eq!(MusicSource::WindowsMedia.as_str(), "windows_media");
        assert_eq!(MusicSource::YouTubeMusic.as_str(), "youtube_music");
    }

    #[test]
    fn test_music_source_display() {
        assert_eq!(format!("{}", MusicSource::Spotify), "spotify");
        assert_eq!(format!("{}", MusicSource::Mpris), "mpris");
        assert_eq!(format!("{}", MusicSource::WindowsMedia), "windows_media");
        assert_eq!(format!("{}", MusicSource::YouTubeMusic), "youtube_music");
    }

    #[test]
    fn test_music_source_serialization() {
        // Test serde serialization with snake_case
        // Note: YouTubeMusic becomes "you_tube_music" due to snake_case conversion
        let spotify = MusicSource::Spotify;
        let serialized = serde_json::to_string(&spotify).unwrap();
        assert_eq!(serialized, "\"spotify\"");

        let windows = MusicSource::WindowsMedia;
        let serialized = serde_json::to_string(&windows).unwrap();
        assert_eq!(serialized, "\"windows_media\"");

        let youtube = MusicSource::YouTubeMusic;
        let serialized = serde_json::to_string(&youtube).unwrap();
        assert_eq!(serialized, "\"you_tube_music\"");
    }

    #[test]
    fn test_music_source_deserialization() {
        let spotify: MusicSource = serde_json::from_str("\"spotify\"").unwrap();
        assert_eq!(spotify, MusicSource::Spotify);

        let mpris: MusicSource = serde_json::from_str("\"mpris\"").unwrap();
        assert_eq!(mpris, MusicSource::Mpris);

        let windows: MusicSource = serde_json::from_str("\"windows_media\"").unwrap();
        assert_eq!(windows, MusicSource::WindowsMedia);

        let youtube: MusicSource = serde_json::from_str("\"you_tube_music\"").unwrap();
        assert_eq!(youtube, MusicSource::YouTubeMusic);
    }

    #[test]
    fn test_music_source_equality() {
        assert_eq!(MusicSource::Spotify, MusicSource::Spotify);
        assert_ne!(MusicSource::Spotify, MusicSource::Mpris);
    }

    #[test]
    fn test_music_source_clone() {
        let source = MusicSource::Spotify;
        let cloned = source;
        assert_eq!(source, cloned);
    }

    #[test]
    fn test_music_source_hash() {
        use std::collections::HashSet;

        let mut set = HashSet::new();
        set.insert(MusicSource::Spotify);
        set.insert(MusicSource::Mpris);
        set.insert(MusicSource::Spotify); // Duplicate

        assert_eq!(set.len(), 2);
        assert!(set.contains(&MusicSource::Spotify));
        assert!(set.contains(&MusicSource::Mpris));
    }
}
