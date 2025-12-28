use crate::error::SpotifyError;
use crate::oauth::SpotifyOAuth;
use rspotify::prelude::*;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};
use versualizer_core::{DurationExt, PlaybackState, SyncEngine, TrackInfo};

/// Spotify playback state poller
pub struct SpotifyPoller {
    oauth: Arc<SpotifyOAuth>,
    sync_engine: Arc<SyncEngine>,
    poll_interval: Duration,
    cancel_token: CancellationToken,
}

impl SpotifyPoller {
    /// Create a new Spotify poller
    ///
    /// # Arguments
    /// * `oauth` - Spotify OAuth client
    /// * `sync_engine` - Sync engine to update with playback state
    /// * `poll_interval_ms` - Polling interval in milliseconds
    /// * `cancel_token` - Optional external cancellation token for graceful shutdown
    pub fn new(
        oauth: Arc<SpotifyOAuth>,
        sync_engine: Arc<SyncEngine>,
        poll_interval_ms: u64,
        cancel_token: Option<CancellationToken>,
    ) -> Self {
        Self {
            oauth,
            sync_engine,
            poll_interval: Duration::from_millis(poll_interval_ms),
            cancel_token: cancel_token.unwrap_or_default(),
        }
    }

    /// Get a clone of the cancellation token
    #[must_use]
    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel_token.clone()
    }

    /// Stop the poller
    pub fn stop(&self) {
        self.cancel_token.cancel();
    }

    /// Start polling in a background task
    #[must_use]
    pub fn start(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            self.run().await;
        })
    }

    /// Run the polling loop
    async fn run(&self) {
        info!("Starting Spotify playback poller");

        let mut consecutive_errors = 0;
        let max_backoff = Duration::from_secs(30);

        loop {
            tokio::select! {
                () = self.cancel_token.cancelled() => {
                    info!("Poller shutting down gracefully");
                    break;
                }
                () = tokio::time::sleep(self.poll_interval) => {
                    match self.poll_once().await {
                        Ok(()) => {
                            consecutive_errors = 0;
                        }
                        Err(e) => {
                            consecutive_errors += 1;
                            warn!("Poll error (attempt {}): {}", consecutive_errors, e);

                            // Exponential backoff: 100ms * 2^errors, capped at max_backoff
                            // consecutive_errors is capped at 10, so max is 100 * 2^10 = 102,400ms
                            let backoff_ms = 100_u64
                                .saturating_mul(2_u64.saturating_pow(consecutive_errors.min(10)));
                            let backoff = Duration::from_millis(backoff_ms.min(max_backoff.as_millis_u64()));

                            if consecutive_errors >= 5 {
                                error!("Too many consecutive errors, waiting {} seconds", backoff.as_secs());
                            }

                            tokio::time::sleep(backoff).await;

                            // Try to refresh token on auth errors
                            if matches!(e, SpotifyError::Api(_)) {
                                if let Err(refresh_err) = self.oauth.refresh_token().await {
                                    error!("Token refresh failed: {}", refresh_err);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Poll Spotify for current playback state
    async fn poll_once(&self) -> Result<(), SpotifyError> {
        // Proactively refresh token if it expires within 60 seconds
        self.oauth.ensure_token_fresh().await?;

        let request_start = Instant::now();

        let playback = self
            .oauth
            .client()
            .current_playback(None, None::<Vec<_>>)
            .await?;

        let request_latency = request_start.elapsed();

        let state = if let Some(context) = playback {
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
                    let track_id = track
                        .id
                        .as_ref()
                        .map(|id| id.id().to_string())
                        .unwrap_or_default();
                    let info =
                        TrackInfo::new(track_id, &track.name, artists, &track.album.name, dur);
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

            // Compensate for network latency
            // Assume position is from halfway through the request
            let latency_compensation = request_latency / 2;
            let position = context.progress.map_or(Duration::ZERO, |p| {
                p.to_std().unwrap_or(Duration::ZERO) + latency_compensation
            });

            PlaybackState::new(context.is_playing, track_info, position, duration)
        } else {
            // No active playback - this happens when no Spotify device is active
            info!("Spotify: no active playback");
            PlaybackState::default()
        };

        debug!(
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
    ///
    /// # Arguments
    /// * `sync_engine` - Sync engine to listen for track changes
    /// * `cache` - Lyrics cache for storing fetched lyrics
    /// * `providers` - List of lyrics providers to try in order
    /// * `cancel_token` - Optional external cancellation token for graceful shutdown
    pub fn new(
        sync_engine: Arc<SyncEngine>,
        cache: Arc<versualizer_core::cache::LyricsCache>,
        providers: Vec<Box<dyn versualizer_core::LyricsProvider>>,
        cancel_token: Option<CancellationToken>,
    ) -> Self {
        Self {
            sync_engine,
            cache,
            providers,
            cancel_token: cancel_token.unwrap_or_default(),
        }
    }

    /// Get a clone of the cancellation token
    #[must_use]
    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel_token.clone()
    }

    /// Start the lyrics fetcher in a background task
    #[must_use]
    pub fn start(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            self.run().await;
        })
    }

    /// Run the lyrics fetching loop
    async fn run(&self) {
        info!("Starting lyrics fetcher");

        let mut rx = self.sync_engine.subscribe();

        // Check if there's already a track loaded on startup
        if let Some(track) = self.sync_engine.current_track().await {
            if self.sync_engine.lyrics().await.is_none() {
                info!(
                    "Found existing track on startup: {} - {}, fetching lyrics",
                    track.artist, track.name
                );
                self.fetch_lyrics_for_track(&track).await;
            }
        }

        loop {
            tokio::select! {
                            () = self.cancel_token.cancelled() => {
                                info!("Lyrics fetcher shutting down");
                                break;
                            }
                            event = rx.recv() => {
                                match event {
                                    Ok(versualizer_core::SyncEvent::TrackChanged { track, .. } |
            versualizer_core::SyncEvent::PlaybackStarted { track, .. }) => {
                                        self.fetch_lyrics_for_track(&track).await;
                                    }
                                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                        break;
                                    }
                                    _ => {
                                        // Missed some events (Lagged) or other event types, continue
                                    }
                                }
                            }
                        }
        }
    }

    /// Fetch lyrics for a track
    async fn fetch_lyrics_for_track(&self, track: &TrackInfo) {
        let provider_names: Vec<_> = self.providers.iter().map(|p| p.name()).collect();
        info!(
            "Fetching lyrics for: {} - {} (providers: {:?})",
            track.artist, track.name, provider_names
        );

        // Check cache first
        if let Ok(Some(cached)) = self.cache.get_by_provider_id("spotify", &track.id).await {
            info!("Using cached lyrics for {}", track.name);
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
            info!("Trying provider: {}", provider.name());
            match provider.fetch(&query).await {
                Ok(fetched) => {
                    match &fetched.result {
                        versualizer_core::LyricsResult::Synced(lrc) => {
                            info!(
                                "Found synced lyrics from {} ({} lines, provider_id: {})",
                                provider.name(),
                                lrc.lines.len(),
                                fetched.provider_id
                            );

                            // Cache the result
                            let metadata = versualizer_core::cache::TrackMetadata {
                                artist: track.artist.clone(),
                                track: track.name.clone(),
                                album: Some(track.album.clone()),
                                duration_ms: Some(track.duration.as_millis_i64()),
                            };

                            if let Err(e) = self
                                .cache
                                .store(
                                    "spotify", // provider (music source)
                                    &track.id, // provider_track_id (clean ID without prefix)
                                    &fetched.result,
                                    &metadata,
                                    provider.name(), // lyrics_provider (lrclib, spotify_lyrics, etc.)
                                    &fetched.provider_id, // lyrics_provider_id
                                )
                                .await
                            {
                                warn!("Failed to cache lyrics: {}", e);
                            }

                            self.sync_engine.set_lyrics(lrc.clone()).await;
                            return;
                        }
                        versualizer_core::LyricsResult::Unsynced(_) => {
                            info!(
                                "Provider {} returned unsynced lyrics (not usable for karaoke)",
                                provider.name()
                            );
                            // Continue trying other providers for synced lyrics
                        }
                        versualizer_core::LyricsResult::NotFound => {
                            info!("Provider {} returned no lyrics", provider.name());
                        }
                    }
                }
                Err(e) => {
                    warn!("Provider {} failed with error: {}", provider.name(), e);
                }
            }
        }

        // No synced lyrics found
        info!(
            "No synced lyrics found for {} - {} (tried {} providers: {:?})",
            track.artist,
            track.name,
            self.providers.len(),
            provider_names
        );
        self.sync_engine.set_no_lyrics().await;
    }
}
