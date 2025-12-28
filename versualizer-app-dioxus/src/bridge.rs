use crate::state::{KaraokeState, LocalPlaybackTimer};
use dioxus::prelude::*;
use std::sync::Arc;
use tracing::info;
use versualizer_core::config::UiConfig;
use versualizer_core::{DurationExt, SyncEngine, SyncEvent};

/// Bridge `SyncEngine` events to Dioxus signals, with local playback timing.
///
/// This combines two responsibilities:
/// 1. Listens to `SyncEngine` events and updates the timer/karaoke state
/// 2. Runs a local timer loop that derives `current_index` from interpolated position
///
/// The timer approach (inspired by dioxus-motion) reduces re-renders by:
/// - Only hard-syncing on major events (play/pause/seek/track change)
/// - Using drift correction (300ms threshold) for regular position updates
/// - Locally computing line index at configured framerate instead of on every sync event
pub fn use_sync_engine_bridge(sync_engine: &Arc<SyncEngine>, karaoke: KaraokeState) {
    // Get UI config from context to read the configured framerate
    let ui_config: UiConfig = use_context();
    let framerate = ui_config.animation.framerate;

    // Create the local playback timer with configured framerate
    let timer = use_signal(|| LocalPlaybackTimer::new(framerate));

    // Clone once for the closure, then move into async block
    let sync_engine = sync_engine.clone();

    // Spawn the sync event listener
    use_future(move || {
        let sync_engine = sync_engine.clone();
        async move {
            let mut rx = sync_engine.subscribe();

            loop {
                match rx.recv().await {
                    Ok(event) => {
                        handle_sync_event(event, karaoke, timer);
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
    });

    // Spawn the local timer loop that updates current_index
    use_effect(move || {
        // Make karaoke mutable for the async block
        let mut karaoke = karaoke;
        spawn(async move {
            loop {
                // Read timer state into local variables to avoid holding borrow across await
                let (is_playing, poll_interval) = {
                    let t = timer.peek();
                    (t.is_playing(), t.active_poll_interval())
                };

                // Only update when playing and we have lyrics
                if is_playing {
                    if let Some(ref lyrics) = *karaoke.lyrics.peek() {
                        // Compute current position from local timer
                        let position_ms = timer.peek().interpolated_position_ms();

                        // Derive line index from position
                        let new_index = lyrics.line_index_at(position_ms);

                        // Only update signal if line actually changed (reduces re-renders)
                        if new_index != *karaoke.current_index.peek() {
                            karaoke.current_index.set(new_index);
                        }
                    }

                    // Active polling: at configured framerate for smooth line transitions
                    tokio::time::sleep(poll_interval).await;
                } else {
                    // Idle polling: reduced CPU usage when paused
                    tokio::time::sleep(LocalPlaybackTimer::IDLE_POLL_INTERVAL).await;
                }
            }
        });
    });
}

fn handle_sync_event(
    event: SyncEvent,
    mut karaoke: KaraokeState,
    mut timer: Signal<LocalPlaybackTimer>,
) {
    match event {
        // === Lyrics events ===
        SyncEvent::LyricsLoaded { lyrics } => {
            karaoke.set_lyrics(&lyrics);
            info!("Loaded {} precomputed lyric lines", lyrics.lines.len());
        }
        SyncEvent::LyricsNotFound => {
            karaoke.clear_lyrics();
        }

        // === Major events: hard sync position ===
        SyncEvent::PlaybackStarted { position, .. } | SyncEvent::PlaybackResumed { position } => {
            timer.write().hard_sync(position.as_millis_u64());
            timer.write().set_playing(true);
            karaoke.set_playing(true);
        }
        SyncEvent::PlaybackPaused { position } => {
            timer.write().hard_sync(position.as_millis_u64());
            timer.write().set_playing(false);
            karaoke.set_playing(false);
        }
        SyncEvent::SeekOccurred { position } => {
            // Seek is a major event: hard sync immediately
            timer.write().hard_sync(position.as_millis_u64());
        }
        SyncEvent::TrackChanged { .. } => {
            // Clear lyrics and reset timer
            karaoke.clear_lyrics();
            timer.write().hard_sync(0);
            timer.write().set_playing(false);
            karaoke.set_playing(false);
        }
        SyncEvent::PlaybackStopped => {
            karaoke.clear_lyrics();
            timer.write().hard_sync(0);
            timer.write().set_playing(false);
            karaoke.set_playing(false);
        }

        // === Drift correction: only sync if drift exceeds threshold ===
        SyncEvent::PositionSync { position } => {
            // drift_correct() only updates if drift > 300ms
            timer.write().drift_correct(position.as_millis_u64());
        }

        // === Errors ===
        SyncEvent::Error { .. } => {
            // Errors are logged elsewhere
        }
    }
}
