use std::time::{Duration, Instant};

/// Current playback state from the music player
#[derive(Debug, Clone)]
pub struct PlaybackState {
    /// Whether music is currently playing
    pub is_playing: bool,
    /// Current track information (None if nothing is playing)
    pub track: Option<TrackInfo>,
    /// Current playback position
    pub position: Duration,
    /// Total track duration
    pub duration: Duration,
    /// When this state was last updated (for interpolation)
    pub updated_at: Instant,
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self {
            is_playing: false,
            track: None,
            position: Duration::ZERO,
            duration: Duration::ZERO,
            updated_at: Instant::now(),
        }
    }
}

impl PlaybackState {
    /// Create a new playback state
    pub fn new(
        is_playing: bool,
        track: Option<TrackInfo>,
        position: Duration,
        duration: Duration,
    ) -> Self {
        Self {
            is_playing,
            track,
            position,
            duration,
            updated_at: Instant::now(),
        }
    }

    /// Get interpolated position based on time elapsed since last update
    pub fn interpolated_position(&self) -> Duration {
        if !self.is_playing {
            return self.position;
        }

        let elapsed = self.updated_at.elapsed();
        let interpolated = self.position + elapsed;

        // Clamp to track duration
        interpolated.min(self.duration)
    }

    /// Check if the track has changed
    pub fn track_changed(&self, other: &PlaybackState) -> bool {
        match (&self.track, &other.track) {
            (Some(a), Some(b)) => a.id != b.id,
            (None, None) => false,
            _ => true,
        }
    }

    /// Check if playback state changed (playing <-> paused)
    pub fn playback_state_changed(&self, other: &PlaybackState) -> bool {
        self.is_playing != other.is_playing
    }

    /// Check if a seek occurred (position jumped unexpectedly)
    pub fn seek_occurred(&self, other: &PlaybackState, threshold: Duration) -> bool {
        if self.track_changed(other) {
            return false;
        }

        // Calculate expected position based on elapsed time
        let expected = self.interpolated_position();
        let actual = other.position;

        // If the difference is larger than threshold, a seek occurred
        if actual > expected {
            actual - expected > threshold
        } else {
            expected - actual > threshold
        }
    }
}

/// Information about the currently playing track
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackInfo {
    /// Unique track identifier (e.g., Spotify track URI)
    pub id: String,
    /// Track name
    pub name: String,
    /// Artist name(s)
    pub artist: String,
    /// Album name
    pub album: String,
    /// Track duration
    pub duration: Duration,
}

impl TrackInfo {
    /// Create a new track info
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        artist: impl Into<String>,
        album: impl Into<String>,
        duration: Duration,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            artist: artist.into(),
            album: album.into(),
            duration,
        }
    }

    /// Get duration in seconds (for lyrics query)
    pub fn duration_secs(&self) -> u32 {
        self.duration.as_secs() as u32
    }
}
