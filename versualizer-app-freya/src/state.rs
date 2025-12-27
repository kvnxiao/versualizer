use freya_radio::prelude::*;
use std::time::Duration;
use versualizer_core::LrcFile;

/// Global app state managed by RadioStation
#[derive(Default)]
pub struct AppState {
    /// Current lyrics (if loaded)
    pub lyrics: Option<LrcFile>,
    /// Index of the currently active line
    pub current_line_index: Option<usize>,
    /// Start time of the current line
    pub line_start_time: Duration,
    /// Duration of the current line in milliseconds
    pub line_duration_ms: u64,
    /// Whether a track is currently loaded
    pub has_track: bool,
    /// Whether playback is active
    pub is_playing: bool,
}

/// Channels for selective UI updates.
/// Components subscribe to specific channels to only re-render when relevant state changes.
#[derive(PartialEq, Eq, Clone, Debug, Copy, Hash)]
pub enum AppChannel {
    /// Lyrics loaded or cleared
    Lyrics,
    /// Current line changed (triggers animation restart)
    LineChange,
    /// Play/pause/stop state changed
    PlaybackState,
}

impl RadioChannel<AppState> for AppChannel {}
