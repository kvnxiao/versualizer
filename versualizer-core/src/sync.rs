use crate::lrc::LrcFile;
use crate::playback::{PlaybackState, TrackInfo};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, RwLock};

/// Events emitted by the sync engine
#[derive(Debug, Clone)]
pub enum SyncEvent {
    /// Playback started for a track
    PlaybackStarted {
        track: TrackInfo,
        position: Duration,
    },
    /// Playback was paused
    PlaybackPaused {
        position: Duration,
    },
    /// Playback was resumed
    PlaybackResumed {
        position: Duration,
    },
    /// Playback stopped (no track playing)
    PlaybackStopped,
    /// Track changed to a new track
    TrackChanged {
        track: TrackInfo,
        position: Duration,
    },
    /// Regular position sync update
    PositionSync {
        position: Duration,
    },
    /// A seek occurred within the current track
    SeekOccurred {
        position: Duration,
    },
    /// Lyrics were loaded for current track
    LyricsLoaded {
        lyrics: LrcFile,
    },
    /// No lyrics found for current track
    LyricsNotFound,
    /// Error occurred
    Error {
        message: String,
    },
}

/// Sync engine state
struct SyncEngineInner {
    state: PlaybackState,
    lyrics: Option<LrcFile>,
}

/// Engine that synchronizes playback state and lyrics
pub struct SyncEngine {
    inner: RwLock<SyncEngineInner>,
    event_tx: broadcast::Sender<SyncEvent>,
}

impl SyncEngine {
    /// Create a new sync engine
    #[must_use] 
    pub fn new() -> Arc<Self> {
        let (event_tx, _) = broadcast::channel(64);

        Arc::new(Self {
            inner: RwLock::new(SyncEngineInner {
                state: PlaybackState::default(),
                lyrics: None,
            }),
            event_tx,
        })
    }

    /// Subscribe to sync events
    pub fn subscribe(&self) -> broadcast::Receiver<SyncEvent> {
        self.event_tx.subscribe()
    }

    /// Update playback state and emit appropriate events
    pub async fn update_state(&self, new_state: PlaybackState) {
        let mut inner = self.inner.write().await;
        let old_state = &inner.state;

        // Detect what changed
        let track_changed = old_state.track_changed(&new_state);
        let playback_changed = old_state.playback_state_changed(&new_state);
        let seek_occurred = old_state.seek_occurred(&new_state, Duration::from_secs(2));

        // Emit appropriate events
        if track_changed {
            // Clear lyrics for new/changed track
            inner.lyrics = None;

            if let Some(ref track) = new_state.track {
                let _ = self.event_tx.send(SyncEvent::TrackChanged {
                    track: track.clone(),
                    position: new_state.position,
                });
                // Also emit play state so listeners know if track is playing or paused
                if new_state.is_playing {
                    let _ = self.event_tx.send(SyncEvent::PlaybackResumed {
                        position: new_state.position,
                    });
                } else {
                    let _ = self.event_tx.send(SyncEvent::PlaybackPaused {
                        position: new_state.position,
                    });
                }
            } else {
                let _ = self.event_tx.send(SyncEvent::PlaybackStopped);
            }
        } else if playback_changed {
            if new_state.is_playing {
                if old_state.track.is_some() {
                    let _ = self.event_tx.send(SyncEvent::PlaybackResumed {
                        position: new_state.position,
                    });
                } else if let Some(ref track) = new_state.track {
                    let _ = self.event_tx.send(SyncEvent::PlaybackStarted {
                        track: track.clone(),
                        position: new_state.position,
                    });
                }
            } else {
                let _ = self.event_tx.send(SyncEvent::PlaybackPaused {
                    position: new_state.position,
                });
            }
        } else if seek_occurred {
            let _ = self.event_tx.send(SyncEvent::SeekOccurred {
                position: new_state.position,
            });
        } else {
            // Regular position update
            let _ = self.event_tx.send(SyncEvent::PositionSync {
                position: new_state.position,
            });
        }

        inner.state = new_state;
    }

    /// Set lyrics for the current track
    pub async fn set_lyrics(&self, lyrics: LrcFile) {
        self.inner.write().await.lyrics = Some(lyrics.clone());
        let _ = self.event_tx.send(SyncEvent::LyricsLoaded { lyrics });
    }

    /// Mark that no lyrics were found
    pub async fn set_no_lyrics(&self) {
        self.inner.write().await.lyrics = None;
        let _ = self.event_tx.send(SyncEvent::LyricsNotFound);
    }

    /// Emit an error event
    pub fn emit_error(&self, message: String) {
        let _ = self.event_tx.send(SyncEvent::Error { message });
    }

    /// Get current playback state
    pub async fn state(&self) -> PlaybackState {
        self.inner.read().await.state.clone()
    }

    /// Get current lyrics
    pub async fn lyrics(&self) -> Option<LrcFile> {
        self.inner.read().await.lyrics.clone()
    }

    /// Get interpolated current position
    pub async fn current_position(&self) -> Duration {
        self.inner.read().await.state.interpolated_position()
    }

    /// Check if currently playing
    pub async fn is_playing(&self) -> bool {
        self.inner.read().await.state.is_playing
    }

    /// Get current track info
    pub async fn current_track(&self) -> Option<TrackInfo> {
        self.inner.read().await.state.track.clone()
    }
}

impl Default for SyncEngine {
    fn default() -> Self {
        let (event_tx, _) = broadcast::channel(64);
        Self {
            inner: RwLock::new(SyncEngineInner {
                state: PlaybackState::default(),
                lyrics: None,
            }),
            event_tx,
        }
    }
}
