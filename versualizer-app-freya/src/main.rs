mod app;
mod state;

use crate::state::{AppChannel, AppState};
use freya::prelude::*;
use freya::winit::window::WindowLevel;
use freya_radio::prelude::*;
use futures_channel::mpsc::unbounded;
use futures_lite::StreamExt;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use versualizer_core::config::LyricsProviderType;
use versualizer_core::providers::{LrclibProvider, SpotifyLyricsProvider};
use versualizer_core::{Config, LyricsCache, LyricsProvider, SyncEngine, SyncEvent};
use versualizer_spotify::{LyricsFetcher, SpotifyOAuth, SpotifyPoller};

const LOG_TARGET: &str = "versualizer::app";
const LOG_TARGET_SYNC: &str = "versualizer::sync::events";

fn main() {
    // Initialize logging
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load config or create template on first run
    let config = match Config::load_or_create() {
        Ok(config) => config,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };

    // Create tokio runtime for background tasks
    let runtime = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");

    // Initialize sync engine
    let sync_engine = SyncEngine::new();

    // Initialize lyrics cache and fetcher
    let cache = runtime.block_on(async {
        match LyricsCache::new().await {
            Ok(cache) => Arc::new(cache),
            Err(e) => {
                error!("Failed to initialize lyrics cache: {}", e);
                std::process::exit(1);
            }
        }
    });

    // Create lyrics providers based on config
    let providers: Vec<Box<dyn LyricsProvider>> = config
        .lyrics
        .providers
        .iter()
        .filter_map(|provider_type| -> Option<Box<dyn LyricsProvider>> {
            match provider_type {
                LyricsProviderType::Lrclib => {
                    info!(target: LOG_TARGET, "Initializing LRCLIB provider");
                    Some(Box::new(LrclibProvider::new()))
                }
                LyricsProviderType::SpotifyLyrics => {
                    if let Some(sp_dc) = &config.spotify.sp_dc {
                        if !sp_dc.is_empty() {
                            info!(target: LOG_TARGET, "Initializing Spotify lyrics provider (sp_dc configured)");
                            Some(Box::new(SpotifyLyricsProvider::new(sp_dc)))
                        } else {
                            info!(target: LOG_TARGET, "Skipping Spotify lyrics provider: sp_dc is empty");
                            None
                        }
                    } else {
                        info!(target: LOG_TARGET, "Skipping Spotify lyrics provider: sp_dc not configured");
                        None
                    }
                }
            }
        })
        .collect();

    let provider_names: Vec<_> = providers.iter().map(|p| p.name()).collect();
    info!(target: LOG_TARGET, "Initialized {} lyrics provider(s): {:?}", providers.len(), provider_names);

    // Create lyrics fetcher
    let lyrics_fetcher = Arc::new(LyricsFetcher::new(
        sync_engine.clone(),
        cache.clone(),
        providers,
    ));

    // Create RadioStation for global UI state
    let mut radio_station = RadioStation::<AppState, AppChannel>::create_global(AppState::default());

    // Create channel to bridge SyncEngine events to RadioStation
    let (tx, mut rx) = unbounded::<SyncEvent>();

    // Spawn task to forward SyncEngine events to channel
    runtime.spawn({
        let sync_engine = sync_engine.clone();
        async move {
            let mut event_rx = sync_engine.subscribe();
            while let Ok(event) = event_rx.recv().await {
                let _ = tx.unbounded_send(event);
            }
        }
    });

    // Spawn background tasks
    runtime.spawn(start_spotify_poller(config.clone(), sync_engine.clone()));
    runtime.spawn(start_lyrics_fetcher(lyrics_fetcher));
    runtime.spawn(log_sync_events(sync_engine.clone()));

    // Launch the Freya application
    launch(
        LaunchConfig::new()
            .with_future(async move {
                // Process SyncEngine events and update RadioStation
                while let Some(event) = rx.next().await {
                    match event {
                        SyncEvent::LyricsLoaded { lyrics } => {
                            let mut state = radio_station.write_channel(AppChannel::Lyrics);
                            state.lyrics = Some(lyrics);
                        }
                        SyncEvent::LyricsNotFound => {
                            let mut state = radio_station.write_channel(AppChannel::Lyrics);
                            state.lyrics = None;
                        }
                        SyncEvent::PositionSync { position } | SyncEvent::SeekOccurred { position } => {
                            // Check if line changed
                            let current_idx = {
                                let state = radio_station.read();
                                if let Some(ref lyrics) = state.lyrics {
                                    let new_idx = lyrics.current_line_index(position);
                                    if new_idx != state.current_line_index {
                                        Some((new_idx, lyrics.clone()))
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            };

                            if let Some((new_idx, lyrics)) = current_idx {
                                let mut state = radio_station.write_channel(AppChannel::LineChange);
                                state.current_line_index = new_idx;
                                if let Some(idx) = new_idx {
                                    state.line_start_time = lyrics.lines[idx].start_time;
                                    // Calculate duration to next line
                                    let next_start = lyrics
                                        .lines
                                        .get(idx + 1)
                                        .map(|l| l.start_time)
                                        .unwrap_or(state.line_start_time + Duration::from_secs(5));
                                    state.line_duration_ms = next_start
                                        .saturating_sub(state.line_start_time)
                                        .as_millis() as u64;
                                }
                            }
                            // Animation handles smooth interpolation between line changes
                        }
                        SyncEvent::TrackChanged { .. } | SyncEvent::PlaybackStarted { .. } => {
                            let mut state = radio_station.write_channel(AppChannel::PlaybackState);
                            state.has_track = true;
                            state.is_playing = true;
                            // Clear lyrics until new ones load
                            state.lyrics = None;
                            state.current_line_index = None;
                        }
                        SyncEvent::PlaybackPaused { .. } => {
                            radio_station
                                .write_channel(AppChannel::PlaybackState)
                                .is_playing = false;
                        }
                        SyncEvent::PlaybackResumed { .. } => {
                            radio_station
                                .write_channel(AppChannel::PlaybackState)
                                .is_playing = true;
                        }
                        SyncEvent::PlaybackStopped => {
                            let mut state = radio_station.write_channel(AppChannel::PlaybackState);
                            state.has_track = false;
                            state.is_playing = false;
                            state.lyrics = None;
                            state.current_line_index = None;
                        }
                        SyncEvent::Error { .. } => {
                            // Errors are logged elsewhere
                        }
                    }
                }
            })
            .with_fallback_font("Noto Sans CJK SC")
            .with_fallback_font("Noto Sans CJK JP")
            .with_fallback_font("Noto Sans CJK KR")
            .with_fallback_font("Microsoft YaHei")
            .with_fallback_font("PingFang SC")
            .with_fallback_font("Segoe UI Emoji")
            .with_window(
                WindowConfig::new(FpRender::from_render(app::App { radio_station }))
                    .with_title("Versualizer")
                    .with_size(
                        config.ui.window.width as f64,
                        config.ui.window.height as f64,
                    )
                    .with_background(Color::TRANSPARENT)
                    .with_decorations(false)
                    .with_transparency(true)
                    .with_window_handle(|window| {
                        window.set_window_level(WindowLevel::AlwaysOnTop);
                    }),
            ),
    );
}

/// Start the Spotify poller to fetch playback state
async fn start_spotify_poller(config: Config, sync_engine: Arc<SyncEngine>) {
    info!(target: LOG_TARGET, "Initializing Spotify OAuth...");

    let oauth = match SpotifyOAuth::new(
        &config.spotify.client_id,
        &config.spotify.client_secret,
        &config.spotify.oauth_redirect_uri,
    ) {
        Ok(oauth) => Arc::new(oauth),
        Err(e) => {
            error!(target: LOG_TARGET, "Failed to create Spotify OAuth: {}", e);
            return;
        }
    };

    // Ensure we're authenticated
    if let Err(e) = oauth.ensure_authenticated().await {
        error!(target: LOG_TARGET, "Spotify authentication failed: {}", e);
        return;
    }

    info!(target: LOG_TARGET, "Spotify authenticated successfully!");

    // Create and start the poller
    let poller = Arc::new(SpotifyPoller::new(
        oauth,
        sync_engine,
        config.spotify.poll_interval_ms,
    ));

    info!(target: LOG_TARGET, "Starting Spotify poller (interval: {}ms)", config.spotify.poll_interval_ms);
    let handle = poller.start();
    let _ = handle.await;
}

/// Start the lyrics fetcher to download and cache lyrics
async fn start_lyrics_fetcher(lyrics_fetcher: Arc<LyricsFetcher>) {
    info!(target: LOG_TARGET, "Starting lyrics fetcher...");
    let handle = lyrics_fetcher.start();
    let _ = handle.await;
}

/// Log all sync events to the console
async fn log_sync_events(sync_engine: Arc<SyncEngine>) {
    let mut rx = sync_engine.subscribe();

    loop {
        match rx.recv().await {
            Ok(event) => {
                match &event {
                    SyncEvent::PlaybackStarted { track, position } => {
                        info!(
                            target: LOG_TARGET_SYNC,
                            "Playback started: {} - {} (at {:?})",
                            track.artist, track.name, position
                        );
                    }
                    SyncEvent::PlaybackPaused { position } => {
                        info!(target: LOG_TARGET_SYNC, "Playback paused at {:?}", position);
                    }
                    SyncEvent::PlaybackResumed { position } => {
                        info!(target: LOG_TARGET_SYNC, "Playback resumed at {:?}", position);
                    }
                    SyncEvent::PlaybackStopped => {
                        info!(target: LOG_TARGET_SYNC, "Playback stopped");
                    }
                    SyncEvent::TrackChanged { track, position } => {
                        info!(
                            target: LOG_TARGET_SYNC,
                            "Track changed: {} - {} [{}] (at {:?})",
                            track.artist, track.name, track.album, position
                        );
                    }
                    SyncEvent::PositionSync { position } => {
                        // Log position updates less frequently (every 5 seconds worth)
                        if position.as_secs() % 5 == 0 && position.subsec_millis() < 1100 {
                            info!(target: LOG_TARGET_SYNC, "Position: {:?}", position);
                        }
                    }
                    SyncEvent::SeekOccurred { position } => {
                        info!(target: LOG_TARGET_SYNC, "Seek to {:?}", position);
                    }
                    SyncEvent::LyricsLoaded { lyrics } => {
                        info!(
                            target: LOG_TARGET_SYNC,
                            "Lyrics loaded: {} lines",
                            lyrics.lines.len()
                        );
                    }
                    SyncEvent::LyricsNotFound => {
                        info!(target: LOG_TARGET_SYNC, "No lyrics found for current track");
                    }
                    SyncEvent::Error { message } => {
                        error!(target: LOG_TARGET_SYNC, "Sync error: {}", message);
                    }
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                info!(target: LOG_TARGET_SYNC, "Sync event channel closed");
                break;
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                info!(target: LOG_TARGET_SYNC, "Missed {} sync events", n);
            }
        }
    }
}
