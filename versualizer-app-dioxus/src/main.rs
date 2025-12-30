#![cfg_attr(feature = "bundle", windows_subsystem = "windows")]
mod app;
mod bridge;
mod components;
mod state;
mod theme_watcher;
mod window_resize;
mod window_state;

use crate::app::App;
use crate::bridge::use_sync_engine_bridge;
use crate::state::KaraokeState;
use crate::window_state::WindowState;
use dioxus::desktop::tao::dpi::PhysicalPosition;
use dioxus::desktop::{LogicalSize, WindowBuilder};
use dioxus::prelude::*;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use versualizer_core::config::LyricsProviderType;
use versualizer_core::{
    CoreError, LyricsCache, LyricsFetcher, LyricsProvider, MusicSource, SyncEngine, SyncEvent,
    VersualizerConfig,
};
use versualizer_lyrics_lrclib::LrclibProvider;
use versualizer_lyrics_spotify::SpotifyLyricsProvider;
use versualizer_spotify_api::{
    SpotifyOAuth, SpotifyPoller, SpotifyProviderConfig, SPOTIFY_CONFIG_TEMPLATE,
};

#[allow(clippy::too_many_lines)]
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
    // Pass provider templates to include in the generated config file
    let provider_templates: &[&str] = &[SPOTIFY_CONFIG_TEMPLATE];
    let config = match VersualizerConfig::load_or_create(Some(provider_templates)) {
        Ok(config) => config,
        Err(e) => {
            error!("{e}");
            std::process::exit(1);
        }
    };

    // Validate provider-specific config based on music source
    if let Err(e) = validate_provider_config(&config) {
        error!("{e}");
        std::process::exit(1);
    }

    // Create tokio runtime for background tasks
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            error!("Failed to create tokio runtime: {e}");
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
                error!("Failed to initialize lyrics cache: {}", e);
                std::process::exit(1);
            }
        }
    });

    // Create lyrics providers based on config
    let providers = create_providers(&config);

    let provider_names: Vec<_> = providers.iter().map(|p| p.name()).collect();
    info!(
        "Initialized {} lyrics provider(s): {:?}",
        providers.len(),
        provider_names
    );

    // Create shared cancellation token for graceful shutdown
    let cancel_token = CancellationToken::new();

    // Set up Ctrl+C handler to trigger graceful shutdown
    let ctrlc_token = cancel_token.clone();
    if let Err(e) = ctrlc::set_handler(move || {
        info!("Received Ctrl+C, shutting down gracefully...");
        ctrlc_token.cancel();
    }) {
        error!("Failed to set Ctrl+C handler: {}", e);
    }

    // Create lyrics fetcher with cancellation token
    let lyrics_fetcher = Arc::new(LyricsFetcher::new(
        sync_engine.clone(),
        cache,
        providers,
        Some(cancel_token.clone()),
    ));

    // Spawn background tasks
    runtime.spawn(start_spotify_poller(
        config.clone(),
        sync_engine.clone(),
        cancel_token.clone(),
    ));
    runtime.spawn(start_lyrics_fetcher(lyrics_fetcher));
    runtime.spawn(log_sync_events(sync_engine.clone()));

    // Load saved window position if available
    let saved_position = WindowState::load();

    // Configure window with default initial size
    // Window will be auto-resized by CSS-driven hook after first render
    let window = WindowBuilder::new()
        .with_title("Versualizer")
        .with_transparent(true)
        .with_decorations(false)
        .with_resizable(true)
        .with_maximizable(false)
        .with_always_on_top(true)
        .with_closable(true)
        .with_visible_on_all_workspaces(true)
        .with_inner_size(LogicalSize::new(900.0, 200.0));

    // Disable window shadow on Windows for true overlay effect
    #[cfg(target_os = "windows")]
    let window = {
        use dioxus::desktop::tao::platform::windows::WindowBuilderExtWindows;
        window.with_undecorated_shadow(false)
    };

    #[cfg(target_os = "macos")]
    let window = {
        use dioxus::desktop::tao::platform::macos::WindowBuilderExtMacOS;
        window
            .with_movable_by_window_background(true)
            .with_title_hidden(true)
            .with_titlebar_hidden(true)
            .with_titlebar_buttons_hidden(true)
            .with_titlebar_transparent(true)
            .with_has_shadow(false)
    };

    // Apply saved position if available
    let window = if let Some(state) = saved_position {
        info!("Restoring window position: ({}, {})", state.x, state.y);
        window.with_position(PhysicalPosition::new(state.x, state.y))
    } else {
        window
    };

    // CSS is now handled by the theme_watcher module in the App component
    // This allows for hot-reload of CSS from ~/.config/versualizer/theme.css
    let dioxus_config = dioxus::desktop::Config::default()
        .with_window(window)
        .with_disable_context_menu(true);

    // Launch Dioxus application
    // Use with_context to inject SyncEngine, UI config, and cancellation token before launch
    dioxus::LaunchBuilder::desktop()
        .with_cfg(dioxus_config)
        .with_context(sync_engine)
        .with_context(config.ui)
        .with_context(cancel_token)
        .launch(app);
}

/// Root component that sets up context and renders the app
fn app() -> Element {
    // Create karaoke state with granular signals
    let karaoke = use_context_provider(KaraokeState::new);

    // Get the sync engine from context (injected via with_context)
    let sync_engine: Arc<SyncEngine> = use_context();

    // Bridge SyncEngine events to Dioxus signals
    use_sync_engine_bridge(&sync_engine, karaoke);

    rsx! {
       document::Link { rel: "icon", href: asset!("/icons/icon.ico") },
       App {}
    }
}

