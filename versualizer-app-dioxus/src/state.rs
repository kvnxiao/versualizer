use dioxus::prelude::*;
use std::time::Instant;
use versualizer_core::LrcFile;

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

/// Precomputed lyrics with all timing information
#[derive(Clone, Debug, Default)]
pub struct PrecomputedLyrics {
    /// All lines with their timing info
    pub lines: Vec<TimedLine>,
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

            lines.push(TimedLine {
                text: line.text.clone(),
                start_time_ms,
                duration_ms,
            });
        }

        Self { lines }
    }

    /// Find the line index for a given position in milliseconds
    #[must_use]
    pub fn line_index_at(&self, position_ms: u64) -> Option<usize> {
        // Find the last line that started before or at the current position
        self.lines
            .iter()
            .enumerate()
            .rev()
            .find(|(_, line)| line.start_time_ms <= position_ms)
            .map(|(i, _)| i)
    }
}

/// Karaoke display state with precomputed lyrics for efficient UI-driven animation.
///
/// The UI receives all lyrics upfront and drives line transitions locally based on
/// timing information. Position sync events only update the reference point for
/// drift correction.
#[derive(Clone, Copy)]
pub struct KaraokeState {
    /// All precomputed lines for the current track
    pub lyrics: Signal<Option<PrecomputedLyrics>>,
    /// Current line index
    pub current_index: Signal<Option<usize>>,
    /// Reference position from last sync event (milliseconds)
    pub reference_position_ms: Signal<u64>,
    /// When we received the reference position (for local interpolation)
    pub reference_instant: Signal<Option<Instant>>,
    /// Whether playback is active
    pub is_playing: Signal<bool>,
}

impl KaraokeState {
    /// Create a new karaoke state with default values
    #[must_use]
    pub fn new() -> Self {
        Self {
            lyrics: Signal::new(None),
            current_index: Signal::new(None),
            reference_position_ms: Signal::new(0),
            reference_instant: Signal::new(None),
            is_playing: Signal::new(false),
        }
    }

    /// Set lyrics from an LRC file, precomputing all timing info
    pub fn set_lyrics(&mut self, lrc: &LrcFile) {
        let precomputed = PrecomputedLyrics::from_lrc(lrc);
        self.lyrics.set(Some(precomputed));
        // Reset position tracking
        self.current_index.set(None);
        self.reference_position_ms.set(0);
        self.reference_instant.set(None);
    }

    /// Clear lyrics (no lyrics available or track changed)
    pub fn clear_lyrics(&mut self) {
        self.lyrics.set(None);
        self.current_index.set(None);
        self.reference_position_ms.set(0);
        self.reference_instant.set(None);
    }

    /// Update the reference position (from sync events)
    /// This also updates the current line index based on the position
    pub fn sync_position(&mut self, position_ms: u64) {
        self.reference_position_ms.set(position_ms);
        self.reference_instant.set(Some(Instant::now()));

        // Update current line index based on position
        if let Some(ref lyrics) = *self.lyrics.read() {
            let new_index = lyrics.line_index_at(position_ms);
            self.current_index.set(new_index);
        }
    }

    /// Set the playing state
    pub fn set_playing(&mut self, playing: bool) {
        self.is_playing.set(playing);
        if playing {
            // Reset the reference instant when resuming to avoid jump
            self.reference_instant.set(Some(Instant::now()));
        }
    }

    /// Get visible lines around the current position
    #[must_use]
    pub fn visible_lines(&self, before: usize, after: usize) -> Vec<TimedLine> {
        let lyrics = self.lyrics.read();
        let current_idx = *self.current_index.read();

        let Some(ref lyrics) = *lyrics else {
            return Vec::new();
        };

        let Some(idx) = current_idx else {
            // No current line yet, return first few lines if available
            return lyrics.lines.iter().take(after + 1).cloned().collect();
        };

        let start = idx.saturating_sub(before);
        let end = (idx + after + 1).min(lyrics.lines.len());

        lyrics.lines[start..end].to_vec()
    }
}

impl Default for KaraokeState {
    fn default() -> Self {
        Self::new()
    }
}
