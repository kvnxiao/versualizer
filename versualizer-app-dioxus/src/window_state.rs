//! Window state persistence for saving and restoring window position.

use serde::{Deserialize, Serialize};
use std::fs;
use tracing::{info, warn};

/// Persisted window state (position only, size is config-driven).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowState {
    /// X position of the window's outer bounds
    pub x: i32,
    /// Y position of the window's outer bounds
    pub y: i32,
}

impl WindowState {
    /// Load window state from the cache file.
    /// Returns `None` if the file doesn't exist or can't be parsed.
    #[must_use]
    pub fn load() -> Option<Self> {
        let path = versualizer_core::window_state_path();

        if !path.exists() {
            return None;
        }

        match fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str(&content) {
                Ok(state) => {
                    info!("Loaded window state from {:?}", path);
                    Some(state)
                }
                Err(e) => {
                    warn!("Failed to parse window state: {}", e);
                    None
                }
            },
            Err(e) => {
                warn!("Failed to read window state file: {}", e);
                None
            }
        }
    }

    /// Save window state to the cache file.
    pub fn save(&self) {
        let path = versualizer_core::window_state_path();

        // Ensure parent directory exists
        if let Some(parent) = path.parent()
            && let Err(e) = fs::create_dir_all(parent)
        {
            warn!("Failed to create window state directory: {}", e);
            return;
        }

        match serde_json::to_string_pretty(self) {
            Ok(content) => {
                if let Err(e) = fs::write(&path, content) {
                    warn!("Failed to write window state: {}", e);
                } else {
                    info!("Saved window state to {:?}", path);
                }
            }
            Err(e) => {
                warn!("Failed to serialize window state: {}", e);
            }
        }
    }
}
