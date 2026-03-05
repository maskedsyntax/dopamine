# Dopamine

**Pure audio adrenaline, served from the shell.**

A polished, offline TUI music player for Windows, Linux, and macOS. Rewritten in Rust for extreme performance and a jitter-free experience.

## Features
- **Fast & Lightweight**: Built with Rust and Ratatui for minimal CPU and memory footprint.
- **Hierarchical Browsing**: Navigate your collection by Songs, Artists, or Albums.
- **Playlist Management**: Create, view, and manage local playlists with full persistence.
- **Smart Search**: Context-aware fuzzy search across tracks, artists, and albums.
- **Stable UI**: Redesigned player bar with smooth marquee scrolling, real-time progress tracking, and logical playback controls.
- **Background Scanning**: Multi-threaded library indexing with automatic deduplication and stale file cleanup.
- **Safe Operations**: Confirmation prompts for critical actions like quitting or deleting data.

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
```

## Keybindings

### Navigation
- `1` - `4`: Switch views (Home, Artists, Albums, Playlists)
- `j` / `k` or `Arrows`: Navigate lists
- `Enter`: Play selected track / Open folder or playlist
- `Backspace`: Go back to previous view
- `/`: Search
- `Ctrl-n`: Create new playlist
- `a`: Add highlighted track to playlist
- `Delete`: Delete highlighted playlist (in Playlist view)

### Playback
- `Space`: Pause / Resume
- `n`: Next track
- `p`: Previous track
- `+` / `-`: Volume up / down
- `s`: Scan library (~/Music)
- `q`: Quit (with confirmation)
