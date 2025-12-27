use crate::error::VersualizerError;
use crate::lrc::{LrcFile, LrcLine, LrcMetadata};
use crate::provider::{FetchedLyrics, LyricsProvider, LyricsQuery, LyricsResult};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;
use tracing::{info, warn};

const LOG_TARGET: &str = "versualizer::provider::spotify_lyrics";
const SPOTIFY_LYRICS_API: &str = "https://spclient.wg.spotify.com/color-lyrics/v2/track";

/// Spotify unofficial lyrics provider
///
/// **WARNING:** This uses an unofficial Spotify API that requires the SP_DC cookie
/// from a logged-in Spotify web session. This may violate Spotify's Terms of Service.
/// Use at your own risk.
pub struct SpotifyLyricsProvider {
    sp_dc: String,
    client: Client,
}

impl SpotifyLyricsProvider {
    /// Create a new Spotify lyrics provider
    pub fn new(sp_dc: impl Into<String>) -> Self {
        Self {
            sp_dc: sp_dc.into(),
            client: Client::builder()
                .build()
                .expect("Failed to create HTTP client"),
        }
    }

    /// Check if SP_DC cookie is configured
    pub fn is_configured(&self) -> bool {
        !self.sp_dc.is_empty()
    }

    /// Extract track ID from Spotify URI or URL
    fn extract_track_id(id: &str) -> Option<&str> {
        // Handle various formats:
        // - spotify:track:4iV5W9uYEdYUVa79Axb7Rh
        // - https://open.spotify.com/track/4iV5W9uYEdYUVa79Axb7Rh
        // - 4iV5W9uYEdYUVa79Axb7Rh

        if id.starts_with("spotify:track:") {
            return Some(&id[14..]);
        }

        if id.contains("open.spotify.com/track/") {
            let parts: Vec<&str> = id.split("/track/").collect();
            if parts.len() >= 2 {
                // Remove any query parameters
                return Some(parts[1].split('?').next().unwrap_or(parts[1]));
            }
        }

        // Assume it's already a track ID if it's 22 chars (base62)
        if id.len() == 22 && id.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Some(id);
        }

        None
    }
}

#[derive(Debug, Deserialize)]
struct SpotifyLyricsResponse {
    lyrics: SpotifyLyrics,
}

#[derive(Debug, Deserialize)]
struct SpotifyLyrics {
    #[serde(rename = "syncType")]
    sync_type: String,
    lines: Vec<SpotifyLyricsLine>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct SpotifyLyricsLine {
    #[serde(rename = "startTimeMs")]
    start_time_ms: String,
    words: String,
    #[serde(rename = "endTimeMs")]
    end_time_ms: Option<String>,
}

#[async_trait]
impl LyricsProvider for SpotifyLyricsProvider {
    fn name(&self) -> &'static str {
        "spotify_lyrics"
    }

    async fn fetch(&self, query: &LyricsQuery) -> Result<FetchedLyrics, VersualizerError> {
        if !self.is_configured() {
            return Err(VersualizerError::LyricsProviderFailed {
                provider: self.name().to_string(),
                reason: "SP_DC cookie not configured".to_string(),
            });
        }

        let track_id = match &query.spotify_track_id {
            Some(id) => match Self::extract_track_id(id) {
                Some(extracted) => extracted.to_string(),
                None => {
                    return Err(VersualizerError::LyricsProviderFailed {
                        provider: self.name().to_string(),
                        reason: format!("Invalid Spotify track ID: {}", id),
                    });
                }
            },
            None => {
                return Err(VersualizerError::LyricsProviderFailed {
                    provider: self.name().to_string(),
                    reason: "Spotify track ID required for Spotify lyrics".to_string(),
                });
            }
        };

        info!(target: LOG_TARGET, "Fetching Spotify lyrics for track: {}", track_id);

        let url = format!("{}/{}?format=json&market=from_token", SPOTIFY_LYRICS_API, track_id);

        let response = self
            .client
            .get(&url)
            .header("Cookie", format!("sp_dc={}", self.sp_dc))
            .header("App-Platform", "WebPlayer")
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
            .send()
            .await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            info!(target: LOG_TARGET, "No Spotify lyrics found for track: {}", track_id);
            return Ok(FetchedLyrics {
                result: LyricsResult::NotFound,
                provider_id: track_id,
            });
        }

        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            warn!(target: LOG_TARGET, "SP_DC cookie expired or invalid (401 Unauthorized)");
            return Err(VersualizerError::LyricsProviderFailed {
                provider: self.name().to_string(),
                reason: "SP_DC cookie expired or invalid".to_string(),
            });
        }

        if !response.status().is_success() {
            warn!(target: LOG_TARGET, "Spotify lyrics API returned status: {}", response.status());
            return Err(VersualizerError::LyricsProviderFailed {
                provider: self.name().to_string(),
                reason: format!("Spotify lyrics API returned status: {}", response.status()),
            });
        }

        let result: SpotifyLyricsResponse = response.json().await?;

        match result.lyrics.sync_type.as_str() {
            "LINE_SYNCED" | "SYLLABLE_SYNCED" => {
                let lines: Vec<LrcLine> = result
                    .lyrics
                    .lines
                    .into_iter()
                    .filter(|line| !line.words.is_empty() && line.words != "♪")
                    .map(|line| {
                        let start_ms: u64 = line.start_time_ms.parse().unwrap_or(0);
                        LrcLine {
                            start_time: Duration::from_millis(start_ms),
                            text: line.words,
                            words: None, // Spotify doesn't provide word-level timing in this API
                        }
                    })
                    .collect();

                if lines.is_empty() {
                    return Ok(FetchedLyrics {
                        result: LyricsResult::NotFound,
                        provider_id: track_id,
                    });
                }

                let lrc = LrcFile {
                    metadata: LrcMetadata {
                        title: Some(query.track_name.clone()),
                        artist: Some(query.artist_name.clone()),
                        album: query.album_name.clone(),
                        ..Default::default()
                    },
                    lines,
                };

                info!(target: LOG_TARGET, "Got Spotify synced lyrics with {} lines", lrc.lines.len());
                Ok(FetchedLyrics {
                    result: LyricsResult::Synced(lrc),
                    provider_id: track_id,
                })
            }
            "UNSYNCED" => {
                let text: String = result
                    .lyrics
                    .lines
                    .iter()
                    .filter(|line| !line.words.is_empty() && line.words != "♪")
                    .map(|line| line.words.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");

                if text.is_empty() {
                    return Ok(FetchedLyrics {
                        result: LyricsResult::NotFound,
                        provider_id: track_id,
                    });
                }

                info!(target: LOG_TARGET, "Got Spotify unsynced lyrics");
                Ok(FetchedLyrics {
                    result: LyricsResult::Unsynced(text),
                    provider_id: track_id,
                })
            }
            _ => {
                warn!(target: LOG_TARGET, "Unknown Spotify sync type: {}", result.lyrics.sync_type);
                Ok(FetchedLyrics {
                    result: LyricsResult::NotFound,
                    provider_id: track_id,
                })
            }
        }
    }
}
