use crate::error::{CoreError, Result};
use crate::lrc::LrcFile;
use crate::provider::LyricsResult;
use chrono::{DateTime, Utc};
use rusqlite::OptionalExtension;
use std::path::Path;
use tokio_rusqlite::Connection;
use tracing::{debug, info};

const SCHEMA_SQL: &str = r"
-- Core lyrics storage (source-agnostic)
CREATE TABLE IF NOT EXISTS lyrics (
    id INTEGER PRIMARY KEY,
    artist TEXT NOT NULL,
    track TEXT NOT NULL,
    album TEXT,
    duration_ms INTEGER,
    provider TEXT NOT NULL,
    provider_id TEXT NOT NULL,
    lyrics_type TEXT NOT NULL,
    content TEXT NOT NULL,
    fetched_at INTEGER NOT NULL,
    UNIQUE(artist, track, album)
);

-- Mapping table: provider track IDs -> lyrics
CREATE TABLE IF NOT EXISTS track_id_mapping (
    id INTEGER PRIMARY KEY,
    provider TEXT NOT NULL,
    provider_track_id TEXT NOT NULL,
    lyrics_id INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (lyrics_id) REFERENCES lyrics(id) ON DELETE CASCADE,
    UNIQUE(provider, provider_track_id)
);

CREATE INDEX IF NOT EXISTS idx_lyrics_artist_track ON lyrics(artist, track);
CREATE INDEX IF NOT EXISTS idx_mapping_provider ON track_id_mapping(provider, provider_track_id);
CREATE INDEX IF NOT EXISTS idx_lyrics_provider_id ON lyrics(provider, provider_id);
";

/// Cached lyrics entry
#[derive(Debug, Clone)]
pub struct CachedLyrics {
    pub id: i64,
    pub artist: String,
    pub track: String,
    pub album: Option<String>,
    pub duration_ms: Option<i64>,
    pub provider: String,
    pub provider_id: String,
    pub lyrics_type: LyricsType,
    pub content: String,
    pub fetched_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LyricsType {
    Synced,
    Unsynced,
}

impl LyricsType {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Synced => "synced",
            Self::Unsynced => "unsynced",
        }
    }

    fn from_str(s: &str) -> Option<Self> {
        match s {
            "synced" => Some(Self::Synced),
            "unsynced" => Some(Self::Unsynced),
            _ => None,
        }
    }
}

impl CachedLyrics {
    /// Convert cached content to `LyricsResult`
    #[must_use]
    pub fn to_lyrics_result(&self) -> LyricsResult {
        match self.lyrics_type {
            LyricsType::Synced => LrcFile::parse(&self.content).map_or_else(
                |_| LyricsResult::Unsynced(self.content.clone()),
                LyricsResult::Synced,
            ),
            LyricsType::Unsynced => LyricsResult::Unsynced(self.content.clone()),
        }
    }
}

/// Track metadata for cache storage
#[derive(Debug, Clone)]
pub struct TrackMetadata {
    pub artist: String,
    pub track: String,
    pub album: Option<String>,
    pub duration_ms: Option<i64>,
}

/// SQLite-based lyrics cache
pub struct LyricsCache {
    conn: Connection,
}

impl LyricsCache {
    /// Create a new cache at the default location
    ///
    /// # Errors
    ///
    /// Returns an error if the cache database cannot be created or opened.
    pub async fn new() -> Result<Self> {
        let cache_path = crate::paths::lyrics_cache_db_path();
        Self::open(&cache_path).await
    }

    /// Open a cache at a specific path
    ///
    /// # Errors
    ///
    /// Returns an error if the database cannot be opened or initialized.
    pub async fn open(path: &Path) -> Result<Self> {
        info!("Opening lyrics cache database at {:?}", path);

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(path).await?;

        // Initialize schema
        conn.call(|conn| {
            conn.execute_batch(SCHEMA_SQL)?;
            conn.pragma_update(None, "journal_mode", "WAL")?;
            conn.pragma_update(None, "foreign_keys", "ON")?;
            Ok(())
        })
        .await?;

        info!("Lyrics cache database initialized");
        Ok(Self { conn })
    }

