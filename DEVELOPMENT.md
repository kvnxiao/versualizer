# Development Guide

## Project Overview

Versualizer is a desktop application that displays synchronized lyrics for the currently playing Spotify track. It uses a Dioxus-based UI with Tauri for the native desktop experience.

## Architecture

```text
versualizer/
├── versualizer-core/           # Core library: playback, caching, LRC parsing
├── versualizer-app-dioxus/     # Dioxus desktop app (Tauri-based)
├── versualizer-spotify-api/    # Spotify OAuth and API client
├── versualizer-lyrics-lrclib/  # LRCLIB lyrics provider
└── versualizer-lyrics-spotify/ # Spotify lyrics provider (internal API)
```

### Crate Responsibilities

- **versualizer-core**: Playback state management, lyrics caching (SQLite), LRC parsing, time synchronization
- **versualizer-app-dioxus**: UI components, window management, theme switching
- **versualizer-spotify-api**: OAuth flow, token management, playback polling
- **versualizer-lyrics-lrclib**: External lyrics fetching from LRCLIB API
- **versualizer-lyrics-spotify**: Spotify's internal lyrics API integration

## Prerequisites

- [Rust 1.85+](https://rustup.rs/) (edition 2024)
- [just](https://github.com/casey/just) - command runner
- [dioxus-cli](https://dioxuslabs.com/learn/0.7/getting_started/#install-the-dioxus-cli) - for bundling

## Commands

```bash
just dev      # Run desktop app in dev mode
just lint     # Run clippy and check formatting
just fmt      # Format code
just test     # Run tests
just bundle   # Create release bundle
```

See the [justfile](justfile) for all available commands.

## Configuration

User config stored at platform-specific config directory:

- Windows: `%APPDATA%\versualizer\config.toml`
- macOS: `~/Library/Application Support/versualizer/config.toml`
- Linux: `~/.config/versualizer/config.toml`

## Conventions

### Async Runtime

All async code uses Tokio. The Dioxus app spawns a background Tokio runtime for API calls and playback polling.

### Error Handling

Each crate defines its own error type in `error.rs` using `thiserror`. Cross-crate errors use `#[from]` for conversion.

### Lyrics Providers

Implement the `LyricsProvider` trait from `versualizer-core`:

```rust
#[async_trait]
pub trait LyricsProvider: Send + Sync {
    async fn fetch(&self, track: &TrackInfo) -> Result<Option<Lyrics>>;
}
```

### State Management

UI state flows through Dioxus signals. Background tasks communicate via channels (`futures::channel::mpsc`).
