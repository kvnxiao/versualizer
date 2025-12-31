pub mod cache;
pub mod config;
pub mod error;
pub mod fetcher;
pub mod lrc;
pub mod paths;
pub mod playback;
pub mod provider;
pub mod source;
pub mod sync;
pub mod time;

pub use cache::LyricsCache;
pub use config::{
    build_config_template, AnimationConfig, LayoutConfig, LyricsConfig, LyricsProviderType,
    MusicConfig, ProvidersConfig, UiConfig, VersualizerConfig,
};

/// Re-export toml error type for config parsing error handling
pub use toml::de::Error as TomlParseError;
pub use error::CoreError;
pub use fetcher::LyricsFetcher;
pub use lrc::{LrcFile, LrcLine, LrcMetadata, LrcWord};
pub use paths::{
    config_dir, theme_path, window_state_path, CONFIG_DIR_NAME, CONFIG_FILE_NAME,
    LYRICS_CACHE_DB_FILE_NAME, THEME_FILE_NAME, WINDOW_STATE_FILE_NAME,
};
pub use playback::{PlaybackState, TrackInfo};
pub use provider::{FetchedLyrics, LyricsProvider, LyricsQuery, LyricsResult};
pub use source::{MusicSource, MusicSourceProvider, MusicSourceProviderBuilder};
pub use sync::{SyncEngine, SyncEvent};
pub use time::DurationExt;