    /// Fast lookup by provider track ID (e.g., Spotify track ID)
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn get_by_provider_id(
        &self,
        provider: &str,
        provider_track_id: &str,
    ) -> Result<Option<CachedLyrics>> {
        debug!(
            "Looking up lyrics in cache by provider ID: {}:{}",
            provider, provider_track_id
        );
        let provider = provider.to_string();
        let id = provider_track_id.to_string();

        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare_cached(
                    r"
                    SELECT l.id, l.artist, l.track, l.album, l.duration_ms,
                           l.provider, l.provider_id, l.lyrics_type, l.content, l.fetched_at
                    FROM lyrics l
                    INNER JOIN track_id_mapping m ON l.id = m.lyrics_id
                    WHERE m.provider = ?1 AND m.provider_track_id = ?2
                ",
                )?;

                let result = stmt
                    .query_row(rusqlite::params![provider, id], |row| {
                        Ok(CachedLyrics {
                            id: row.get(0)?,
                            artist: row.get(1)?,
                            track: row.get(2)?,
                            album: row.get(3)?,
                            duration_ms: row.get(4)?,
                            provider: row.get(5)?,
                            provider_id: row.get(6)?,
                            lyrics_type: LyricsType::from_str(&row.get::<_, String>(7)?)
                                .unwrap_or(LyricsType::Unsynced),
                            content: row.get(8)?,
                            fetched_at: DateTime::from_timestamp(row.get::<_, i64>(9)?, 0)
                                .unwrap_or_else(Utc::now),
                        })
                    })
                    .optional()?;

                Ok(result)
            })
            .await
            .map_err(Into::into)
    }

    /// Fallback lookup by metadata (when source ID not cached)
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn get_by_metadata(
        &self,
        artist: &str,
        track: &str,
        album: Option<&str>,
    ) -> Result<Option<CachedLyrics>> {
        let artist = artist.to_lowercase();
        let track = track.to_lowercase();
        let album = album.map(str::to_lowercase);

        self.conn
            .call(move |conn| {
                let result = if let Some(album) = album {
                    let mut stmt = conn.prepare_cached(
                        r"
                        SELECT id, artist, track, album, duration_ms,
                               provider, provider_id, lyrics_type, content, fetched_at
                        FROM lyrics
                        WHERE LOWER(artist) = ?1 AND LOWER(track) = ?2 AND LOWER(album) = ?3
                    ",
                    )?;

                    stmt.query_row(rusqlite::params![artist, track, album], |row| {
                        Ok(CachedLyrics {
                            id: row.get(0)?,
                            artist: row.get(1)?,
                            track: row.get(2)?,
                            album: row.get(3)?,
                            duration_ms: row.get(4)?,
                            provider: row.get(5)?,
                            provider_id: row.get(6)?,
                            lyrics_type: LyricsType::from_str(&row.get::<_, String>(7)?)
                                .unwrap_or(LyricsType::Unsynced),
                            content: row.get(8)?,
                            fetched_at: DateTime::from_timestamp(row.get::<_, i64>(9)?, 0)
                                .unwrap_or_else(Utc::now),
                        })
                    })
                    .optional()?
                } else {
                    let mut stmt = conn.prepare_cached(
                        r"
                        SELECT id, artist, track, album, duration_ms,
                               provider, provider_id, lyrics_type, content, fetched_at
                        FROM lyrics
                        WHERE LOWER(artist) = ?1 AND LOWER(track) = ?2
                        ORDER BY fetched_at DESC
                        LIMIT 1
                    ",
                    )?;

                    stmt.query_row(rusqlite::params![artist, track], |row| {
                        Ok(CachedLyrics {
                            id: row.get(0)?,
                            artist: row.get(1)?,
                            track: row.get(2)?,
                            album: row.get(3)?,
                            duration_ms: row.get(4)?,
                            provider: row.get(5)?,
                            provider_id: row.get(6)?,
                            lyrics_type: LyricsType::from_str(&row.get::<_, String>(7)?)
                                .unwrap_or(LyricsType::Unsynced),
                            content: row.get(8)?,
                            fetched_at: DateTime::from_timestamp(row.get::<_, i64>(9)?, 0)
                                .unwrap_or_else(Utc::now),
                        })
                    })
                    .optional()?
                };

                Ok(result)
            })
            .await
            .map_err(Into::into)
    }

    /// Store lyrics and create mapping to provider track ID
    ///
    /// # Errors
    ///
    /// Returns an error if the lyrics cannot be stored or if the lyrics result is `NotFound`.
    pub async fn store(
        &self,
        provider: &str,
        provider_track_id: &str,
        lyrics: &LyricsResult,
        metadata: &TrackMetadata,
        lyrics_provider: &str,
        lyrics_provider_id: &str,
    ) -> Result<i64> {
        info!(
            "Storing lyrics in cache: {} - {} (lyrics_provider: {}, lyrics_provider_id: {}, provider: {}:{})",
            metadata.artist, metadata.track, lyrics_provider, lyrics_provider_id, provider, provider_track_id
        );
        let provider = provider.to_string();
        let provider_track_id = provider_track_id.to_string();
        let lyrics_provider = lyrics_provider.to_string();
        let lyrics_provider_id = lyrics_provider_id.to_string();
        let metadata = metadata.clone();

        let (lyrics_type, content) = match lyrics {
            LyricsResult::Synced(lrc) => {
                // Store the original LRC content - we need to serialize it
                let content = serialize_lrc(lrc);
                (LyricsType::Synced, content)
            }
            LyricsResult::Unsynced(text) => (LyricsType::Unsynced, text.clone()),
            LyricsResult::NotFound => {
                return Err(CoreError::LyricsNotFound {
                    track: metadata.track.clone(),
                    artist: metadata.artist,
                });
            }
        };

        let now = Utc::now().timestamp();
        let lyrics_type_str = lyrics_type.as_str().to_string();

        self.conn
            .call(move |conn| {
                // Insert or update lyrics entry
                conn.execute(
                    r"
                    INSERT INTO lyrics (artist, track, album, duration_ms, provider, provider_id, lyrics_type, content, fetched_at)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                    ON CONFLICT(artist, track, album) DO UPDATE SET
                        provider = excluded.provider,
                        provider_id = excluded.provider_id,
                        lyrics_type = excluded.lyrics_type,
                        content = excluded.content,
                        fetched_at = excluded.fetched_at
                ",
                    rusqlite::params![
                        metadata.artist,
                        metadata.track,
                        metadata.album,
                        metadata.duration_ms,
                        lyrics_provider,
                        lyrics_provider_id,
                        lyrics_type_str,
                        content,
                        now
                    ],
                )?;

                let lyrics_id = conn.last_insert_rowid();

                // Create mapping from provider track ID to lyrics
                conn.execute(
                    r"
                    INSERT INTO track_id_mapping (provider, provider_track_id, lyrics_id, created_at)
                    VALUES (?1, ?2, ?3, ?4)
                    ON CONFLICT(provider, provider_track_id) DO UPDATE SET
                        lyrics_id = excluded.lyrics_id,
                        created_at = excluded.created_at
                ",
                    rusqlite::params![provider, provider_track_id, lyrics_id, now],
                )?;

                Ok(lyrics_id)
            })
            .await
            .map_err(Into::into)
    }

    /// Delete old cache entries beyond TTL
    ///
    /// # Errors
    ///
    /// Returns an error if the database cleanup fails.
    pub async fn cleanup(&self, ttl_days: u32) -> Result<usize> {
        let cutoff = Utc::now().timestamp() - (i64::from(ttl_days) * 24 * 60 * 60);

        self.conn
            .call(move |conn| {
                let deleted = conn.execute(
                    "DELETE FROM lyrics WHERE fetched_at < ?1",
                    rusqlite::params![cutoff],
                )?;
                Ok(deleted)
            })
            .await
            .map_err(Into::into)
    }

    /// Checkpoint WAL for clean shutdown
    ///
    /// # Errors
    ///
    /// Returns an error if the WAL checkpoint fails.
    pub async fn checkpoint(&self) -> Result<()> {
        self.conn
            .call(|conn| {
                conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")?;
                Ok(())
            })
            .await
            .map_err(Into::into)
    }
}

