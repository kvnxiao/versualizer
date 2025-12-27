use dioxus::prelude::*;
use std::time::{Duration, Instant};
use tracing::{debug, trace};
use versualizer_core::LrcFile;

/// UI display configuration for karaoke rendering.
/// These values control line visibility, scaling, and animation timing.
#[derive(Clone, Debug)]
pub struct KaraokeDisplayConfig {
    /// Number of visible lines (1-3), excluding buffer lines
    pub max_lines: usize,
    /// Scale factor for the current line (e.g., 1.0)
    pub current_line_scale: f32,
    /// Scale factor for upcoming/buffer lines (e.g., 0.8)
    pub upcoming_line_scale: f32,
    /// Transition duration in milliseconds
    pub transition_ms: u32,
    /// CSS easing function (e.g., "ease-in-out")
    pub easing: String,
}

impl Default for KaraokeDisplayConfig {
    fn default() -> Self {
        Self {
            max_lines: 3,
            current_line_scale: 1.0,
            upcoming_line_scale: 0.8,
            transition_ms: 200,
            easing: "ease-in-out".into(),
        }
    }
}

/// Convert u128 milliseconds to u64, saturating at `u64::MAX`.
/// In practice, this is safe because song durations never exceed `u64::MAX` milliseconds
/// (which would be ~584 million years).
fn millis_to_u64(millis: u128) -> u64 {
    u64::try_from(millis).unwrap_or(u64::MAX)
}

/// A precomputed lyric line with all timing info needed for UI animation
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TimedLine {
    /// The lyric text
    pub text: String,
    /// When this line starts (milliseconds from track start)
    pub start_time_ms: u64,
    /// Duration until the next line starts (milliseconds)
    pub duration_ms: u64,
}

/// Sentinel value indicating we're in the instrumental intro (before first lyric line)
pub const INTRO_LINE_INDEX: i32 = -1;

/// Music note character for instrumental sections
const MUSIC_NOTE: &str = "\u{266A}"; // ♪

/// Precomputed lyrics with all timing information
#[derive(Clone, Debug, Default)]
pub struct PrecomputedLyrics {
    /// All lines with their timing info
    pub lines: Vec<TimedLine>,
    /// Duration of instrumental intro (0 if lyrics start at beginning)
    pub intro_duration_ms: u64,
}

impl PrecomputedLyrics {
    /// Create precomputed lyrics from an LRC file
    #[must_use]
    pub fn from_lrc(lrc: &LrcFile) -> Self {
        let mut lines = Vec::with_capacity(lrc.lines.len());

        for (i, line) in lrc.lines.iter().enumerate() {
            let start_time_ms = millis_to_u64(line.start_time.as_millis());

            // Duration is time until next line, or default 5 seconds for last line
            let duration_ms = if i + 1 < lrc.lines.len() {
                let next_start = millis_to_u64(lrc.lines[i + 1].start_time.as_millis());
                next_start.saturating_sub(start_time_ms)
            } else {
                5000 // Default 5 seconds for the last line
            };

            // Use music note for empty/whitespace-only lines (instrumental breaks)
            let text = if line.text.trim().is_empty() {
                MUSIC_NOTE.into()
            } else {
                line.text.clone()
            };

            lines.push(TimedLine {
                text,
                start_time_ms,
                duration_ms,
            });
        }

        // Calculate intro duration (time before first line starts)
        let intro_duration_ms = lines.first().map_or(0, |l| l.start_time_ms);

        Self {
            lines,
            intro_duration_ms,
        }
    }

    /// Find the line index for a given position in milliseconds.
    /// Returns `INTRO_LINE_INDEX` (-1) if we're before the first line starts.
    #[must_use]
    pub fn line_index_at(&self, position_ms: u64) -> i32 {
        // Find the last line that started before or at the current position
        self.lines
            .iter()
            .enumerate()
            .rev()
            .find(|(_, line)| line.start_time_ms <= position_ms)
            .map_or(INTRO_LINE_INDEX, |(i, _)| {
                // Safe: line count is always much less than i32::MAX
                #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
                let idx = i as i32;
                idx
            })
    }

