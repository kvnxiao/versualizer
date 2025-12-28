//! Lyrics fetcher that orchestrates multiple lyrics providers.

use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::cache::{LyricsCache, TrackMetadata};
use crate::playback::TrackInfo;
use crate::provider::{LyricsProvider, LyricsQuery, LyricsResult};
use crate::sync::{SyncEngine, SyncEvent};
use crate::time::DurationExt;

/// Lyrics fetcher that listens for track changes and fetches lyrics
pub struct LyricsFetcher {
    sync_engine: Arc<SyncEngine>,
    cache: Arc<LyricsCache>,
    providers: Vec<Box<dyn LyricsProvider>>,
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
        cache: Arc<LyricsCache>,
        providers: Vec<Box<dyn LyricsProvider>>,
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
        info!("Initializing lyrics fetching handler");

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
                        Ok(SyncEvent::TrackChanged { track, .. } |
                           SyncEvent::PlaybackStarted { track, .. }) => {
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
            "Fetching lyrics for: {} - {} (source: {}, providers: {:?})",
            track.artist, track.name, track.source, provider_names
        );

        // Check cache first using source-specific ID
        if let Ok(Some(cached)) = self
            .cache
            .get_by_provider_id(track.source.as_str(), &track.source_track_id)
            .await
        {
            info!("Using cached lyrics for {}", track.name);
            if let LyricsResult::Synced(lrc) = cached.to_lyrics_result() {
                self.sync_engine.set_lyrics(lrc).await;
                return;
            }
        }

        // Build query with all provider IDs from track info
        let mut query = LyricsQuery::new(&track.name, &track.artist)
            .with_album(&track.album)
            .with_duration(track.duration_secs())
            .with_provider_id(track.source.as_str(), &track.source_track_id);

        // Copy additional provider IDs
        for (provider, id) in &track.provider_ids {
            query = query.with_provider_id(provider, id);
        }

        for provider in &self.providers {
            info!("Trying provider: {}", provider.name());
            match provider.fetch(&query).await {
                Ok(fetched) => {
                    match &fetched.result {
                        LyricsResult::Synced(lrc) => {
                            info!(
                                "Found synced lyrics from {} ({} lines, provider_id: {})",
                                provider.name(),
                                lrc.lines.len(),
                                fetched.provider_id
                            );

                            // Cache the result
                            let metadata = TrackMetadata {
                                artist: track.artist.clone(),
                                track: track.name.clone(),
                                album: Some(track.album.clone()),
                                duration_ms: Some(track.duration.as_millis_i64()),
                            };

                            if let Err(e) = self
                                .cache
                                .store(
                                    track.source.as_str(), // music source
                                    &track.source_track_id, // source-specific track ID
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
                        LyricsResult::Unsynced(_) => {
                            info!(
                                "Provider {} returned unsynced lyrics (not usable for karaoke)",
                                provider.name()
                            );
                            // Continue trying other providers for synced lyrics
                        }
                        LyricsResult::NotFound => {
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
