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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lyrics_query_new() {
        let query = LyricsQuery::new("Test Song", "Test Artist");

        assert_eq!(query.track_name, "Test Song");
        assert_eq!(query.artist_name, "Test Artist");
        assert!(query.album_name.is_none());
        assert!(query.duration_secs.is_none());
        assert!(query.provider_ids.is_empty());
    }

    #[test]
    fn test_lyrics_query_with_album() {
        let query = LyricsQuery::new("Song", "Artist").with_album("Album");

        assert_eq!(query.album_name, Some("Album".to_string()));
    }

    #[test]
    fn test_lyrics_query_with_duration() {
        let query = LyricsQuery::new("Song", "Artist").with_duration(180);

        assert_eq!(query.duration_secs, Some(180));
    }

    #[test]
    fn test_lyrics_query_with_provider_id() {
        let query = LyricsQuery::new("Song", "Artist")
            .with_provider_id("spotify", "spotify_track_123")
            .with_provider_id("lrclib", "12345");

        assert_eq!(query.provider_id("spotify"), Some("spotify_track_123"));
        assert_eq!(query.provider_id("lrclib"), Some("12345"));
        assert_eq!(query.provider_id("unknown"), None);
    }

    #[test]
    fn test_lyrics_query_spotify_track_id() {
        let query = LyricsQuery::new("Song", "Artist")
            .with_provider_id("spotify", "4uLU6hMCjMI75M1A2tKUQC");

        assert_eq!(
            query.spotify_track_id(),
            Some("4uLU6hMCjMI75M1A2tKUQC")
        );
    }

    #[test]
    fn test_lyrics_query_spotify_track_id_missing() {
        let query = LyricsQuery::new("Song", "Artist");

        assert!(query.spotify_track_id().is_none());
    }

    #[test]
    fn test_lyrics_query_chained_builder() {
        let query = LyricsQuery::new("Test Song", "Test Artist")
            .with_album("Test Album")
            .with_duration(200)
            .with_provider_id("spotify", "abc123");

        assert_eq!(query.track_name, "Test Song");
        assert_eq!(query.artist_name, "Test Artist");
        assert_eq!(query.album_name, Some("Test Album".to_string()));
        assert_eq!(query.duration_secs, Some(200));
        assert_eq!(query.provider_id("spotify"), Some("abc123"));
    }

    #[test]
    fn test_lyrics_result_not_found() {
        let result = LyricsResult::NotFound;

        assert!(!result.is_found());
        assert!(!result.is_synced());
        assert!(result.as_synced().is_none());
        assert!(result.text().is_none());
    }

    #[test]
    fn test_lyrics_result_unsynced() {
        let result = LyricsResult::Unsynced("Plain text lyrics\nLine 2".to_string());

        assert!(result.is_found());
        assert!(!result.is_synced());
        assert!(result.as_synced().is_none());
        assert_eq!(
            result.text(),
            Some("Plain text lyrics\nLine 2".to_string())
        );
    }

    #[test]
    fn test_lyrics_result_synced() {
        let lrc = LrcFile::parse("[00:05.00]First line\n[00:10.00]Second line").unwrap();
        let result = LyricsResult::Synced(lrc);

        assert!(result.is_found());
        assert!(result.is_synced());
        assert!(result.as_synced().is_some());

        let text = result.text().unwrap();
        assert!(text.contains("First line"));
        assert!(text.contains("Second line"));
    }

    #[test]
    fn test_lyrics_result_synced_text_extraction() {
        let lrc = LrcFile::parse("[00:05.00]Line 1\n[00:10.00]Line 2\n[00:15.00]Line 3").unwrap();
        let result = LyricsResult::Synced(lrc);

        let text = result.text().unwrap();
        assert_eq!(text, "Line 1\nLine 2\nLine 3");
    }

    #[test]
    fn test_fetched_lyrics_struct() {
        let lrc = LrcFile::parse("[00:05.00]Test").unwrap();
        let fetched = FetchedLyrics {
            result: LyricsResult::Synced(lrc),
            provider_id: "12345".to_string(),
        };

        assert!(fetched.result.is_synced());
        assert_eq!(fetched.provider_id, "12345");
    }
}
