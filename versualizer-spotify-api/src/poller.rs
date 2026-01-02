//! Spotify playback state polling.

use crate::error::SpotifyError;
use crate::oauth::SpotifyOAuth;
use async_trait::async_trait;
use rspotify::prelude::*;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};
use versualizer_core::{
    CoreError, DurationExt, MusicSource, MusicSourceProvider, PlaybackState, SyncEngine, TrackInfo,
};

/// Spotify playback state poller implementing [`MusicSourceProvider`].
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

    /// Start polling in a background task
    #[must_use]
    pub fn start(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            if let Err(e) = self.run().await {
                error!("Spotify poller stopped with error: {}", e);
            }
        })
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
                    let info = TrackInfo::new(
                        MusicSource::Spotify,
                        &track_id,
                        &track.name,
                        artists,
                        &track.album.name,
                        dur,
                    )
                    // Also add the track ID under "spotify" for lyrics providers
                    .with_provider_id("spotify", &track_id);
                    (Some(info), dur)
                }
                Some(rspotify::model::PlayableItem::Episode(episode)) => {
                    let dur = episode.duration.to_std().unwrap_or(Duration::ZERO);
                    // Use just the ID part, not the full URI
                    let episode_id = episode.id.id().to_string();
                    let info = TrackInfo::new(
                        MusicSource::Spotify,
                        &episode_id,
                        &episode.name,
                        &episode.show.name,
                        "Podcast",
                        dur,
                    )
                    .with_provider_id("spotify", &episode_id);
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
            // No active playback - SyncEngine will emit PlaybackStopped event
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

#[async_trait]
impl MusicSourceProvider for SpotifyPoller {
    fn source(&self) -> MusicSource {
        MusicSource::Spotify
    }

    fn name(&self) -> &'static str {
        "spotify"
    }

    fn cancel_token(&self) -> CancellationToken {
        self.cancel_token.clone()
    }

    async fn run(&self) -> Result<(), CoreError> {
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
                            if matches!(e, SpotifyError::Api(_))
                                && let Err(refresh_err) = self.oauth.refresh_token().await
                            {
                                error!("Token refresh failed: {}", refresh_err);
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
