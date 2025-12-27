use crate::state::KaraokeState;
use dioxus::prelude::*;
use std::sync::Arc;
use tracing::info;
use versualizer_core::{DurationExt, SyncEngine, SyncEvent};

const LOG_TARGET: &str = "versualizer::bridge";

/// Bridge `SyncEngine` events to Dioxus signals.
/// This function spawns an async task that listens to `SyncEngine` events
/// and updates the karaoke state signals accordingly.
pub fn use_sync_engine_bridge(sync_engine: Arc<SyncEngine>, karaoke: KaraokeState) {
    use_future(move || {
        let sync_engine = sync_engine.clone();
        async move {
            let mut rx = sync_engine.subscribe();

            loop {
                match rx.recv().await {
                    Ok(event) => {
                        handle_sync_event(event, karaoke);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        info!(target: LOG_TARGET, "Sync event channel closed");
                        break;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        info!(target: LOG_TARGET, "Missed {} sync events", n);
                    }
                }
            }
        }
    });
}

fn handle_sync_event(event: SyncEvent, mut karaoke: KaraokeState) {
    match event {
        SyncEvent::LyricsLoaded { lyrics } => {
            // Precompute all lines with timing and store in state
            karaoke.set_lyrics(&lyrics);
            info!(
                target: LOG_TARGET,
                "Loaded {} precomputed lyric lines",
                lyrics.lines.len()
            );
        }
        SyncEvent::LyricsNotFound => {
            karaoke.clear_lyrics();
        }
        SyncEvent::PositionSync { position } => {
            // Update reference position for drift correction
            karaoke.sync_position(position.as_millis_u64());
        }
        SyncEvent::SeekOccurred { position } => {
            // Seek requires immediate position update
            karaoke.sync_position(position.as_millis_u64());
        }
        SyncEvent::PlaybackStarted { position, .. }
        | SyncEvent::PlaybackResumed { position } => {
            karaoke.sync_position(position.as_millis_u64());
            karaoke.set_playing(true);
        }
        SyncEvent::PlaybackPaused { position } => {
            karaoke.sync_position(position.as_millis_u64());
            karaoke.set_playing(false);
        }
        SyncEvent::TrackChanged { .. } => {
            // Clear lyrics until new ones load
            karaoke.clear_lyrics();
            karaoke.set_playing(false);
        }
        SyncEvent::PlaybackStopped => {
            karaoke.clear_lyrics();
            karaoke.set_playing(false);
        }
        SyncEvent::Error { .. } => {
            // Errors are logged elsewhere
        }
    }
}
