use crate::source::MusicSource;
use crate::time::DurationExt;
use std::collections::HashMap;
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
    pub fn track_changed(&self, other: &Self) -> bool {
        match (&self.track, &other.track) {
            (Some(a), Some(b)) => {
                a.source != b.source || a.source_track_id != b.source_track_id
            }
            (None, None) => false,
            _ => true,
        }
    }

    /// Check if playback state changed (playing <-> paused)
    #[must_use]
    pub const fn playback_state_changed(&self, other: &Self) -> bool {
        self.is_playing != other.is_playing
    }

    /// Check if a seek occurred (position jumped unexpectedly)
    #[must_use]
    pub fn seek_occurred(&self, other: &Self, threshold: Duration) -> bool {
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

/// Provider-specific track identifiers.
///
/// Key is the provider name (e.g., "spotify", "youtube"), value is the ID.
pub type ProviderTrackIds = HashMap<String, String>;

/// Information about the currently playing track
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackInfo {
    /// Music source this track came from
    pub source: MusicSource,
    /// Primary track ID from the source (source-specific format)
    pub source_track_id: String,
    /// Additional provider-specific IDs for lyrics lookup
    pub provider_ids: ProviderTrackIds,
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
        source: MusicSource,
        source_track_id: impl Into<String>,
        name: impl Into<String>,
        artist: impl Into<String>,
        album: impl Into<String>,
        duration: Duration,
    ) -> Self {
        Self {
            source,
            source_track_id: source_track_id.into(),
            provider_ids: HashMap::new(),
            name: name.into(),
            artist: artist.into(),
            album: album.into(),
            duration,
        }
    }

    /// Add a provider-specific track ID
    #[must_use]
    pub fn with_provider_id(mut self, provider: impl Into<String>, id: impl Into<String>) -> Self {
        self.provider_ids.insert(provider.into(), id.into());
        self
    }

    /// Get duration in seconds (for lyrics query).
    ///
    /// Saturates at `u32::MAX` (approximately 136 years), which is more than sufficient
    /// for any audio track.
    #[must_use]
    pub fn duration_secs(&self) -> u32 {
        self.duration.as_secs_u32()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_playback_state_default() {
        let state = PlaybackState::default();
        assert!(!state.is_playing);
        assert!(state.track.is_none());
        assert_eq!(state.position, Duration::ZERO);
        assert_eq!(state.duration, Duration::ZERO);
    }

    #[test]
    fn test_playback_state_new() {
        let track = TrackInfo::new(
            MusicSource::Spotify,
            "track123",
            "Test Song",
            "Test Artist",
            "Test Album",
            Duration::from_secs(180),
        );

        let state = PlaybackState::new(
            true,
            Some(track),
            Duration::from_secs(30),
            Duration::from_secs(180),
        );

        assert!(state.is_playing);
        assert!(state.track.is_some());
        assert_eq!(state.position, Duration::from_secs(30));
        assert_eq!(state.duration, Duration::from_secs(180));
    }

    #[test]
    fn test_interpolated_position_paused() {
        let state = PlaybackState {
            is_playing: false,
            track: None,
            position: Duration::from_secs(30),
            duration: Duration::from_secs(180),
            updated_at: Instant::now() - Duration::from_secs(5),
        };

        // When paused, position should not advance
        assert_eq!(state.interpolated_position(), Duration::from_secs(30));
    }

    #[test]
    fn test_interpolated_position_clamped() {
        let state = PlaybackState {
            is_playing: true,
            track: None,
            position: Duration::from_secs(178),
            duration: Duration::from_secs(180),
            updated_at: Instant::now() - Duration::from_secs(10), // 10 seconds ago
        };

        // Position should be clamped to duration
        assert_eq!(state.interpolated_position(), Duration::from_secs(180));
    }

    #[test]
    fn test_track_changed_same_track() {
        let track = TrackInfo::new(
            MusicSource::Spotify,
            "track123",
            "Song",
            "Artist",
            "Album",
            Duration::from_secs(180),
        );

        let state1 = PlaybackState::new(true, Some(track.clone()), Duration::ZERO, Duration::from_secs(180));
        let state2 = PlaybackState::new(true, Some(track), Duration::from_secs(30), Duration::from_secs(180));

        assert!(!state1.track_changed(&state2));
    }

    #[test]
    fn test_track_changed_different_track() {
        let track1 = TrackInfo::new(
            MusicSource::Spotify,
            "track123",
            "Song 1",
            "Artist",
            "Album",
            Duration::from_secs(180),
        );

        let track2 = TrackInfo::new(
            MusicSource::Spotify,
            "track456",
            "Song 2",
            "Artist",
            "Album",
            Duration::from_secs(200),
        );

        let state1 = PlaybackState::new(true, Some(track1), Duration::ZERO, Duration::from_secs(180));
        let state2 = PlaybackState::new(true, Some(track2), Duration::ZERO, Duration::from_secs(200));

        assert!(state1.track_changed(&state2));
    }

    #[test]
    fn test_track_changed_none_to_some() {
        let track = TrackInfo::new(
            MusicSource::Spotify,
            "track123",
            "Song",
            "Artist",
            "Album",
            Duration::from_secs(180),
        );

        let state1 = PlaybackState::default();
        let state2 = PlaybackState::new(true, Some(track), Duration::ZERO, Duration::from_secs(180));

        assert!(state1.track_changed(&state2));
    }

    #[test]
    fn test_track_changed_both_none() {
        let state1 = PlaybackState::default();
        let state2 = PlaybackState::default();

        assert!(!state1.track_changed(&state2));
    }

    #[test]
    fn test_playback_state_changed() {
        let state1 = PlaybackState {
            is_playing: true,
            ..Default::default()
        };
        let state2 = PlaybackState {
            is_playing: false,
            ..Default::default()
        };

        assert!(state1.playback_state_changed(&state2));
        assert!(!state1.playback_state_changed(&state1));
    }

    #[test]
    fn test_track_info_new() {
        let track = TrackInfo::new(
            MusicSource::Spotify,
            "track123",
            "Test Song",
            "Test Artist",
            "Test Album",
            Duration::from_secs(180),
        );

        assert_eq!(track.source, MusicSource::Spotify);
        assert_eq!(track.source_track_id, "track123");
        assert_eq!(track.name, "Test Song");
        assert_eq!(track.artist, "Test Artist");
        assert_eq!(track.album, "Test Album");
        assert_eq!(track.duration, Duration::from_secs(180));
        assert!(track.provider_ids.is_empty());
    }

    #[test]
    fn test_track_info_with_provider_id() {
        let track = TrackInfo::new(
            MusicSource::Spotify,
            "track123",
            "Song",
            "Artist",
            "Album",
            Duration::from_secs(180),
        )
        .with_provider_id("spotify", "spotify_track_id")
        .with_provider_id("lrclib", "lrclib_id");

        assert_eq!(track.provider_ids.get("spotify"), Some(&"spotify_track_id".to_string()));
        assert_eq!(track.provider_ids.get("lrclib"), Some(&"lrclib_id".to_string()));
    }

    #[test]
    fn test_track_info_duration_secs() {
        let track = TrackInfo::new(
            MusicSource::Spotify,
            "track123",
            "Song",
            "Artist",
            "Album",
            Duration::from_secs(183), // 3 minutes 3 seconds
        );

        assert_eq!(track.duration_secs(), 183);
    }
}
