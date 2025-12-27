use crate::error::VersualizerError;
use crate::lrc::LrcFile;
use async_trait::async_trait;

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
    /// Spotify track ID (for Spotify lyrics provider)
    pub spotify_track_id: Option<String>,
}

impl LyricsQuery {
    /// Create a new lyrics query
    pub fn new(track_name: impl Into<String>, artist_name: impl Into<String>) -> Self {
        Self {
            track_name: track_name.into(),
            artist_name: artist_name.into(),
            album_name: None,
            duration_secs: None,
            spotify_track_id: None,
        }
    }

    /// Set album name
    pub fn with_album(mut self, album: impl Into<String>) -> Self {
        self.album_name = Some(album.into());
        self
    }

    /// Set duration
    pub fn with_duration(mut self, duration_secs: u32) -> Self {
        self.duration_secs = Some(duration_secs);
        self
    }

    /// Set Spotify track ID
    pub fn with_spotify_id(mut self, id: impl Into<String>) -> Self {
        self.spotify_track_id = Some(id.into());
        self
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
    pub fn is_found(&self) -> bool {
        !matches!(self, LyricsResult::NotFound)
    }

    /// Check if lyrics are synced
    pub fn is_synced(&self) -> bool {
        matches!(self, LyricsResult::Synced(_))
    }

    /// Get as LrcFile if synced
    pub fn as_synced(&self) -> Option<&LrcFile> {
        match self {
            LyricsResult::Synced(lrc) => Some(lrc),
            _ => None,
        }
    }

    /// Get text content regardless of type
    pub fn text(&self) -> Option<String> {
        match self {
            LyricsResult::Synced(lrc) => {
                Some(lrc.lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n"))
            }
            LyricsResult::Unsynced(text) => Some(text.clone()),
            LyricsResult::NotFound => None,
        }
    }
}

/// Trait for lyrics providers
#[async_trait]
pub trait LyricsProvider: Send + Sync {
    /// Get the provider name
    fn name(&self) -> &'static str;

    /// Fetch lyrics for a query
    async fn fetch(&self, query: &LyricsQuery) -> Result<FetchedLyrics, VersualizerError>;
}
