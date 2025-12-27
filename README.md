# Versualizer

A cross-platform desktop lyrics visualizer with karaoke-style synchronized lyrics display.

## Features

- 🎵 Real-time Spotify playback detection
- 📝 Synchronized lyrics from LRCLIB.net (stable interface to support other online lyrics API databases)
- 🎤 Karaoke-style color-fill animation (for a given line, "fill in" the line with a color gradient as the song progresses via timestamps and progress percentage interpolations)
  - This needs to look nice, so we need a UI implementaiton that supports text colors with animation keyframes (CSS or other native implementations that support it)
- 💾 Local SQLite caching of fetched lyrics
- 🪟 Always-on-top overlay window
  - Window itself needs to be fully transparent, no decorations, and supports being dragged around without window decoration framing
- 🔄 Multi-crate architecture for flexible UI implementations

## Setup

### 1. Get Spotify API Credentials

1. Go to [Spotify Developer Dashboard](https://developer.spotify.com/dashboard)
2. Log in with your Spotify account
3. Click "Create app"
4. Fill in:
   - **App name**: "Versualizer" (or any name)
   - **App description**: "Lyrics visualizer for Spotify"
   - **Redirect URI**: `http://localhost:8888/callback`
   - **API**: Select "Web API"
5. Accept terms and save
6. Click "Settings" to view your **Client ID** and **Client Secret**

### 2. Configure Versualizer

On first run, Versualizer will always create a configuration file at `~/.config/versualizer/config.toml` regardless of OS platform.

Edit this file and add your credentials:

```toml
# Versualizer Configuration
# Get your Spotify API credentials from: https://developer.spotify.com/dashboard

# The following fields are absolutely required
# spotify.client_id
# spotify.client_secret
# spotify.oauth_redirect_uri
[spotify]
# Your Spotify application client ID (required)
client_id = "your_client_id_here"
# Your Spotify application client secret (required)
client_secret = "your_client_secret_here"
# OAuth redirect URI - must match what you set in Spotify Dashboard (required)
oauth_redirect_uri = "http://localhost:8888/callback"
```

## Architecture

The project uses a multi-crate workspace design (no top-level crates folder, all crates kept at root repo level):

- **versualizer-core**: Core library containing lyrics fetching/caching, LRC parsing, playback state structure and "sync engine"
  - "Playback state" represents the current state of the media player (try to be platform agnostic here; we currently only support Spotify but maybe in the future we can support local media players on different OS platforms via detecting metadata from the current song file being played)
  - "Sync engine" dispatches events that modify the playback state to represent what is being played as close to realtime as possible, and also notify updates to the frontend via events
- **versualizer-spotify**: Spotify Web API integration for OAuth setup to enable playback state detection and play button timestamp syncing
  - This crate supports an OAuth setup functionality, where a localhost server is set up according to the spotify credentials from the config.toml file, and the server is closed upon successful authentication - where the successful response tokens are persisted for subsequent app restarts without needing to re-authenticate via the local OAuth server.
  - Successful web API playback state sync should notify the sync engine in versualizer-core if it is out of date.
- **versualizer-app-\***: Desktop application using a GUI library that supports all the necessary karaoke text effects

This architecture allows easy implementation of different UI frontends while sharing the core functionality.

## Sync algorithm

### Playback state detection

Playback state should be tracked in the following states:

- Playing
- Paused / stopped

And events should exist and be dispatched to the UI every recurring polling sync. All events should contain the current playback timestamp and the max song time in seconds for the current song.

- Playback started (prev state was paused or stopped, now playing)
- Playback stopped or paused (prev state was playing, now stopped or paused)
- Next track started (prev state was playing a different song regardless of playback state)
- Previous track started (prev state was playing the same song regardless of playback state)
- Simple timestamp sync (to update the playback timestamp)

NOTE: Since syncing with Spotify is an async network request, we should factor in the request-response latency into the timestamp sync.

### Timing

- When starting up the app for the first time, authenticate to Spotify's Web API via OAuth or use the cached secret tokens if available
- Then immediately sync the playback state on spotify to the sync engine's state

## License

MIT