/// Serialize an `LrcFile` back to LRC format for storage
fn serialize_lrc(lrc: &LrcFile) -> String {
    use std::fmt::Write;

    let mut output = String::new();

    // Write metadata
    if let Some(ref title) = lrc.metadata.title {
        let _ = writeln!(output, "[ti:{title}]");
    }
    if let Some(ref artist) = lrc.metadata.artist {
        let _ = writeln!(output, "[ar:{artist}]");
    }
    if let Some(ref album) = lrc.metadata.album {
        let _ = writeln!(output, "[al:{album}]");
    }
    if lrc.metadata.offset != 0 {
        let _ = writeln!(output, "[offset:{}]", lrc.metadata.offset);
    }

    // Write lines
    for line in &lrc.lines {
        let timestamp = format_timestamp(line.start_time);

        if let Some(ref words) = line.words {
            // Enhanced LRC format
            let _ = write!(output, "[{timestamp}]");
            for word in words {
                let _ = write!(
                    output,
                    " <{}> {}",
                    format_timestamp(word.start_time),
                    word.text
                );
            }
            output.push('\n');
        } else {
            // Simple LRC format
            let _ = writeln!(output, "[{timestamp}]{}", line.text);
        }
    }

    output
}

/// Format a duration as LRC timestamp (mm:ss.xx)
fn format_timestamp(duration: std::time::Duration) -> String {
    let total_secs = duration.as_secs();
    let minutes = total_secs / 60;
    let seconds = total_secs % 60;
    let hundredths = duration.subsec_millis() / 10;

    format!("{minutes:02}:{seconds:02}.{hundredths:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lrc::{LrcLine, LrcMetadata, LrcWord};

    #[test]
    fn test_lyrics_type_as_str() {
        assert_eq!(LyricsType::Synced.as_str(), "synced");
        assert_eq!(LyricsType::Unsynced.as_str(), "unsynced");
    }

    #[test]
    fn test_lyrics_type_from_str() {
        assert_eq!(LyricsType::from_str("synced"), Some(LyricsType::Synced));
        assert_eq!(LyricsType::from_str("unsynced"), Some(LyricsType::Unsynced));
        assert_eq!(LyricsType::from_str("unknown"), None);
        assert_eq!(LyricsType::from_str(""), None);
    }

    #[test]
    fn test_format_timestamp_basic() {
        use std::time::Duration;

        // 12 seconds, 340 milliseconds
        let duration = Duration::from_millis(12340);
        assert_eq!(format_timestamp(duration), "00:12.34");
    }

    #[test]
    fn test_format_timestamp_with_minutes() {
        use std::time::Duration;

        // 1 minute, 30 seconds
        let duration = Duration::from_secs(90);
        assert_eq!(format_timestamp(duration), "01:30.00");
    }

    #[test]
    fn test_format_timestamp_zero() {
        use std::time::Duration;

        let duration = Duration::ZERO;
        assert_eq!(format_timestamp(duration), "00:00.00");
    }

    #[test]
    fn test_format_timestamp_long_duration() {
        use std::time::Duration;

        // 5 minutes, 45 seconds, 670 ms
        let duration = Duration::from_millis(5 * 60 * 1000 + 45 * 1000 + 670);
        assert_eq!(format_timestamp(duration), "05:45.67");
    }

    #[test]
    fn test_serialize_lrc_simple() {
        use std::time::Duration;

        let lrc = LrcFile {
            metadata: LrcMetadata::default(),
            lines: vec![
                LrcLine {
                    start_time: Duration::from_millis(5000),
                    text: "Hello world".to_string(),
                    words: None,
                },
                LrcLine {
                    start_time: Duration::from_millis(10000),
                    text: "Second line".to_string(),
                    words: None,
                },
            ],
        };

        let serialized = serialize_lrc(&lrc);
        assert!(serialized.contains("[00:05.00]Hello world"));
        assert!(serialized.contains("[00:10.00]Second line"));
    }

    #[test]
    fn test_serialize_lrc_with_metadata() {
        use std::time::Duration;

        let lrc = LrcFile {
            metadata: LrcMetadata {
                title: Some("Test Song".to_string()),
                artist: Some("Test Artist".to_string()),
                album: Some("Test Album".to_string()),
                offset: 0,
                ..Default::default()
            },
            lines: vec![LrcLine {
                start_time: Duration::from_millis(5000),
                text: "Lyrics here".to_string(),
                words: None,
            }],
        };

        let serialized = serialize_lrc(&lrc);
        assert!(serialized.contains("[ti:Test Song]"));
        assert!(serialized.contains("[ar:Test Artist]"));
        assert!(serialized.contains("[al:Test Album]"));
    }

    #[test]
    fn test_serialize_lrc_with_offset() {
        use std::time::Duration;

        let lrc = LrcFile {
            metadata: LrcMetadata {
                offset: 500,
                ..Default::default()
            },
            lines: vec![LrcLine {
                start_time: Duration::from_millis(5000),
                text: "Test".to_string(),
                words: None,
            }],
        };

        let serialized = serialize_lrc(&lrc);
        assert!(serialized.contains("[offset:500]"));
    }

    #[test]
    fn test_serialize_lrc_enhanced_format() {
        use std::time::Duration;

        let lrc = LrcFile {
            metadata: LrcMetadata::default(),
            lines: vec![LrcLine {
                start_time: Duration::from_millis(5000),
                text: "Hello world".to_string(),
                words: Some(vec![
                    LrcWord {
                        start_time: Duration::from_millis(5000),
                        end_time: Some(Duration::from_millis(5500)),
                        text: "Hello".to_string(),
                    },
                    LrcWord {
                        start_time: Duration::from_millis(5500),
                        end_time: Some(Duration::from_millis(6000)),
                        text: "world".to_string(),
                    },
                ]),
            }],
        };

        let serialized = serialize_lrc(&lrc);
        assert!(serialized.contains("[00:05.00]"));
        assert!(serialized.contains("<00:05.00>"));
        assert!(serialized.contains("Hello"));
        assert!(serialized.contains("<00:05.50>"));
        assert!(serialized.contains("world"));
    }

    #[test]
    fn test_cached_lyrics_to_lyrics_result_synced() {
        use chrono::Utc;

        let cached = CachedLyrics {
            id: 1,
            artist: "Artist".to_string(),
            track: "Track".to_string(),
            album: Some("Album".to_string()),
            duration_ms: Some(180000),
            provider: "lrclib".to_string(),
            provider_id: "123".to_string(),
            lyrics_type: LyricsType::Synced,
            content: "[00:05.00]Test lyrics".to_string(),
            fetched_at: Utc::now(),
        };

        let result = cached.to_lyrics_result();
        assert!(result.is_synced());
        assert!(result.is_found());
    }

    #[test]
    fn test_cached_lyrics_to_lyrics_result_unsynced() {
        use chrono::Utc;

        let cached = CachedLyrics {
            id: 1,
            artist: "Artist".to_string(),
            track: "Track".to_string(),
            album: None,
            duration_ms: None,
            provider: "lrclib".to_string(),
            provider_id: "123".to_string(),
            lyrics_type: LyricsType::Unsynced,
            content: "Plain text lyrics".to_string(),
            fetched_at: Utc::now(),
        };

        let result = cached.to_lyrics_result();
        assert!(!result.is_synced());
        assert!(result.is_found());
        assert_eq!(result.text(), Some("Plain text lyrics".to_string()));
    }
}
