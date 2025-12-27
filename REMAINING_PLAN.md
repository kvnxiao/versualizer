# Versualizer - Remaining Implementation Plan

This document tracks the remaining work based on comparing [PLAN.md](PLAN.md) against the current codebase state.

## Implementation Status Summary

| Milestone                            | Status      | Progress |
| ------------------------------------ | ----------- | -------- |
| **Milestone 1: Core Foundation**     | Complete    | 100%     |
| **Milestone 2: Lyrics Providers**    | Complete    | 100%     |
| **Milestone 3: Spotify Integration** | Complete    | 100%     |
| **Milestone 4: Freya UI**            | Partial     | ~70%     |
| **Milestone 5: Polish**              | Not Started | 0%       |

---

## Completed Features

### Milestone 1: Core Foundation

- [x] Workspace `Cargo.toml` setup
- [x] `config.rs` - TOML parsing with defaults, first-run detection, template generation
- [x] `error.rs` - Complete error types with `thiserror`
- [x] `paths.rs` - Cross-platform config paths
- [x] `lrc.rs` - Full LRC parser (Simple + Enhanced formats) with 13 unit tests
- [x] `cache.rs` - SQLite lyrics cache with `tokio-rusqlite`
- [x] `playback.rs` - Playback state with position interpolation
- [x] `provider.rs` - `LyricsProvider` trait and query types
- [x] `sync.rs` - Event-driven sync engine with broadcast channels

### Milestone 2: Lyrics Providers

- [x] `LrclibProvider` - Full implementation with exact match + search fallback
- [x] `SpotifyLyricsProvider` - Unofficial API with SP_DC cookie support
- [x] Provider fallback chain wired to cache

### Milestone 3: Spotify Integration

- [x] `oauth.rs` - Full OAuth flow with Axum server, token persistence, refresh
- [x] `poller.rs` - Playback polling with cancellation, latency compensation, exponential backoff
- [x] `LyricsFetcher` - Automatic lyrics fetching on track changes

### Milestone 4: Freya UI (Partial)

- [x] Transparent, borderless, always-on-top window setup
- [x] CJK font fallbacks configured
- [x] RadioStation-based state management
- [x] SyncEngine -> UI event bridging
- [x] Basic `KaraokeLineComponent` with gradient fill animation
- [x] Progress-based clip rendering (gradient fill mode)

---

## Remaining Work

### Milestone 4: Freya UI - Remaining Tasks

#### 1. Character-Level Fill Mode

**Status:** Not implemented
**Location:** [app.rs](versualizer-app-freya/src/app.rs)
**Plan Reference:** Section 3.2 - `karaoke_line` function

The current implementation only supports gradient (pixel-level) fill. Character-level fill requires:

- Rendering each character as a separate span
- Calculating per-character progress threshold
- Coloring characters based on whether progress has passed their threshold

```rust
// Per PLAN.md - character-level fill approach
fn character_span(
    character: char,
    char_index: usize,
    total_chars: usize,
    progress: f32,
    config: &UiConfig,
) -> impl IntoElement {
    let char_threshold = char_index as f32 / total_chars as f32;
    let color = if progress >= char_threshold {
        config.sung_color
    } else {
        config.unsung_color
    };
    // ...
}
```

#### 2. UI Config Integration