    /// Create a virtual "intro line" with music note for the instrumental intro period
    #[must_use]
    pub fn intro_line(&self) -> TimedLine {
        TimedLine {
            text: MUSIC_NOTE.into(),
            start_time_ms: 0,
            duration_ms: self.intro_duration_ms,
        }
    }

    /// Check if there's an instrumental intro (first line doesn't start at 0)
    #[must_use]
    pub const fn has_intro(&self) -> bool {
        self.intro_duration_ms > 0
    }
}

/// Karaoke display state with precomputed lyrics for efficient UI-driven animation.
///
/// The UI receives all lyrics upfront. Line transitions are driven by `LocalPlaybackTimer`
/// which updates `current_index` based on locally interpolated playback position.
/// This decouples UI updates from network sync events for smoother animation.
#[derive(Clone, Copy)]
pub struct KaraokeState {
    /// All precomputed lines for the current track
    pub lyrics: Signal<Option<PrecomputedLyrics>>,
    /// Current line index (-1 = intro/before first line, 0+ = actual line index)
    /// Updated by the local playback timer loop, not directly by sync events
    pub current_index: Signal<i32>,
    /// Whether playback is active (used by UI for animation state)
    pub is_playing: Signal<bool>,
}

impl KaraokeState {
    /// Create a new karaoke state with default values
    #[must_use]
    pub fn new() -> Self {
        Self {
            lyrics: Signal::new(None),
            current_index: Signal::new(INTRO_LINE_INDEX),
            is_playing: Signal::new(false),
        }
    }

    /// Set lyrics from an LRC file, precomputing all timing info
    pub fn set_lyrics(&mut self, lrc: &LrcFile) {
        let precomputed = PrecomputedLyrics::from_lrc(lrc);
        self.lyrics.set(Some(precomputed));
        // Reset to intro state - timer will update current_index
        self.current_index.set(INTRO_LINE_INDEX);
    }

    /// Clear lyrics (no lyrics available or track changed)
    pub fn clear_lyrics(&mut self) {
        self.lyrics.set(None);
        self.current_index.set(INTRO_LINE_INDEX);
    }

    /// Set the playing state
    pub fn set_playing(&mut self, playing: bool) {
        self.is_playing.set(playing);
    }

    /// Get visible lines around the current position.
    /// When in intro (idx < 0), returns intro line + first few actual lines.
    /// When on a line (idx >= 0), returns lines around the current position.
    #[must_use]
    pub fn visible_lines(&self, before: usize, after: usize) -> Vec<TimedLine> {
        let lyrics = self.lyrics.read();
        let current_idx = *self.current_index.read();

        let Some(ref lyrics) = *lyrics else {
            return Vec::new();
        };

        if current_idx < 0 {
            // In intro: show intro line (music note) + upcoming lines
            let mut result = Vec::with_capacity(1 + after);
            if lyrics.has_intro() {
                result.push(lyrics.intro_line());
            }
            // Add upcoming actual lines
            result.extend(lyrics.lines.iter().take(after).cloned());
            result
        } else {
            // On an actual line
            #[allow(clippy::cast_sign_loss)]
            let idx = current_idx as usize;
            let start = idx.saturating_sub(before);
            let end = (idx + after + 1).min(lyrics.lines.len());
            lyrics.lines[start..end].to_vec()
        }
    }
}

impl Default for KaraokeState {
    fn default() -> Self {
        Self::new()
    }
}

/// Local playback timer that tracks position independently between sync events.
///
/// Inspired by dioxus-motion's timing approach: maintains a reference point and
/// interpolates position locally, only hard-syncing on major events (play/pause/seek)
/// and using drift correction for regular position updates.
#[derive(Clone, Debug)]
pub struct LocalPlaybackTimer {
    /// Position at the last sync point (milliseconds)
    reference_position_ms: u64,
    /// When we received the reference position
    reference_instant: Instant,
    /// Whether playback is currently active
    is_playing: bool,
}