/// Validate provider-specific configuration based on selected music source
fn validate_provider_config(config: &VersualizerConfig) -> Result<(), CoreError> {
    if config.music.source == MusicSource::Spotify {
        let spotify_config =
            SpotifyProviderConfig::from_providers(&config.providers)?.ok_or_else(|| {
                CoreError::ConfigMissingField {
                    field: "providers.spotify".into(),
                }
            })?;
        spotify_config.validate()?;
    }
    // Future sources would have their own validation
    Ok(())
}

fn create_providers(config: &VersualizerConfig) -> Vec<Box<dyn LyricsProvider>> {
    config
        .lyrics
        .providers
        .iter()
        .filter_map(|provider_type| -> Option<Box<dyn LyricsProvider>> {
            match provider_type {
                LyricsProviderType::Lrclib => {
                    info!("Initializing LRCLIB provider");
                    match LrclibProvider::new() {
                        Ok(provider) => Some(Box::new(provider)),
                        Err(e) => {
                            error!("Failed to create LRCLIB provider: {}", e);
                            None
                        }
                    }
                }
                LyricsProviderType::SpotifyLyrics => {
                    // Access Spotify config from providers section
                    let spotify_config =
                        match SpotifyProviderConfig::from_providers(&config.providers) {
                            Ok(Some(cfg)) => cfg,
                            Ok(None) => {
                                info!("Skipping Spotify lyrics provider: not configured");
                                return None;
                            }
                            Err(e) => {
                                error!("Failed to parse Spotify config: {}", e);
                                return None;
                            }
                        };

                    spotify_config.sp_dc.as_ref().map_or_else(
                        || {
                            info!("Skipping Spotify lyrics provider: sp_dc not configured");
                            None
                        },
                        |sp_dc| {
                            if sp_dc.is_empty() {
                                info!("Skipping Spotify lyrics provider: sp_dc is empty");
                                None
                            } else {
                                info!("Initializing Spotify lyrics provider (sp_dc configured)");
                                let secret_key_url = spotify_config.secret_key_url.clone();
                                match SpotifyLyricsProvider::new(sp_dc, secret_key_url) {
                                    Ok(provider) => {
                                        Some(Box::new(provider) as Box<dyn LyricsProvider>)
                                    }
                                    Err(e) => {
                                        error!("Failed to create Spotify lyrics provider: {}", e);
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
async fn start_spotify_poller(
    config: VersualizerConfig,
    sync_engine: Arc<SyncEngine>,
    cancel_token: CancellationToken,
) {
    info!("Initializing Spotify Web API poller...");

    // Get Spotify config from providers section
    let spotify_config = match SpotifyProviderConfig::from_providers(&config.providers) {
        Ok(Some(cfg)) => cfg,
        Ok(None) => {
            error!("Spotify provider not configured");
            return;
        }
        Err(e) => {
            error!("Failed to parse Spotify config: {}", e);
            return;
        }
    };

    let oauth = match SpotifyOAuth::new(
        &spotify_config.client_id,
        &spotify_config.client_secret,
        &spotify_config.oauth_redirect_uri,
    ) {
        Ok(oauth) => Arc::new(oauth),
        Err(e) => {
            error!("Failed to create Spotify OAuth: {}", e);
            return;
        }
    };

    // Ensure we're authenticated
    if let Err(e) = oauth.ensure_authenticated().await {
        error!("Spotify authentication failed: {}", e);
        return;
    }

    info!("Spotify authenticated successfully!");

    // Create and start the poller with cancellation token
    let poller = Arc::new(SpotifyPoller::new(
        oauth,
        sync_engine,
        spotify_config.poll_interval_ms,
        Some(cancel_token),
    ));

    info!(
        "Starting Spotify poller (interval: {}ms)",
        spotify_config.poll_interval_ms
    );
    let handle = poller.start();
    let _ = handle.await;
}

/// Start the lyrics fetcher to download and cache lyrics
async fn start_lyrics_fetcher(lyrics_fetcher: Arc<LyricsFetcher>) {
    info!("Starting lyrics fetcher...");
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
                            "Playback started: {} - {} (at {:?})",
                            track.artist, track.name, position
                        );
                    }
                    SyncEvent::PlaybackPaused { position } => {
                        info!("Playback paused at {:?}", position);
                    }
                    SyncEvent::PlaybackResumed { position } => {
                        info!("Playback resumed at {:?}", position);
                    }
                    SyncEvent::PlaybackStopped => {
                        info!("Playback stopped");
                    }
                    SyncEvent::TrackChanged { track, position } => {
                        info!(
                            "Track changed: {} - {} [{}] (at {:?})",
                            track.artist, track.name, track.album, position
                        );
                    }
                    SyncEvent::PositionSync { .. } => {
                        // Timer position already logged by spotify::poller
                    }
                    SyncEvent::SeekOccurred { position } => {
                        info!("Seek to {:?}", position);
                    }
                    SyncEvent::LyricsLoaded { lyrics } => {
                        info!("Lyrics loaded: {} lines", lyrics.lines.len());
                    }
                    SyncEvent::LyricsNotFound => {
                        info!("No lyrics found for current track");
                    }
                    SyncEvent::Error { message } => {
                        error!("Sync error: {}", message);
                    }
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                info!("Sync event channel closed");
                break;
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                info!("Missed {} sync events", n);
            }
        }
    }
}