**Status:** Hardcoded values
**Location:** [app.rs:123-126](versualizer-app-freya/src/app.rs#L123-L126)

Currently uses hardcoded colors and font settings instead of reading from `Config`:

```rust
// Current (hardcoded):
let sung_color = parse_hex_color("#00FF00");
let unsung_color = parse_hex_color("#FFFFFF");
let font_size = 36.0f32;

// Should read from config:
let sung_color = parse_hex_color(&config.ui.sung_color);
let unsung_color = parse_hex_color(&config.ui.unsung_color);
let font_size = config.ui.font.size as f32;
```

#### 3. Multi-Line Lyrics Display

**Status:** Single line only
**Location:** [app.rs](versualizer-app-freya/src/app.rs)
**Plan Reference:** Section 3.3 - `LayoutConfig.max_lines_visible`

Current implementation shows only the current line. Plan specifies:

- Configurable `max_lines_visible` (default: 3)
- `current_line_scale` for emphasized active line
- Upcoming/previous lines displayed with different styling

#### 4. Draggable Overlay Component

**Status:** Not implemented
**Location:** Should be in `versualizer-app-freya/src/components/draggable_overlay.rs`
**Plan Reference:** Section 3.4

Per PLAN.md, requires:

```rust
fn draggable_overlay(children: impl IntoElement) -> impl IntoElement {
    let platform = Platform::get();
    rect()
        .on_press(move |_| {
            platform.with_window(None, |window| {
                let _ = window.drag_window();
            });
        })
        .child(children)
}
```

#### 5. Hide Overlay When No Lyrics

**Status:** Partial - shows semi-transparent background always
**Location:** [app.rs:142](versualizer-app-freya/src/app.rs#L142)
**Plan Reference:** Section 3.5, Config `[behavior] no_lyrics_behavior`

Current implementation always shows a semi-transparent background. Should:

- Check if lyrics are available
- Return empty transparent rect when no lyrics (per `no_lyrics_behavior` config)

#### 6. Fill Mode Toggle

**Status:** Only gradient mode implemented
**Location:** [app.rs](versualizer-app-freya/src/app.rs)
**Plan Reference:** Config `[ui] fill_mode`

Config supports `fill_mode = "character" | "gradient"` but only gradient is implemented.

---

### Milestone 5: Polish - Not Started

#### 1. Global Hotkeys

**Status:** Not implemented
**Plan Reference:** Section "Global Hotkeys"

Dependencies are configured (`global-hotkey` crate). Needs:

- Hotkey string parser (e.g., "Ctrl+Shift+L" -> `HotKey`)
- Registration of user-configured hotkeys from config
- Handlers for toggle visibility, quit, sync offset adjustment

#### 2. Window Position Persistence

**Status:** Not implemented
**Plan Reference:** Section "Window Position Persistence"

Needs:

- `WindowState` struct (x, y, width, height, monitor)
- Save position on window move/close
- Restore position on app launch
- Store at `~/.config/versualizer/window_state.json`

#### 3. Graceful Shutdown Handling

**Status:** Partial
**Plan Reference:** Section "Shutdown & Cleanup"

`CancellationToken` is used in poller, but full shutdown sequence is not implemented:

- Signal all background tasks to stop
- Wait for poller completion with timeout
- Flush cache WAL (`checkpoint()` method exists but not called)
- Save window state

#### 4. Error Notification UI

**Status:** Not implemented
**Plan Reference:** Section "Graceful Degradation"

Need UI components to show:

- Auth failure notification with retry button
- "No active playback" message (configurable via `no_lyrics_behavior`)
- Network offline indicator
- Rate limit warnings

#### 5. Logging/Tracing Polish

**Status:** Basic implementation
**Location:** [main.rs:23-27](versualizer-app-freya/src/main.rs#L23-L27)

Current: Basic tracing-subscriber setup. Needs:

- Log file output option
- Configurable log levels per module
- Structured logging for debugging playback sync issues

#### 6. System Tray (Optional)

**Status:** Not implemented
**Plan Reference:** Section "System Tray"

Marked as optional in plan. Would require `tray-icon` crate:

- Show/Hide toggle
- Settings access
- Quit option

#### 7. Cross-Platform Testing

**Status:** Not done

Test on:

- [ ] Windows
- [ ] macOS
- [ ] Linux (X11)
- [ ] Linux (Wayland)

#### 8. Performance Optimization

**Status:** Not addressed

Potential areas:

- Reduce UI redraws when position updates don't change visible line
- Optimize lyrics lookup in large cache
- Profile animation frame rate

---

## Missing Components (per PLAN.md file structure)

The following files mentioned in PLAN.md don't exist:

```
versualizer-app-freya/src/components/
├── mod.rs                  ❌ Not created
├── karaoke_line.rs         ❌ (inline in app.rs)
└── draggable_overlay.rs    ❌ Not created
```

---

## Known Issues / Technical Debt

1. **Config not passed to UI components** - `App` struct doesn't receive config, uses hardcoded values
2. **No components directory** - All UI code is in single `app.rs` file
3. **Background color hardcoded** - Uses `rgba(0,0,0,0.5)` instead of `config.ui.background_color`
4. **No error recovery UI** - Errors logged but not shown to user
5. **Missing unit tests for providers** - LRCLIB and Spotify providers lack tests

---

## Recommended Priority Order

1. **UI Config Integration** - Low effort, high impact (use actual config values)
2. **Hide Overlay When No Lyrics** - Essential UX feature
3. **Draggable Overlay** - Essential for user positioning
4. **Window Position Persistence** - Quality of life
5. **Global Hotkeys** - Important for usability
6. **Character-Level Fill Mode** - Nice to have alternative
7. **Multi-Line Display** - Enhancement
8. **Graceful Shutdown** - Robustness
9. **Error Notification UI** - User feedback
10. **System Tray** - Optional polish

---

## Quick Reference: Key File Locations

| Feature           | File                                                                     |
| ----------------- | ------------------------------------------------------------------------ |
| Config loading    | [versualizer-core/src/config.rs](versualizer-core/src/config.rs)         |
| LRC parsing       | [versualizer-core/src/lrc.rs](versualizer-core/src/lrc.rs)               |
| Lyrics cache      | [versualizer-core/src/cache.rs](versualizer-core/src/cache.rs)           |
| Sync engine       | [versualizer-core/src/sync.rs](versualizer-core/src/sync.rs)             |
| Spotify OAuth     | [versualizer-spotify/src/oauth.rs](versualizer-spotify/src/oauth.rs)     |
| Spotify poller    | [versualizer-spotify/src/poller.rs](versualizer-spotify/src/poller.rs)   |
| UI app entry      | [versualizer-app-freya/src/main.rs](versualizer-app-freya/src/main.rs)   |
| Karaoke component | [versualizer-app-freya/src/app.rs](versualizer-app-freya/src/app.rs)     |
| UI state          | [versualizer-app-freya/src/state.rs](versualizer-app-freya/src/state.rs) |
