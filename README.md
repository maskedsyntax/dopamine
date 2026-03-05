# Dopamine

A polished, offline TUI music player for Windows, Linux, and macOS.

## Features
- **Fast & Lightweight**: Rewritten in Rust for maximum performance and low resource usage.
- **Modern UI**: Clean, responsive layout built with `ratatui` and `crossterm`.
- **Advanced Library Management**: Automatic deduplication and background scanning.
- **Search Everywhere**: Instant fuzzy search across your entire collection.

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
- `/` : Search
- `s` : Scan library (~/Music)
- `j` / `k` (or arrows) : Navigate
- `Enter` : Play
- `Space` : Pause/Resume
- `q` : Quit