/// Log target for timer-related messages
const TIMER_LOG_TARGET: &str = "versualizer::timer";

impl LocalPlaybackTimer {
    /// Drift threshold in milliseconds. If local and server positions differ by more
    /// than this amount, we hard-sync. Otherwise, we trust our local timer.
    /// 300ms tolerates ~2-3 poll intervals of cumulative drift while keeping lyrics
    /// visually in sync (less than a syllable of error).
    const DRIFT_THRESHOLD_MS: u64 = 300;

    /// Polling interval when playback is active (targeting ~60fps for smooth updates)
    pub const ACTIVE_POLL_INTERVAL: Duration = Duration::from_millis(16);

    /// Polling interval when playback is idle (reduced CPU usage)
    pub const IDLE_POLL_INTERVAL: Duration = Duration::from_millis(100);

    /// Create a new timer starting at position 0, paused
    #[must_use]
    pub fn new() -> Self {
        Self {
            reference_position_ms: 0,
            reference_instant: Instant::now(),
            is_playing: false,
        }
    }

    /// Get the current interpolated position in milliseconds.
    /// When playing, adds elapsed time since last sync to the reference position.
    /// When paused, returns the reference position unchanged.
    #[must_use]
    pub fn interpolated_position_ms(&self) -> u64 {
        if self.is_playing {
            let elapsed_ms = self.reference_instant.elapsed().as_millis();
            // Safe: song durations never exceed u64::MAX milliseconds
            #[allow(clippy::cast_possible_truncation)]
            let elapsed = elapsed_ms as u64;
            self.reference_position_ms.saturating_add(elapsed)
        } else {
            self.reference_position_ms
        }
    }

    /// Hard sync to a specific position. Used for major events like
    /// play/pause/seek where we want to immediately match server state.
    pub fn hard_sync(&mut self, position_ms: u64) {
        self.reference_position_ms = position_ms;
        self.reference_instant = Instant::now();
    }

    /// Apply drift correction if the server position differs significantly.
    /// Only syncs if the drift exceeds `DRIFT_THRESHOLD_MS`, otherwise
    /// trusts the local timer to avoid unnecessary jumps.
    ///
    /// Returns `true` if a correction was applied.
    pub fn drift_correct(&mut self, server_position_ms: u64) -> bool {
        let local = self.interpolated_position_ms();
        let drift = server_position_ms.abs_diff(local);

        // Determine drift direction for logging
        let drift_direction = if server_position_ms > local {
            "behind"
        } else {
            "ahead"
        };

        if drift > Self::DRIFT_THRESHOLD_MS {
            debug!(
                target: TIMER_LOG_TARGET,
                "Drift correction applied: local={}ms, server={}ms, drift={}ms ({}) > threshold={}ms",
                local, server_position_ms, drift, drift_direction, Self::DRIFT_THRESHOLD_MS
            );
            self.hard_sync(server_position_ms);
            true
        } else {
            // Small drift: ignore, local timer is accurate enough
            trace!(
                target: TIMER_LOG_TARGET,
                "Drift within threshold: local={}ms, server={}ms, drift={}ms ({}) <= threshold={}ms",
                local, server_position_ms, drift, drift_direction, Self::DRIFT_THRESHOLD_MS
            );
            false
        }
    }

    /// Set the playing state, handling the transition properly.
    /// - When resuming: resets the reference instant to avoid time jumps
    /// - When pausing: captures the current position as the new reference
    pub fn set_playing(&mut self, playing: bool) {
        if playing && !self.is_playing {
            // Resuming: reset instant so elapsed time starts from 0
            self.reference_instant = Instant::now();
        } else if !playing && self.is_playing {
            // Pausing: capture current interpolated position
            self.reference_position_ms = self.interpolated_position_ms();
            self.reference_instant = Instant::now();
        }
        self.is_playing = playing;
    }

    /// Check if playback is currently active
    #[must_use]
    pub const fn is_playing(&self) -> bool {
        self.is_playing
    }
}

impl Default for LocalPlaybackTimer {
    fn default() -> Self {
        Self::new()
    }
}
