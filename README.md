# Dopamine

**Pure audio adrenaline, served from the shell.**

A polished, offline TUI music player for Windows, Linux, and macOS. Rewritten in Rust for extreme performance and a jitter-free experience.

## Features
- **Fast & Lightweight**: Built with Rust and Ratatui for minimal CPU and memory footprint.
- **Improved Lyrics System**:
    - **Multi-stage Lookup**: Robust fetching with exact and fuzzy fallback strategies.
    - **Local Storage**: Automatically saves lyrics as `.lrc` files next to your music and in the database.
    - **Instant Access**: Fetched lyrics are synchronized across all views (Library, Search, Queue) for immediate display.
    - **Manual Sync**: Fine-tune lyrics timing with on-the-fly offset adjustments.
- **Smart Navigation**: Remembers your selection when navigating between views—automatically focuses the currently playing track when returning to the library.
- **Theme Presets**: Includes built-in support for popular themes: Mocha, Dracula, Nord, and Monokai.
- **Case-Insensitive Search**: Context-aware fuzzy search that ignores case across tracks, artists, albums, genres, and playlists.
- **Stable UI**: Redesigned player bar with smooth marquee scrolling, real-time progress tracking, and logical playback controls.
- **Background Scanning**: Multi-threaded library indexing with automatic deduplication and stale file cleanup.
- **MPRIS Integration**: Control playback from system-wide media controls and see "Now Playing" status in your OS.

## Tech Stack
- **Language**: Rust
- **TUI Framework**: [ratatui](https://github.com/ratatui/ratatui)
- **Audio Engine**: [rodio](https://github.com/RustAudio/rodio)
- **Database**: [rusqlite](https://github.com/rusqlite/rusqlite) (SQLite)
- **Metadata**: [lofty](https://github.com/pdeljanov/lofty)

## Installation
Requires [Rust](https://www.rust-lang.org/tools/install).

```bash
git clone https://github.com/maskedsyntax/dopamine.git
cd dopamine
cargo build --release
./target/release/dopamine
```

## Keybindings

### Navigation
- `1` - `0`: Switch views (Home, Artists, Albums, Playlists, Genres, Years, Queue, Lyrics, EQ, Devices)
- `j` / `k` or `Arrows`: Navigate lists
- `Enter`: Play selected track / Open folder or playlist
- `Backspace`: Go back to previous view (Smart Focus: returns to currently playing track)
- `/`: Search (Case-insensitive)
- `Ctrl-n`: Create new playlist
- `a`: Add highlighted track to playlist
- `Delete`: Delete highlighted playlist (in Playlist view)
- `t`: Cycle Theme presets (Mocha, Dracula, Nord, Monokai)
- `y`: Cycle Sleep Timer

### Playback & Audio
- `Space`: Pause / Resume
- `s`: Next track
- `p`: Previous track
- `h` / `l`: Seek backward / forward (10s)
- `+` / `-`: Volume up / down
- `S` (Shift+S): Scan library
- `z`: Toggle Shuffle
- `r`: Toggle Repeat mode (None, One, All)
- `{` / `}`: Adjust lyrics offset (-/+ 500ms)
- `q`: Quit (with confirmation)

## Configuration
Dopamine stores its configuration in `~/.config/dopamine/config.toml`. You can specify your music directories and set your default theme here.
