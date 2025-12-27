use crate::error::CoreError;
use crate::lrc::LrcFile;
use crate::provider::{FetchedLyrics, LyricsProvider, LyricsQuery, LyricsResult};
use async_trait::async_trait;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::{policies::ExponentialBackoff, RetryTransientMiddleware};
use serde::Deserialize;
use std::time::Duration;
use tracing::{debug, info, warn};

const LOG_TARGET: &str = "versualizer::provider::lrclib";
const LRCLIB_API_URL: &str = "https://lrclib.net/api";

/// Default timeout for HTTP requests (10 seconds)
const DEFAULT_TIMEOUT_SECS: u64 = 10;
/// Default number of retry attempts
const DEFAULT_MAX_RETRIES: u32 = 3;

/// LRCLIB.net lyrics provider
pub struct LrclibProvider {
    client: ClientWithMiddleware,
}

impl LrclibProvider {
    /// Create a new LRCLIB provider with default 10-second timeout and 3 retries.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be created.
    pub fn new() -> Result<Self, CoreError> {
        // Base client with timeout
        let base_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .connect_timeout(Duration::from_secs(5))
            .user_agent("Versualizer/1.0 (https://github.com/versualizer)")
            .build()?;

        // Wrap with retry middleware (exponential backoff)
        let retry_policy = ExponentialBackoff::builder().build_with_max_retries(DEFAULT_MAX_RETRIES);
        let client = ClientBuilder::new(base_client)
            .with(RetryTransientMiddleware::new_with_policy(retry_policy))
            .build();

        Ok(Self { client })
    }
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct LrclibResponse {
    id: i64,
    #[serde(rename = "trackName")]
    track_name: String,
    #[serde(rename = "artistName")]
    artist_name: String,
    #[serde(rename = "albumName")]
    album_name: Option<String>,
    duration: Option<f64>,
    instrumental: bool,
    #[serde(rename = "plainLyrics")]
    plain_lyrics: Option<String>,
    #[serde(rename = "syncedLyrics")]
    synced_lyrics: Option<String>,
}

#[async_trait]
impl LyricsProvider for LrclibProvider {
    fn name(&self) -> &'static str {
        "lrclib"
    }

    async fn fetch(&self, query: &LyricsQuery) -> Result<FetchedLyrics, CoreError> {
        info!(
            target: LOG_TARGET,
            "Fetching lyrics from LRCLIB for: {} - {} (duration: {:?}s)",
            query.artist_name, query.track_name, query.duration_secs
        );

        // Try the /get endpoint first for exact match with artist + track + album + duration
        let mut url = format!(
            "{}/get?artist_name={}&track_name={}",
            LRCLIB_API_URL,
            urlencoding::encode(&query.artist_name),
            urlencoding::encode(&query.track_name)
        );

        if let Some(ref album) = query.album_name {
            use std::fmt::Write;
            let _ = write!(url, "&album_name={}", urlencoding::encode(album));
        }

        if let Some(duration) = query.duration_secs {
            use std::fmt::Write;
            let _ = write!(url, "&duration={duration}");
        }

        debug!(target: LOG_TARGET, "LRCLIB request URL (exact): {}", url);

        let response = self.client.get(&url).send().await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            info!(target: LOG_TARGET, "LRCLIB exact match not found, trying search by track name only");
            // Try searching with just track name and match duration manually
            return self.search_by_track_name(query).await;
        }

        if !response.status().is_success() {
            warn!(target: LOG_TARGET, "LRCLIB returned status: {}", response.status());
            return Err(CoreError::LyricsProviderFailed {
                provider: self.name().to_string(),
                reason: format!("LRCLIB returned status: {}", response.status()),
            });
        }

        let result: LrclibResponse = response.json().await?;
        info!(target: LOG_TARGET, "LRCLIB found match with id: {}", result.id);
        Ok(Self::parse_response(result))
    }
}

/// Duration tolerance for matching (±2 seconds)
const DURATION_TOLERANCE_SECS: f64 = 2.0;

impl LrclibProvider {
    /// Search by track name only and match duration within ±2 seconds
    async fn search_by_track_name(
        &self,
        query: &LyricsQuery,
    ) -> Result<FetchedLyrics, CoreError> {
        // Search with just track name
        let url = format!(
            "{}/search?track_name={}",
            LRCLIB_API_URL,
            urlencoding::encode(&query.track_name)
        );

        debug!(target: LOG_TARGET, "LRCLIB request URL (search by track name): {}", url);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            warn!(target: LOG_TARGET, "LRCLIB search returned status: {}", response.status());
            // Fall back to full search with artist + track
            return self.search_fallback(query).await;
        }

        let results: Vec<LrclibResponse> = response.json().await?;

        if results.is_empty() {
            info!(target: LOG_TARGET, "LRCLIB search by track name returned no results, trying full search");
            return self.search_fallback(query).await;
        }

