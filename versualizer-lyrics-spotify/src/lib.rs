//! Spotify unofficial lyrics provider using TOTP-based authentication.
//!
//! **WARNING:** This uses an unofficial Spotify API that requires the `sp_dc` cookie
//! from a logged-in Spotify web session. This may violate Spotify's Terms of Service.
//! Use at your own risk.

mod auth;
mod token_manager;
mod totp;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::{policies::ExponentialBackoff, RetryTransientMiddleware};
use serde::Deserialize;
use tracing::{info, warn};
use versualizer_core::{
    CoreError, FetchedLyrics, LrcFile, LrcLine, LrcMetadata, LyricsProvider, LyricsQuery,
    LyricsResult,
};
use versualizer_spotify_api::config::DEFAULT_SECRET_KEY_URL;

use token_manager::SpotifyTokenManager;

const SPOTIFY_LYRICS_API: &str = "https://spclient.wg.spotify.com/color-lyrics/v2/track";

/// Default timeout for HTTP requests (10 seconds)
const DEFAULT_TIMEOUT_SECS: u64 = 10;
/// Default number of retry attempts
const DEFAULT_MAX_RETRIES: u32 = 3;
/// User agent for requests
const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";

/// Spotify unofficial lyrics provider using TOTP-based authentication.
///
/// **WARNING:** This uses an unofficial Spotify API that requires the `sp_dc` cookie
/// from a logged-in Spotify web session. This may violate Spotify's Terms of Service.
/// Use at your own risk.
pub struct SpotifyLyricsProvider {
    token_manager: Arc<SpotifyTokenManager>,
    client: ClientWithMiddleware,
    configured: bool,
}

impl SpotifyLyricsProvider {
    /// Create a new Spotify lyrics provider with default 10-second timeout and 3 retries.
    ///
    /// # Arguments
    ///
    /// * `sp_dc` - The Spotify `sp_dc` cookie value
    /// * `secret_key_url` - Optional custom URL for fetching secret keys (uses default if `None`)
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be created.
    pub fn new(
        sp_dc: impl Into<String>,
        secret_key_url: Option<String>,
    ) -> Result<Self, CoreError> {
        let sp_dc = sp_dc.into();
        let configured = !sp_dc.is_empty();

        if configured {
            warn!(
                "SpotifyLyricsProvider enabled. WARNING: This uses an unofficial Spotify API \
                 that may violate Spotify's Terms of Service. Use at your own risk."
            );
        }

        // Base client with timeout
        let base_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .connect_timeout(Duration::from_secs(5))
            .build()?;

        // Create token manager with the base client
        let secret_url = secret_key_url.unwrap_or_else(|| DEFAULT_SECRET_KEY_URL.to_string());
        let token_manager = Arc::new(SpotifyTokenManager::new(sp_dc, secret_url, base_client.clone()));

        // Wrap with retry middleware (exponential backoff) for lyrics requests
        let retry_policy =
            ExponentialBackoff::builder().build_with_max_retries(DEFAULT_MAX_RETRIES);
        let client = ClientBuilder::new(base_client)
            .with(RetryTransientMiddleware::new_with_policy(retry_policy))
            .build();

        Ok(Self {
            token_manager,
            client,
            configured,
        })
    }

    /// Check if `sp_dc` cookie is configured
    #[must_use]
    pub const fn is_configured(&self) -> bool {
        self.configured
    }

    /// Extract track ID from Spotify URI or URL
    fn extract_track_id(id: &str) -> Option<&str> {
        // Handle various formats:
        // - spotify:track:4iV5W9uYEdYUVa79Axb7Rh
        // - https://open.spotify.com/track/4iV5W9uYEdYUVa79Axb7Rh
        // - 4iV5W9uYEdYUVa79Axb7Rh

        if let Some(stripped) = id.strip_prefix("spotify:track:") {
            return Some(stripped);
        }

        if id.contains("open.spotify.com/track/") {
            let parts: Vec<&str> = id.split("/track/").collect();
            if parts.len() >= 2 {
                // Remove any query parameters
                return parts[1].split('?').next();
            }
        }

        // Assume it's already a track ID if it's 22 chars (base62)
        if id.len() == 22 && id.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Some(id);
        }

