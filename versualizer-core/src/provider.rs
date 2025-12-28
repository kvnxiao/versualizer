use crate::error::CoreError;
use crate::lrc::LrcFile;
use async_trait::async_trait;
use std::collections::HashMap;

/// Query parameters for fetching lyrics
#[derive(Debug, Clone)]
pub struct LyricsQuery {
    /// Track name
    pub track_name: String,
    /// Artist name
    pub artist_name: String,
    /// Album name (optional)
    pub album_name: Option<String>,
    /// Track duration in seconds (for matching)
    pub duration_secs: Option<u32>,
    /// Provider-specific track IDs (key: provider name, value: track ID)
    pub provider_ids: HashMap<String, String>,
}

impl LyricsQuery {
    /// Create a new lyrics query
    pub fn new(track_name: impl Into<String>, artist_name: impl Into<String>) -> Self {
        Self {
            track_name: track_name.into(),
            artist_name: artist_name.into(),
            album_name: None,
            duration_secs: None,
            provider_ids: HashMap::new(),
        }
    }

    /// Set album name
    #[must_use]
    pub fn with_album(mut self, album: impl Into<String>) -> Self {
        self.album_name = Some(album.into());
        self
    }

    /// Set duration
    #[must_use]
    pub const fn with_duration(mut self, duration_secs: u32) -> Self {
        self.duration_secs = Some(duration_secs);
        self
    }

    /// Add a provider-specific track ID
    #[must_use]
    pub fn with_provider_id(mut self, provider: impl Into<String>, id: impl Into<String>) -> Self {
        self.provider_ids.insert(provider.into(), id.into());
        self
    }

    /// Get a provider-specific track ID
    #[must_use]
    pub fn provider_id(&self, provider: &str) -> Option<&str> {
        self.provider_ids.get(provider).map(String::as_str)
    }

    /// Convenience method to get Spotify track ID
    #[must_use]
    pub fn spotify_track_id(&self) -> Option<&str> {
        self.provider_id("spotify")
    }
}

/// Result from a lyrics provider
#[derive(Debug, Clone)]
pub enum LyricsResult {
    /// Synchronized lyrics with timing
    Synced(LrcFile),
    /// Plain text lyrics without timing
    Unsynced(String),
    /// No lyrics found
    NotFound,
}

/// Lyrics with provider metadata
#[derive(Debug, Clone)]
pub struct FetchedLyrics {
    /// The lyrics result
    pub result: LyricsResult,
    /// Provider-specific ID (e.g., LRCLIB's numeric ID as string, Spotify track ID)
    pub provider_id: String,
}

impl LyricsResult {
    /// Check if lyrics were found
    #[must_use]
    pub const fn is_found(&self) -> bool {
        !matches!(self, Self::NotFound)
    }

    /// Check if lyrics are synced
    #[must_use]
    pub const fn is_synced(&self) -> bool {
        matches!(self, Self::Synced(_))
    }

    /// Get as `LrcFile` if synced
    #[must_use]
    pub const fn as_synced(&self) -> Option<&LrcFile> {
        match self {
            Self::Synced(lrc) => Some(lrc),
            _ => None,
        }
    }

    /// Get text content regardless of type
    #[must_use]
    pub fn text(&self) -> Option<String> {
        match self {
            Self::Synced(lrc) => Some(
                lrc.lines
                    .iter()
                    .map(|l| l.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            Self::Unsynced(text) => Some(text.clone()),
            Self::NotFound => None,
        }
    }
}

/// Trait for lyrics providers
#[async_trait]
pub trait LyricsProvider: Send + Sync {
    /// Get the provider name
    fn name(&self) -> &'static str;

    /// Fetch lyrics for a query
    async fn fetch(&self, query: &LyricsQuery) -> Result<FetchedLyrics, CoreError>;
}
