# Versualizer

A cross-platform desktop lyrics visualizer with karaoke-style synchronized lyrics display.

## Features

- Real-time Spotify playback detection
- Karaoke-style animated lyrics with color-fill effect
- Multiple lyrics providers (LRCLIB, Spotify)
- Local SQLite caching for offline lyrics
- Always-on-top transparent overlay window
- CSS-based customizable theming with hot-reload

## Installation

Requires [Rust 1.85+](https://rustup.rs/), [dioxus-cli](https://dioxuslabs.com/learn/0.7/getting_started/#install-the-dioxus-cli), and [just](https://github.com/casey/just).

```bash
git clone https://github.com/kvnxiao/versualizer
cd versualizer
just bundle
```

## Spotify Setup

1. Create an app at [Spotify Developer Dashboard](https://developer.spotify.com/dashboard) with redirect URI `http://127.0.0.1:8888/callback`
1. Edit `~/.config/versualizer/config.toml` (created on first run) with your credentials:

```toml
[providers.spotify]
client_id = "your_client_id"
client_secret = "your_client_secret"
oauth_redirect_uri = "http://127.0.0.1:8888/callback"
```

1. Run the app - a browser window opens for OAuth authorization

## Customization

Edit `~/.config/versualizer/theme.css` to customize the overlay appearance. Changes are hot-reloaded.

## Development

See [DEVELOPMENT.md](DEVELOPMENT.md) for architecture, conventions, and commands.

## License

[MIT](./LICENSE)
