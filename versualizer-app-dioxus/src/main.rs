mod app;
mod bridge;
mod components;
mod state;

use crate::app::App;
use crate::bridge::use_sync_engine_bridge;
use crate::state::{KaraokeDisplayConfig, KaraokeState};
use dioxus::desktop::{LogicalSize, WindowBuilder};
use dioxus::prelude::*;
use std::sync::Arc;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use versualizer_core::config::LyricsProviderType;
use versualizer_core::providers::LrclibProvider;
use versualizer_core::{Config, LyricsCache, LyricsProvider, SyncEngine, SyncEvent};
use versualizer_spotify::{LyricsFetcher, SpotifyLyricsProvider, SpotifyOAuth, SpotifyPoller};

const LOG_TARGET: &str = "versualizer::app";
const LOG_TARGET_SYNC: &str = "versualizer::sync::events";

fn main() {
    // Initialize logging
    // Filter out noisy rspotify HTTP request logs
    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,rspotify_http=warn")),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load config or create template on first run
    let config = match Config::load_or_create() {
        Ok(config) => config,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    // Create tokio runtime for background tasks
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Failed to create tokio runtime: {e}");
            std::process::exit(1);
        }
    };

    // Initialize sync engine
    let sync_engine = SyncEngine::new();

    // Initialize lyrics cache
    let cache = runtime.block_on(async {
        match LyricsCache::new().await {
            Ok(cache) => Arc::new(cache),
            Err(e) => {
                error!(target: LOG_TARGET, "Failed to initialize lyrics cache: {}", e);
                std::process::exit(1);
            }
        }
    });

    // Create lyrics providers based on config
    let providers = create_providers(&config);

    let provider_names: Vec<_> = providers.iter().map(|p| p.name()).collect();
    info!(target: LOG_TARGET, "Initialized {} lyrics provider(s): {:?}", providers.len(), provider_names);

    // Create lyrics fetcher
    let lyrics_fetcher = Arc::new(LyricsFetcher::new(
        sync_engine.clone(),
        cache,
        providers,
    ));

    // Spawn background tasks
    runtime.spawn(start_spotify_poller(config.clone(), sync_engine.clone()));
    runtime.spawn(start_lyrics_fetcher(lyrics_fetcher));
    runtime.spawn(log_sync_events(sync_engine.clone()));

    // Configure window
    let window = WindowBuilder::new()
        .with_title("Versualizer")
        .with_transparent(true)
        .with_decorations(false)
        .with_always_on_top(true)
        .with_inner_size(LogicalSize::new(
            f64::from(config.ui.window.width),
            f64::from(config.ui.window.height),
        ));

    // Disable window shadow on Windows for true overlay effect
    #[cfg(target_os = "windows")]
    let window = {
        use dioxus::desktop::tao::platform::windows::WindowBuilderExtWindows;
        window.with_undecorated_shadow(false)
    };

    // Embed CSS directly to avoid path resolution issues in desktop mode
    let css = include_str!("../assets/style.css");
    let custom_head = format!(r"<style>{css}</style>");

    let dioxus_config = dioxus::desktop::Config::default()
        .with_window(window)
        .with_custom_head(custom_head);

    // Create display config from loaded config
    let display_config = KaraokeDisplayConfig {
        max_lines: config.ui.layout.max_lines.clamp(1, 3),
        current_line_scale: config.ui.layout.current_line_scale,
        upcoming_line_scale: 0.8,
        transition_ms: config.ui.animation.transition_ms,
        easing: convert_easing(&config.ui.animation.easing),
    };

    // Launch Dioxus application
    // Use with_context to inject SyncEngine and display config before launch
    dioxus::LaunchBuilder::desktop()
        .with_cfg(dioxus_config)
        .with_context(sync_engine)
        .with_context(display_config)
        .launch(app);
}

/// Convert config easing format to CSS easing function
fn convert_easing(easing: &str) -> String {
    match easing {
        "linear" => "linear",
        "ease_in" => "ease-in",
        "ease_out" => "ease-out",
        _ => "ease-in-out",
    }
    .into()
}

/// Root component that sets up context and renders the app
fn app() -> Element {
    // Create karaoke state with granular signals
    let karaoke = use_context_provider(KaraokeState::new);

    // Get the sync engine from context (injected via with_context)
    let sync_engine: Arc<SyncEngine> = use_context();

    // Bridge SyncEngine events to Dioxus signals
    use_sync_engine_bridge(&sync_engine, karaoke);

    rsx! { App {} }
}

fn create_providers(config: &Config) -> Vec<Box<dyn LyricsProvider>> {
    config
        .lyrics
        .providers
        .iter()
        .filter_map(|provider_type| -> Option<Box<dyn LyricsProvider>> {
            match provider_type {
                LyricsProviderType::Lrclib => {
                    info!(target: LOG_TARGET, "Initializing LRCLIB provider");
                    match LrclibProvider::new() {
                        Ok(provider) => Some(Box::new(provider)),
                        Err(e) => {
                            error!(target: LOG_TARGET, "Failed to create LRCLIB provider: {}", e);
                            None
                        }
                    }
                }
                LyricsProviderType::SpotifyLyrics => {
                    config.spotify.sp_dc.as_ref().map_or_else(
                        || {
                            info!(target: LOG_TARGET, "Skipping Spotify lyrics provider: sp_dc not configured");
                            None
                        },
                        |sp_dc| {
                            if sp_dc.is_empty() {
                                info!(target: LOG_TARGET, "Skipping Spotify lyrics provider: sp_dc is empty");
                                None
                            } else {
                                info!(target: LOG_TARGET, "Initializing Spotify lyrics provider (sp_dc configured)");
                                match SpotifyLyricsProvider::new(sp_dc) {
                                    Ok(provider) => Some(Box::new(provider) as Box<dyn LyricsProvider>),
                                    Err(e) => {
                                        error!(target: LOG_TARGET, "Failed to create Spotify lyrics provider: {}", e);
                                        None
                                    }
                                }
                            }
                        },
                    )
                }
            }
        })
        .collect()
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
