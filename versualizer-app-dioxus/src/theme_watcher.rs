//! Theme file watching and hot-reload CSS injection.
//!
//! This module handles:
//! 1. Copying the embedded CSS template to the user's config directory on first run
//! 2. Loading CSS from the user's theme file at runtime
//! 3. Watching the theme file for changes and updating a Signal to trigger re-render

use dioxus::prelude::*;
use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode, DebounceEventResult};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::mpsc as tokio_mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

/// Errors that can occur during theme operations
#[derive(Debug, Error)]
pub enum ThemeError {
    #[error("Failed to read theme file: {0}")]
    ReadError(#[from] std::io::Error),

    #[error("Failed to initialize file watcher: {0}")]
    WatcherError(#[from] notify::Error),
}

/// Embedded default CSS template (compiled into the binary)
const DEFAULT_CSS: &str = include_str!("../assets/default_theme.css");

/// Initialize theme file, copying the embedded template if it doesn't exist.
/// Returns the CSS content to use.
///
/// # Errors
///
/// Returns an error if the config directory cannot be created or the file cannot be written.
pub fn initialize_theme() -> Result<String, ThemeError> {
    let theme_path = versualizer_core::theme_path();

    if theme_path.exists() {
        // Load existing theme
        info!("Loading theme from {:?}", theme_path);
        Ok(fs::read_to_string(&theme_path)?)
    } else {
        // First run: copy embedded CSS to config directory
        info!(
            "Theme file not found, creating from template at {:?}",
            theme_path
        );

        // Ensure config directory exists
        let config_dir = versualizer_core::config_dir();
        fs::create_dir_all(&config_dir)?;

        // Write the embedded CSS template
        fs::write(&theme_path, DEFAULT_CSS)?;

        Ok(DEFAULT_CSS.to_string())
    }
}

/// Load CSS content from the theme file.
/// Falls back to embedded CSS if the file cannot be read.
#[must_use]
pub fn load_theme_css() -> String {
    let theme_path = versualizer_core::theme_path();

    match fs::read_to_string(&theme_path) {
        Ok(css) => css,
        Err(e) => {
            warn!("Failed to read theme file, using embedded CSS: {}", e);
            DEFAULT_CSS.to_string()
        }
    }
}

/// Dioxus hook that provides reactive CSS content with file watching.
///
/// This hook:
/// 1. Initializes the theme file on first run
/// 2. Provides a `Signal<String>` with the current CSS content
/// 3. Watches the theme file for changes and updates the signal
///
/// When the signal updates, the component re-renders and the `<style>` element
/// in the RSX is updated with the new CSS content.
#[must_use]
pub fn use_theme_watcher(cancel_token: CancellationToken) -> Signal<String> {
    // Initialize CSS signal with current theme content
    let mut css_content = use_signal(|| {
        initialize_theme().unwrap_or_else(|e| {
            error!("Failed to initialize theme: {}", e);
            DEFAULT_CSS.to_string()
        })
    });

    // Spawn the file watcher task
    use_effect(move || {
        let cancel_token = cancel_token.clone();

        spawn(async move {
            let theme_path = versualizer_core::theme_path();

            // Create a tokio channel for file watcher events
            // Using Arc to share the sender across threads
            let (tx, mut rx) = tokio_mpsc::channel::<()>(16);
            let tx = Arc::new(tx);

            // Create debounced watcher (300ms debounce to handle rapid saves)
            let tx_clone = Arc::clone(&tx);
            let mut debouncer = match new_debouncer(
                Duration::from_millis(300),
                move |res: DebounceEventResult| {
                    if let Ok(events) = res {
                        for _ in events {
                            // Send notification that file changed
                            // Use blocking_send since we're in a sync callback
                            let _ = tx_clone.blocking_send(());
                        }
                    }
                },
            ) {
                Ok(d) => d,
                Err(e) => {
                    error!("Failed to create file watcher: {}", e);
                    return;
                }
            };

            // Watch the theme file's parent directory (more reliable than watching the file directly)
            let watch_path = theme_path
                .parent()
                .map_or_else(|| theme_path.clone(), PathBuf::from);

            if let Err(e) = debouncer
                .watcher()
                .watch(&watch_path, RecursiveMode::NonRecursive)
            {
                error!("Failed to watch theme directory: {}", e);
                return;
            }

            info!("Watching theme file for changes: {:?}", theme_path);

            // Poll for file changes or cancellation
            loop {
                tokio::select! {
                    () = cancel_token.cancelled() => {
                        info!("Theme watcher shutting down");
                        break;
                    }
                    Some(()) = rx.recv() => {
                        info!("Theme file changed, reloading CSS");
                        let new_css = load_theme_css();
                        css_content.set(new_css);
                    }
                }
            }

            // Keep debouncer alive until we exit the loop
            drop(debouncer);
        });
    });

    css_content
}
