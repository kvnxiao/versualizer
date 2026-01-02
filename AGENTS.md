# Critical Instructions

**IMPORTANT**: before you do anything else, run the `beans prime` command and heed its output.

**IMPORTANT**: before completing any task, you MUST run the following commands and address any issues:

```bash
cargo clippy --workspace --all-targets --all-features
cargo fmt --check
```

- Fix all clippy warnings and errors before marking work as done
- Run `cargo fmt` to format code if `cargo fmt --check` reports formatting issues
- These checks are mandatory for every code change, no exceptions

# Versualizer

Real-time synchronized lyrics visualizer for Spotify.

## Project Overview

Versualizer is a desktop application that displays synchronized lyrics for the currently playing Spotify track. It uses a Dioxus-based UI with Tauri for the native desktop experience.

## Architecture

```
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

## Development

### Prerequisites

- Rust 1.85+ (edition 2024)
- Dioxus CLI: `cargo install dioxus-cli`

### Commands

```bash
# Run desktop app in dev mode
dx serve --platform desktop

# Check all crates
cargo clippy --workspace --all-targets --all-features

# Format code
cargo fmt

# Run tests
cargo test --workspace
```

### Configuration

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

## Rust Guidelines

General Rust development guidelines are in `~/.claude/rules/`:

- `rust-linting.md` - Clippy configuration
- `rust-error-handling.md` - Error types with thiserror/anyhow
- `rust-defensive-programming.md` - Input validation, builders, newtypes
- `rust-code-quality.md` - Code organization, enums, parameter types
- `rust-unsafe.md` - Avoiding unsafe code
- `rust-testing.md` - Test organization
- `rust-documentation.md` - Doc requirements
- `rust-performance.md` - Performance best practices
- `rust-dependencies.md` - Dependency management
- `rust-workspaces.md` - Multi-crate workspace patterns
