# Versualizer

A cross-platform desktop lyrics visualizer with karaoke-style synchronized lyrics display.

![Rust](https://img.shields.io/badge/rust-1.75+-orange.svg)
![License](https://img.shields.io/badge/license-MIT-blue.svg)

## Features

- Real-time Spotify playback detection via Web API
- Karaoke-style animated lyrics with color-fill effect
- Multiple lyrics providers (LRCLIB, Spotify)
- Local SQLite caching for offline lyrics
- Always-on-top transparent overlay window
- CSS-based customizable theming with hot-reload support via `~/.config/versualizer/theme.css`

## Installation

### From Source

Requires [Rust 1.75+](https://rustup.rs/).

```bash
git clone https://github.com/kvnxiao/versualizer
cd versualizer
cargo install --path ./versualizer-app-dioxus
```

The binary will be installed to your cargo bin folder (e.g. `~/.cargo/bin/`).

## Setup with Spotify

### 1. Spotify API Credentials

1. Go to [Spotify Developer Dashboard](https://developer.spotify.com/dashboard)
2. Create a new app with:
   - **App name**: Versualizer
   - **Redirect URI**: `http://127.0.0.1:8888/callback`
   - **API**: Web API
3. Note your **Client ID** and **Client Secret**

### 2. Configuration

On first run, a config file is created at `~/.config/versualizer/config.toml`.

Edit it with your Spotify credentials:

```toml
# ...
[providers.spotify]
client_id = "your_client_id"
client_secret = "your_client_secret"
oauth_redirect_uri = "http://127.0.0.1:8888/callback"
# ...
```

### 3. First Run

Run the app. On first launch, a browser window opens for Spotify OAuth. After authorizing, tokens are cached for future sessions.

```bash
./target/release/versualizer-dioxus
```

## Customization

Customize the overlay appearance by editing `~/.config/versualizer/theme.css`. Changes are hot-reloaded.

## Development

### Prerequisites

- Rust 1.75+
- Platform-specific dependencies for [Dioxus desktop](https://dioxuslabs.com/learn/0.6/getting_started#platform-specific-dependencies)

### Build & Run

```bash
# Development build
cargo run -p versualizer-app-dioxus

# Release build
cargo build --release

# Run tests
cargo test --workspace

# Run clippy
cargo clippy --workspace
```

### Project Structure

```
versualizer/
├── versualizer-core/           # Core library: sync engine, caching, LRC parsing
├── versualizer-spotify-api/    # Spotify Web API OAuth & playback polling
├── versualizer-lyrics-lrclib/  # LRCLIB.net lyrics provider
├── versualizer-lyrics-spotify/ # Spotify lyrics provider (unofficial)
└── versualizer-app-dioxus/     # Desktop app (Dioxus)
```

## License

[MIT](./LICENSE)
