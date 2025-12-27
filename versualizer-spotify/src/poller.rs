use crate::oauth::SpotifyOAuth;
use rspotify::prelude::*;
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};
use versualizer_core::{PlaybackState, SyncEngine, TrackInfo};

const LOG_TARGET_POLLER: &str = "versualizer::spotify::poller";
const LOG_TARGET_FETCHER: &str = "versualizer::lyrics::fetcher";

#[derive(Debug, Error)]
pub enum PollerError {
    #[error("Spotify API error: {0}")]
    SpotifyApi(#[from] rspotify::ClientError),

    #[error("OAuth error: {0}")]
    OAuth(#[from] crate::oauth::OAuthError),

    #[error("Poller stopped")]
    Stopped,
}

/// Spotify playback state poller
pub struct SpotifyPoller {
    oauth: Arc<SpotifyOAuth>,
    sync_engine: Arc<SyncEngine>,
    poll_interval: Duration,
    cancel_token: CancellationToken,
}

impl SpotifyPoller {
    /// Create a new Spotify poller
    pub fn new(
        oauth: Arc<SpotifyOAuth>,
        sync_engine: Arc<SyncEngine>,
        poll_interval_ms: u64,
    ) -> Self {
        Self {
            oauth,
            sync_engine,
            poll_interval: Duration::from_millis(poll_interval_ms),
            cancel_token: CancellationToken::new(),
        }
    }

    /// Get a clone of the cancellation token
    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel_token.clone()
    }

    /// Stop the poller
    pub fn stop(&self) {
        self.cancel_token.cancel();
    }

    /// Start polling in a background task
    pub fn start(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            self.run().await;
        })
    }

    /// Run the polling loop
    async fn run(&self) {
        info!(target: LOG_TARGET_POLLER, "Starting Spotify playback poller");

        let mut consecutive_errors = 0;
        let max_backoff = Duration::from_secs(30);

        loop {
            tokio::select! {
                _ = self.cancel_token.cancelled() => {
                    info!(target: LOG_TARGET_POLLER, "Poller shutting down gracefully");
                    break;
                }
                _ = tokio::time::sleep(self.poll_interval) => {
                    match self.poll_once().await {
                        Ok(_) => {
                            consecutive_errors = 0;
                        }
                        Err(e) => {
                            consecutive_errors += 1;
                            warn!(target: LOG_TARGET_POLLER, "Poll error (attempt {}): {}", consecutive_errors, e);

                            // Exponential backoff
                            let backoff = Duration::from_millis(
                                (100 * (2_u64.pow(consecutive_errors.min(10)))).min(max_backoff.as_millis() as u64)
                            );

                            if consecutive_errors >= 5 {
                                error!(target: LOG_TARGET_POLLER, "Too many consecutive errors, waiting {} seconds", backoff.as_secs());
                            }

                            tokio::time::sleep(backoff).await;

                            // Try to refresh token on auth errors
                            if matches!(e, PollerError::SpotifyApi(_)) {
                                if let Err(refresh_err) = self.oauth.refresh_token().await {
                                    error!(target: LOG_TARGET_POLLER, "Token refresh failed: {}", refresh_err);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Poll Spotify for current playback state
    async fn poll_once(&self) -> Result<(), PollerError> {
        let request_start = Instant::now();

        let playback = self
            .oauth
            .client()
            .current_playback(None, None::<Vec<_>>)
            .await?;

        let request_latency = request_start.elapsed();

        let state = match playback {
            Some(context) => {
                // Extract track info and duration together to avoid borrow issues
                let (track_info, duration) = match &context.item {
                    Some(rspotify::model::PlayableItem::Track(track)) => {
                        let artists = track
                            .artists
                            .iter()
                            .map(|a| a.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ");

                        let dur = track.duration.to_std().unwrap_or(Duration::ZERO);
                        // Use just the ID part, not the full URI (spotify:track:xxx -> xxx)
                        let track_id = track.id.as_ref().map(|id| id.id().to_string()).unwrap_or_default();
                        let info = TrackInfo::new(
                            track_id,
                            &track.name,
                            artists,
                            &track.album.name,
                            dur,
                        );
                        (Some(info), dur)
                    }
                    Some(rspotify::model::PlayableItem::Episode(episode)) => {
                        let dur = episode.duration.to_std().unwrap_or(Duration::ZERO);
                        // Use just the ID part, not the full URI
                        let episode_id = episode.id.id().to_string();
                        let info = TrackInfo::new(
                            episode_id,
                            &episode.name,
                            &episode.show.name,
                            "Podcast",
                            dur,
                        );
                        (Some(info), dur)
                    }
                    None => (None, Duration::ZERO),
                };

                // Log full playback context when track changes
                let current_state = self.sync_engine.state().await;
                let track_changed = current_state.track_changed(&PlaybackState::new(
                    context.is_playing,
                    track_info.clone(),
                    Duration::ZERO,
                    duration,
                ));

                if track_changed {
                    info!(
                        target: LOG_TARGET_POLLER,
                        "Track changed - full Spotify playback context: {:#?}",
                        context
                    );
                }

                // Compensate for network latency
                // Assume position is from halfway through the request
                let latency_compensation = request_latency / 2;
                let position = context
                    .progress
                    .map(|p| p.to_std().unwrap_or(Duration::ZERO) + latency_compensation)
                    .unwrap_or(Duration::ZERO);

                PlaybackState::new(context.is_playing, track_info, position, duration)
            }
            None => {
                // No active playback
                PlaybackState::default()
            }
        };

        debug!(
            target: LOG_TARGET_POLLER,
            "Polled Spotify: playing={}, track={:?}, position={:?}",
            state.is_playing,
            state.track.as_ref().map(|t| &t.name),
            state.position
        );

        // Update sync engine
        self.sync_engine.update_state(state).await;

        Ok(())
    }
}

/// Lyrics fetcher that listens for track changes and fetches lyrics
pub struct LyricsFetcher {
    sync_engine: Arc<SyncEngine>,
    cache: Arc<versualizer_core::cache::LyricsCache>,
    providers: Vec<Box<dyn versualizer_core::LyricsProvider>>,
    cancel_token: CancellationToken,
}

impl LyricsFetcher {
    /// Create a new lyrics fetcher
    pub fn new(
        sync_engine: Arc<SyncEngine>,
        cache: Arc<versualizer_core::cache::LyricsCache>,
        providers: Vec<Box<dyn versualizer_core::LyricsProvider>>,
    ) -> Self {
        Self {
            sync_engine,
            cache,
            providers,
            cancel_token: CancellationToken::new(),
        }
    }

    /// Get a clone of the cancellation token
    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel_token.clone()
    }

    /// Start the lyrics fetcher in a background task
    pub fn start(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            self.run().await;
        })
    }

    /// Run the lyrics fetching loop
    async fn run(&self) {
        info!(target: LOG_TARGET_FETCHER, "Starting lyrics fetcher");

        let mut rx = self.sync_engine.subscribe();

        loop {
            tokio::select! {
                _ = self.cancel_token.cancelled() => {
                    info!(target: LOG_TARGET_FETCHER, "Lyrics fetcher shutting down");
                    break;
                }
                event = rx.recv() => {
                    match event {
                        Ok(versualizer_core::SyncEvent::TrackChanged { track, .. }) |
                        Ok(versualizer_core::SyncEvent::PlaybackStarted { track, .. }) => {
                            self.fetch_lyrics_for_track(&track).await;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            break;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                            // Missed some events, continue
                            continue;
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    /// Fetch lyrics for a track
    async fn fetch_lyrics_for_track(&self, track: &TrackInfo) {
        let provider_names: Vec<_> = self.providers.iter().map(|p| p.name()).collect();
        info!(
            target: LOG_TARGET_FETCHER,
            "Fetching lyrics for: {} - {} (providers: {:?})",
            track.artist, track.name, provider_names
        );

        // Check cache first
        if let Ok(Some(cached)) = self.cache.get_by_provider_id("spotify", &track.id).await {
            info!(target: LOG_TARGET_FETCHER, "Using cached lyrics for {}", track.name);
            if let versualizer_core::LyricsResult::Synced(lrc) = cached.to_lyrics_result() {
                self.sync_engine.set_lyrics(lrc).await;
                return;
            }
        }

        // Try providers in order
        let query = versualizer_core::LyricsQuery::new(&track.name, &track.artist)
            .with_album(&track.album)
            .with_duration(track.duration_secs())
            .with_spotify_id(&track.id);

        for provider in &self.providers {
            info!(target: LOG_TARGET_FETCHER, "Trying provider: {}", provider.name());
            match provider.fetch(&query).await {
                Ok(fetched) => {
                    match &fetched.result {
                        versualizer_core::LyricsResult::Synced(lrc) => {
                            info!(
                                target: LOG_TARGET_FETCHER,
                                "Found synced lyrics from {} ({} lines, provider_id: {})",
                                provider.name(), lrc.lines.len(), fetched.provider_id
                            );

                            // Cache the result
                            let metadata = versualizer_core::cache::TrackMetadata {
                                artist: track.artist.clone(),
                                track: track.name.clone(),
                                album: Some(track.album.clone()),
                                duration_ms: Some(track.duration.as_millis() as i64),
                            };

                            if let Err(e) = self
                                .cache
                                .store(
                                    "spotify",  // provider (music source)
                                    &track.id,  // provider_track_id (clean ID without prefix)
                                    &fetched.result,
                                    &metadata,
                                    provider.name(),  // lyrics_provider (lrclib, spotify_lyrics, etc.)
                                    &fetched.provider_id,  // lyrics_provider_id
                                )
                                .await
                            {
                                warn!(target: LOG_TARGET_FETCHER, "Failed to cache lyrics: {}", e);
                            }

                            self.sync_engine.set_lyrics(lrc.clone()).await;
                            return;
                        }
                        versualizer_core::LyricsResult::Unsynced(_) => {
                            info!(target: LOG_TARGET_FETCHER, "Provider {} returned unsynced lyrics (not usable for karaoke)", provider.name());
                            // Continue trying other providers for synced lyrics
                        }
                        versualizer_core::LyricsResult::NotFound => {
                            info!(target: LOG_TARGET_FETCHER, "Provider {} returned no lyrics", provider.name());
                        }
                    }
                }
                Err(e) => {
                    warn!(target: LOG_TARGET_FETCHER, "Provider {} failed with error: {}", provider.name(), e);
                }
            }
        }

        // No synced lyrics found
        info!(
            target: LOG_TARGET_FETCHER,
            "No synced lyrics found for {} - {} (tried {} providers: {:?})",
            track.artist, track.name, self.providers.len(), provider_names
        );
        self.sync_engine.set_no_lyrics().await;
    }
}
