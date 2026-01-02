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
use dioxus::desktop::tao::window::Icon;
use dioxus::desktop::{LogicalSize, WindowBuilder};
use dioxus::prelude::*;
use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};
use versualizer_core::config::LyricsProviderType;
use versualizer_core::{
    CoreError, LyricsCache, LyricsFetcher, LyricsProvider, MusicSource, SyncEngine, SyncEvent,
    TomlParseError, VersualizerConfig,
};
use versualizer_lyrics_lrclib::LrclibProvider;
use versualizer_lyrics_spotify::SpotifyLyricsProvider;
use versualizer_spotify_api::{
    SPOTIFY_CONFIG_TEMPLATE, SpotifyOAuth, SpotifyPoller, SpotifyProviderConfig,
};

const APP_NAME: &str = "Versualizer";

#[allow(clippy::too_many_lines)]
fn main() {
    // Initialize logging with optional file output
    // Check config for logging.enabled before full config load
    let file_logging_enabled = check_file_logging_enabled();
    init_tracing(file_logging_enabled);

    // Load config or create template on first run
    // Pass provider templates to include in the generated config file
    let provider_templates: &[&str] = &[SPOTIFY_CONFIG_TEMPLATE];
    let config = match VersualizerConfig::load_or_create(Some(provider_templates)) {
        Ok(config) => config,
        Err(CoreError::ConfigNotFound { path }) => {
            // Config was just created - show dialog informing user
            show_new_config_dialog(&path);
            std::process::exit(0);
        }
        Err(CoreError::ConfigParseError(parse_error)) => {
            // Config has TOML syntax errors - show dialog with reset option
            show_config_parse_error_dialog(&parse_error, &VersualizerConfig::config_path());
            std::process::exit(1);
        }
        Err(e) => {
            error!("{e}");
            show_generic_error_dialog(&e.to_string());
            std::process::exit(1);
        }
    };

    // Validate config fields and show dialog if any are missing
    let validation = validate_config_fields(&config);
    if !validation.is_valid() {
        show_config_error_dialog(&validation, &VersualizerConfig::config_path());
    }

    // Validate provider-specific config based on music source (for any remaining validation)
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

    // Load window icon for taskbar
    let window_icon = load_window_icon();

    // Configure window with default initial size
    // Window will be auto-resized by CSS-driven hook after first render
    let window = WindowBuilder::new()
        .with_title(APP_NAME)
        .with_transparent(true)
        .with_decorations(false)
        .with_resizable(true)
        .with_maximizable(false)
        .with_always_on_top(true)
        .with_closable(true)
        .with_visible_on_all_workspaces(true)
        .with_inner_size(LogicalSize::new(900.0, 200.0))
        .with_window_icon(window_icon);

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
        document::Title { "{APP_NAME}" },
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

/// Result of config validation with all missing fields collected
struct ConfigValidationResult {
    missing_fields: Vec<String>,
}

impl ConfigValidationResult {
    fn is_valid(&self) -> bool {
        self.missing_fields.is_empty()
    }

    fn error_message(&self) -> String {
        format!(
            "The following required configuration fields are missing or empty:\n\n{}",
            self.missing_fields
                .iter()
                .map(|f| format!("  \u{2022} {f}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    }
}

/// Validate config and collect all missing fields for user-friendly error display
fn validate_config_fields(config: &VersualizerConfig) -> ConfigValidationResult {
    let mut missing_fields = Vec::new();

    if config.music.source == MusicSource::Spotify {
        match SpotifyProviderConfig::from_providers(&config.providers) {
            Ok(Some(spotify_config)) => {
                if spotify_config.client_id.is_empty() {
                    missing_fields.push("providers.spotify.client_id".into());
                }
                if spotify_config.client_secret.is_empty() {
                    missing_fields.push("providers.spotify.client_secret".into());
                }
            }
            Ok(None) => {
                missing_fields.push("providers.spotify".into());
            }
            Err(_) => {
                missing_fields.push("providers.spotify (invalid format)".into());
            }
        }
    }

    ConfigValidationResult { missing_fields }
}

/// Show a native OS dialog for missing configuration and handle user response
fn show_config_error_dialog(validation: &ConfigValidationResult, config_path: &Path) {
    let message = format!(
        "{}\n\nPlease edit the configuration file to add these values.\n\n\
        Get Spotify credentials from:\nhttps://developer.spotify.com/dashboard",
        validation.error_message()
    );

    let result = MessageDialog::new()
        .set_level(MessageLevel::Error)
        .set_title("Versualizer - Configuration Required")
        .set_description(&message)
        .set_buttons(MessageButtons::OkCancelCustom(
            "Open Config".into(),
            "Exit".into(),
        ))
        .show();

    if matches!(result, MessageDialogResult::Custom(ref s) if s == "Open Config") {
        // Open config file in default editor
        if let Err(e) = open::that(config_path) {
            error!("Failed to open config file: {e}");
        }
    }

    // Always exit after showing the dialog
    std::process::exit(1);
}

/// Show dialog when config is newly created
fn show_new_config_dialog(config_path: &Path) {
    let message = "A configuration file has been created.\n\n\
        Please edit it with your Spotify credentials:\n\
        \u{2022} providers.spotify.client_id\n\
        \u{2022} providers.spotify.client_secret\n\n\
        Get these from:\nhttps://developer.spotify.com/dashboard";

    let result = MessageDialog::new()
        .set_level(MessageLevel::Info)
        .set_title("Versualizer - Configuration Created")
        .set_description(message)
        .set_buttons(MessageButtons::OkCancelCustom(
            "Open Config".into(),
            "Exit".into(),
        ))
        .show();

    if matches!(result, MessageDialogResult::Custom(ref s) if s == "Open Config")
        && let Err(e) = open::that(config_path)
    {
        error!("Failed to open config file: {e}");
    }
}

/// Show dialog when config file has TOML parsing errors
fn show_config_parse_error_dialog(parse_error: &TomlParseError, config_path: &Path) {
    let message = format!(
        "Your configuration file has a syntax error and cannot be loaded.\n\n\
        Error: {parse_error}\n\n\
        You can either:\n\
        \u{2022} Open the config file and fix the syntax error\n\
        \u{2022} Reset to a fresh configuration template"
    );

    let result = MessageDialog::new()
        .set_level(MessageLevel::Error)
        .set_title("Versualizer - Configuration Error")
        .set_description(&message)
        .set_buttons(MessageButtons::OkCancelCustom(
            "Open Config".into(),
            "Reset Config".into(),
        ))
        .show();

    match result {
        MessageDialogResult::Custom(button) if button == "Open Config" => {
            // Open config file in default editor
            if let Err(e) = open::that(config_path) {
                error!("Failed to open config file: {e}");
            }
        }
        MessageDialogResult::Custom(button) if button == "Reset Config" => {
            // Reset config to template
            if let Err(e) = reset_config_to_template(config_path) {
                error!("Failed to reset config file: {e}");
                // Show error dialog
                MessageDialog::new()
                    .set_level(MessageLevel::Error)
                    .set_title("Versualizer - Reset Failed")
                    .set_description(format!("Failed to reset configuration:\n{e}"))
                    .set_buttons(MessageButtons::Ok)
                    .show();
            } else {
                // Show success and open the file
                MessageDialog::new()
                    .set_level(MessageLevel::Info)
                    .set_title("Versualizer - Configuration Reset")
                    .set_description(
                        "Configuration has been reset to the default template.\n\n\
                        Please edit it with your Spotify credentials and restart the app.",
                    )
                    .set_buttons(MessageButtons::Ok)
                    .show();
                if let Err(e) = open::that(config_path) {
                    error!("Failed to open config file: {e}");
                }
            }
        }
        _ => {
            // User closed dialog or clicked an unexpected button - just exit
        }
    }

    std::process::exit(1);
}

/// Reset the config file to the default template
fn reset_config_to_template(config_path: &Path) -> std::io::Result<()> {
    use std::fs;
    use versualizer_core::config::build_config_template;

    let provider_templates: &[&str] = &[SPOTIFY_CONFIG_TEMPLATE];
    let template = build_config_template(Some(provider_templates));

    fs::write(config_path, template)
}

/// Show a generic error dialog for unexpected errors
fn show_generic_error_dialog(error_message: &str) {
    let message = format!(
        "An unexpected error occurred:\n\n{error_message}\n\n\
        Please check your configuration file or report this issue."
    );

    MessageDialog::new()
        .set_level(MessageLevel::Error)
        .set_title("Versualizer - Error")
        .set_description(&message)
        .set_buttons(MessageButtons::Ok)
        .show();
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

/// Load window icon from embedded PNG for taskbar display
fn load_window_icon() -> Option<Icon> {
    // Use the 64x64 PNG for good taskbar resolution
    let icon_bytes = include_bytes!("../icons/64x64.png");

    let img = match image::load_from_memory(icon_bytes) {
        Ok(img) => img.into_rgba8(),
        Err(e) => {
            error!("Failed to load window icon: {}", e);
            return None;
        }
    };

    let (width, height) = img.dimensions();
    let rgba = img.into_raw();

    match Icon::from_rgba(rgba, width, height) {
        Ok(icon) => Some(icon),
        Err(e) => {
            error!("Failed to create window icon: {}", e);
            None
        }
    }
}

/// Check if file logging is enabled by reading the config file.
/// This is done before full config loading to set up tracing first.
/// Returns `false` if config doesn't exist or can't be parsed.
fn check_file_logging_enabled() -> bool {
    // Minimal structs to parse just the logging.enabled field
    #[derive(serde::Deserialize)]
    struct PartialConfig {
        #[serde(default)]
        logging: PartialLoggingConfig,
    }
    #[derive(serde::Deserialize, Default)]
    struct PartialLoggingConfig {
        #[serde(default)]
        enabled: bool,
    }

    let config_path = VersualizerConfig::config_path();
    let Ok(content) = std::fs::read_to_string(&config_path) else {
        return false;
    };

    toml::from_str::<PartialConfig>(&content)
        .map(|c| c.logging.enabled)
        .unwrap_or(false)
}

/// Initialize tracing with console output and optional file logging
fn init_tracing(file_logging_enabled: bool) {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,rspotify_http=warn"));

    let fmt_layer = tracing_subscriber::fmt::layer();

    if file_logging_enabled {
        let log_path = versualizer_core::paths::log_file_path();

        // Create cache directory if needed
        if let Some(parent) = log_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        match File::create(&log_path) {
            Ok(file) => {
                let file_layer = tracing_subscriber::fmt::layer()
                    .with_writer(Arc::new(file))
                    .with_ansi(false);

                tracing_subscriber::registry()
                    .with(env_filter)
                    .with(fmt_layer)
                    .with(file_layer)
                    .init();

                return;
            }
            Err(e) => {
                eprintln!("Failed to create log file at {}: {e}", log_path.display());
            }
        }
    }

    // Fallback: console only
    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .init();
}
