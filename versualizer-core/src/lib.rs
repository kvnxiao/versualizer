pub mod cache;
pub mod config;
pub mod error;
pub mod lrc;
pub mod paths;
pub mod playback;
pub mod provider;
pub mod providers;
pub mod sync;

pub use cache::LyricsCache;
pub use config::Config;
pub use error::VersualizerError;
pub use lrc::{LrcFile, LrcLine, LrcWord};
pub use paths::{CONFIG_DIR_NAME, CONFIG_FILE_NAME, LYRICS_CACHE_DB_FILE_NAME};
pub use playback::{PlaybackState, TrackInfo};
pub use provider::{FetchedLyrics, LyricsProvider, LyricsQuery, LyricsResult};
pub use sync::{SyncEngine, SyncEvent};