        None
    }

    /// Validate query and extract track ID
    fn validate_query(&self, query: &LyricsQuery) -> Result<String, CoreError> {
        if !self.is_configured() {
            return Err(CoreError::LyricsProviderFailed {
                provider: self.name().to_string(),
                reason: "sp_dc cookie not configured".into(),
            });
        }

        query.spotify_track_id().map_or_else(
            || {
                Err(CoreError::LyricsProviderFailed {
                    provider: self.name().to_string(),
                    reason: "Spotify track ID required for Spotify lyrics".into(),
                })
            },
            |id| {
                Self::extract_track_id(id).map(String::from).ok_or_else(|| {
                    CoreError::LyricsProviderFailed {
                        provider: self.name().to_string(),
                        reason: format!("Invalid Spotify track ID: {id}"),
                    }
                })
            },
        )
    }

    /// Send request to Spotify lyrics API using Bearer token authentication.
    async fn send_request(&self, track_id: &str) -> Result<reqwest::Response, CoreError> {
        // Get valid access token (refreshes if needed)
        let access_token = self
            .token_manager
            .get_access_token()
            .await
            .map_err(|e| CoreError::LyricsProviderFailed {
                provider: self.name().to_string(),
                reason: e.to_string(),
            })?;

        let url = format!("{SPOTIFY_LYRICS_API}/{track_id}?format=json&market=from_token");
        info!("Spotify GET: {}", url);

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {access_token}"))
            .header("App-Platform", "WebPlayer")
            .header("User-Agent", USER_AGENT)
            .send()
            .await?;

        info!("Spotify response status: {}", response.status());
        Ok(response)
    }

    /// Check if response indicates not found (404).
    /// Returns `Some(FetchedLyrics)` with `NotFound` result if 404, `None` otherwise.
    fn check_not_found(response: &reqwest::Response, track_id: &str) -> Option<FetchedLyrics> {
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            info!("No Spotify lyrics found for track: {}", track_id);
            return Some(FetchedLyrics {
                result: LyricsResult::NotFound,
                provider_id: track_id.to_string(),
            });
        }
        None
    }

    /// Check for authentication errors and handle token refresh on 401.
    async fn check_auth_error(&self, response: &reqwest::Response) -> Result<(), CoreError> {
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            warn!("Received 401 Unauthorized - invalidating cached token");
            self.token_manager.invalidate_token().await;
            return Err(CoreError::LyricsProviderFailed {
                provider: self.name().to_string(),
                reason: "Authentication failed - token may have expired".into(),
            });
        }

        if !response.status().is_success() {
            warn!("Spotify lyrics API returned status: {}", response.status());
            return Err(CoreError::LyricsProviderFailed {
                provider: self.name().to_string(),
                reason: format!("Spotify lyrics API returned status: {}", response.status()),
            });
        }
        Ok(())
    }

    /// Parse synced lyrics from Spotify response
    fn parse_synced_lyrics(
        lyrics: SpotifyLyrics,
        query: &LyricsQuery,
        track_id: String,
    ) -> FetchedLyrics {
        let lines: Vec<LrcLine> = lyrics
            .lines
            .into_iter()
            .filter(|line| !line.words.is_empty() && line.words != "♪")
            .map(|line| LrcLine {
                start_time: Duration::from_millis(line.start_time_ms.parse().unwrap_or(0)),
                text: line.words,
                words: None,
            })
            .collect();

        if lines.is_empty() {
            return FetchedLyrics {
                result: LyricsResult::NotFound,
                provider_id: track_id,
            };
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

        info!("Got Spotify synced lyrics with {} lines", lrc.lines.len());
        FetchedLyrics {
            result: LyricsResult::Synced(lrc),
            provider_id: track_id,
        }
    }

    /// Parse unsynced lyrics from Spotify response
    fn parse_unsynced_lyrics(lyrics: &SpotifyLyrics, track_id: String) -> FetchedLyrics {
        let text: String = lyrics
            .lines
            .iter()
            .filter(|line| !line.words.is_empty() && line.words != "♪")
            .map(|line| line.words.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        if text.is_empty() {
            return FetchedLyrics {
                result: LyricsResult::NotFound,
                provider_id: track_id,
            };
        }

        info!("Got Spotify unsynced lyrics");
        FetchedLyrics {
            result: LyricsResult::Unsynced(text),
            provider_id: track_id,
        }
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
struct SpotifyLyricsLine {
    #[serde(rename = "startTimeMs")]
    start_time_ms: String,
    words: String,
    // Note: end_time_ms exists in API response but is unused; serde ignores unknown fields by default
}

#[async_trait]
impl LyricsProvider for SpotifyLyricsProvider {
    fn name(&self) -> &'static str {
        "spotify_lyrics"
    }

    async fn fetch(&self, query: &LyricsQuery) -> Result<FetchedLyrics, CoreError> {
        let track_id = self.validate_query(query)?;
        let response = self.send_request(&track_id).await?;

        // Handle 404 (not found) - return early with NotFound result
        if let Some(not_found) = Self::check_not_found(&response, &track_id) {
            return Ok(not_found);
        }

        // Check for auth errors and other failures
        self.check_auth_error(&response).await?;

        let result: SpotifyLyricsResponse = response.json().await?;

        Ok(match result.lyrics.sync_type.as_str() {
            "LINE_SYNCED" | "SYLLABLE_SYNCED" => {
                Self::parse_synced_lyrics(result.lyrics, query, track_id)
            }
            "UNSYNCED" => Self::parse_unsynced_lyrics(&result.lyrics, track_id),
            _ => {
                warn!("Unknown Spotify sync type: {}", result.lyrics.sync_type);
                FetchedLyrics {
                    result: LyricsResult::NotFound,
                    provider_id: track_id,
                }
            }
        })
    }
}