        // Filter by duration (±2 seconds) if we have a query duration
        let filtered: Vec<_> = if let Some(query_duration) = query.duration_secs {
            let query_duration = f64::from(query_duration);
            results
                .into_iter()
                .filter(|r| {
                    r.duration
                        .is_some_and(|d| (d - query_duration).abs() <= DURATION_TOLERANCE_SECS)
                })
                .collect()
        } else {
            results
        };

        if filtered.is_empty() {
            info!(target: LOG_TARGET, "LRCLIB search by track name: no results within duration tolerance, trying full search");
            return self.search_fallback(query).await;
        }

        // Find the best match (prefer synced lyrics, then closest duration)
        #[allow(clippy::cast_possible_truncation)]
        let best = filtered
            .into_iter()
            .filter(|r| r.synced_lyrics.is_some() || r.plain_lyrics.is_some())
            .min_by_key(|r| {
                // Prefer synced, then by duration match
                let sync_score = if r.synced_lyrics.is_some() { 0 } else { 100 };
                let duration_score = match (r.duration, query.duration_secs) {
                    (Some(d), Some(q)) => ((d - f64::from(q)).abs() * 10.0) as i32,
                    _ => 50,
                };
                sync_score + duration_score
            });

        if let Some(result) = best {
            info!(
                target: LOG_TARGET,
                "LRCLIB found match by track name + duration (id: {}, artist: {}, duration: {:?})",
                result.id, result.artist_name, result.duration
            );
            Ok(Self::parse_response(result))
        } else {
            info!(target: LOG_TARGET, "LRCLIB search by track name: no usable lyrics, trying full search");
            self.search_fallback(query).await
        }
    }

    async fn search_fallback(&self, query: &LyricsQuery) -> Result<FetchedLyrics, CoreError> {
        info!(target: LOG_TARGET, "Trying LRCLIB search endpoint with artist + track as final fallback");

        let search_query = format!("{} {}", query.artist_name, query.track_name);
        let url = format!(
            "{}/search?q={}",
            LRCLIB_API_URL,
            urlencoding::encode(&search_query)
        );

        debug!(target: LOG_TARGET, "LRCLIB request URL (full search): {}", url);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            return Err(CoreError::LyricsProviderFailed {
                provider: self.name().to_string(),
                reason: format!("LRCLIB search returned status: {}", response.status()),
            });
        }

        let results: Vec<LrclibResponse> = response.json().await?;

        if results.is_empty() {
            return Err(CoreError::LyricsNotFound {
                track: query.track_name.clone(),
                artist: query.artist_name.clone(),
            });
        }

        // Find the best match (prefer synced lyrics, then closest duration)
        #[allow(clippy::cast_possible_truncation)]
        let best = results
            .into_iter()
            .filter(|r| r.synced_lyrics.is_some() || r.plain_lyrics.is_some())
            .min_by_key(|r| {
                // Prefer synced, then by duration match
                let sync_score = if r.synced_lyrics.is_some() { 0 } else { 100 };
                let duration_score = match (r.duration, query.duration_secs) {
                    (Some(d), Some(q)) => (d - f64::from(q)).abs() as i32,
                    _ => 50,
                };
                sync_score + duration_score
            });

        match best {
            Some(result) => {
                info!(
                    target: LOG_TARGET,
                    "LRCLIB found match via full search (id: {}, artist: {})",
                    result.id, result.artist_name
                );
                Ok(Self::parse_response(result))
            }
            None => Err(CoreError::LyricsNotFound {
                track: query.track_name.clone(),
                artist: query.artist_name.clone(),
            }),
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    fn parse_response(result: LrclibResponse) -> FetchedLyrics {
        let provider_id = result.id.to_string();

        if result.instrumental {
            debug!(target: LOG_TARGET, "Track is instrumental (lrclib id: {})", result.id);
            return FetchedLyrics {
                result: LyricsResult::NotFound,
                provider_id,
            };
        }

        // Prefer synced lyrics
        if let Some(synced) = result.synced_lyrics {
            if !synced.trim().is_empty() {
                match LrcFile::parse(&synced) {
                    Ok(lrc) => {
                        debug!(target: LOG_TARGET, "Got synced lyrics with {} lines (lrclib id: {})", lrc.lines.len(), result.id);
                        return FetchedLyrics {
                            result: LyricsResult::Synced(lrc),
                            provider_id,
                        };
                    }
                    Err(e) => {
                        warn!(target: LOG_TARGET, "Failed to parse synced lyrics: {}", e);
                    }
                }
            }
        }

        // Fall back to plain lyrics
        if let Some(plain) = result.plain_lyrics {
            if !plain.trim().is_empty() {
                debug!(target: LOG_TARGET, "Got plain lyrics (lrclib id: {})", result.id);
                return FetchedLyrics {
                    result: LyricsResult::Unsynced(plain),
                    provider_id,
                };
            }
        }

        FetchedLyrics {
            result: LyricsResult::NotFound,
            provider_id,
        }
    }
}
