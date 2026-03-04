Agents.md
Project Overview
This document serves as a comprehensive guide for developing an offline Text User Interface (TUI) music player in Golang, utilizing the Bubble Tea framework. The player is inspired by spotify-tui but focuses exclusively on local, offline music files. The goal is to create the most feature-rich offline music player possible, replacing Spotify for a user who has downloaded all their songs. This player will support extensive library management, playback capabilities, and user interface enhancements to provide a seamless and professional music listening experience.
The architecture should follow the Model-View-Update (MVU) pattern inherent to Bubble Tea, ensuring a reactive and efficient TUI. Key components include:

Backend: Handle music file scanning, metadata parsing, playback, and data persistence using Golang standard libraries and third-party packages (e.g., go-flac, go-mp3 for audio decoding; id3 for metadata; beep or oto for audio playback).
Frontend: Bubble Tea for rendering views, handling keyboard inputs, and managing state transitions.
Data Storage: Use a local database like SQLite or BoltDB to store library indexes, playlists, and user preferences for quick access and persistence.
Dependencies: Minimize external dependencies, but include necessary ones such as charm.sh/bubbletea, github.com/faiface/beep for audio, and tag libraries for metadata.

This document is self-contained. By following it, you can implement the entire application without additional external references. Begin by setting up a Golang project with go mod init, install dependencies via go get, and structure the code into packages (e.g., cmd for entry point, pkg/player for core logic, pkg/ui for Bubble Tea components).
Feature List
The following is an exhaustive list of features to implement, categorized for clarity. Prioritize core playback and library management first, then add advanced features iteratively. Each feature includes implementation notes to guide development.
1. Library Management

Scan and Import Music Files: Automatically scan user-specified directories for audio files (support formats: MP3, FLAC, OGG, WAV, AAC, M4A). Recursively traverse folders and index files. Use goroutines for background scanning to avoid blocking the UI.
Metadata Parsing and Editing: Extract and display metadata (title, artist, album, genre, year, track number, duration) using libraries like github.com/bogem/id3v2 or github.com/dhowden/tag. Allow users to edit tags directly in the TUI and save changes to files.
Automatic Organization: Option to organize files into folders based on metadata (e.g., Artist/Album/Track.mp3). Provide a confirmation prompt before moving files.
Library Refresh: Manual or scheduled refresh to detect new/deleted files, with progress indicators in the TUI.
Duplicate Detection: Identify and remove duplicate tracks based on metadata or audio fingerprints.

2. Playback Controls

Basic Controls: Play, pause, stop, next track, previous track, seek forward/backward (e.g., by 10 seconds), and loop (single track, playlist, or shuffle).
Volume Adjustment: Increment/decrement volume with keyboard shortcuts; persist volume level across sessions.
Gapless Playback: Ensure seamless transitions between tracks by preloading the next audio buffer.
Crossfade: Optional fading between tracks for smooth playback; configurable duration (e.g., 0-10 seconds).
Playback Speed: Adjust speed (e.g., 0.5x to 2x) without altering pitch, using audio processing libraries.
Resume Playback: Remember and resume from the last played position on restart.

3. Queue and Playlist Management

Playback Queue: Add tracks/albums/playlists to a queue; reorder, remove, or clear items via TUI interactions.
Playlist Creation and Editing: Create, rename, delete playlists; add/remove tracks; support drag-and-drop simulation in TUI.
Smart Playlists: Automatically generated playlists based on criteria (e.g., most played, recently added, by genre, rating).
Playlist Import/Export: Support M3U/PLS formats for compatibility with other players.
Queue Persistence: Save the current queue across sessions.

4. Search and Browsing

Global Search: Search across library by title, artist, album, genre, or lyrics; support fuzzy matching with libraries like github.com/sahilm/fuzzy.
Browsing Views: Hierarchical views for artists, albums, genres, and folders; sortable by various metadata fields (e.g., alphabetical, release year).
Filtering and Sorting: Dynamic filters (e.g., by year range, genre) and sorting options; persist user preferences.
Recently Played/Added: Dedicated sections for quick access to recent activity.

5. User Interface Enhancements

Multi-Pane Layout: Bubble Tea views with panes for library browsing, current playback, queue, and search; navigable via keyboard (e.g., Tab to switch panes).
Keyboard Shortcuts: Comprehensive hotkeys (e.g., Space for play/pause, Arrow keys for navigation, Ctrl+S for search); display a help overlay (e.g., '?' key).
Themes and Customization: Support color themes (e.g., dark/light modes); customizable keybindings via a config file (YAML or TOML).
Progress Bar and Status: Real-time progress bar for current track; status bar showing playback info, volume, and notifications.
Album Art Display: Render ASCII art or simple block representations of album covers if embedded in files (using libraries like github.com/disintegration/imaging for processing).
Visualizer: ASCII-based audio visualizer (e.g., waveform or spectrum) during playback, using real-time audio analysis.

6. Audio Enhancements

Equalizer: Built-in equalizer with presets (e.g., rock, classical) or custom bands; apply effects using audio DSP libraries.
Audio Effects: Reverb, echo, or normalization; toggle via menu.
Multi-Device Output: Select audio output device if multiple are available (using system APIs via cgo if necessary).

7. Lyrics and Additional Media

Lyrics Display: Load and sync lyrics from embedded metadata or separate .lrc files; scroll in sync with playback.
Podcast/Audiobook Support: Treat non-music audio similarly, with chapter navigation and bookmarking.
Ratings and Favorites: Allow users to rate tracks (1-5 stars) and mark favorites; influence smart playlists.

8. Advanced Utilities

Sleep Timer: Set a timer to pause playback after a duration (e.g., 30 minutes).
Alarm Clock: Schedule playback to start at a specific time with selected tracks.
Statistics: Display listening stats (e.g., play counts, total library size, top artists).
Backup and Sync: Option to backup library database and playlists; simulate sync for multi-device use via file export.
Tag-Based Recommendations: Suggest similar tracks based on genre/artist similarity (offline algorithm using metadata).
Error Handling: Graceful handling of corrupted files, missing metadata, or playback errors with user notifications.

9. Configuration and Persistence

Config File: Store user settings (e.g., music directories, themes) in a dotfile (e.g., ~/.config/musicplayer/config.yaml).
Command-Line Flags: Support flags for quick actions (e.g., musicplayer --play playlist_name).
Logging: Optional debug logging to file for troubleshooting.

10. Accessibility and Performance

Accessibility Features: High-contrast modes, screen reader compatibility (if feasible in TUI), large text options.
Performance Optimizations: Lazy loading for large libraries; efficient indexing to handle thousands of tracks.
Cross-Platform Support: Ensure compatibility with Windows, macOS, and Linux; handle terminal resizing.

Implementation Guidelines
To build the application:

Setup: Create a main.go in cmd/musicplayer with Bubble Tea's Program initialization. Define a root model that composes sub-models for each view (e.g., LibraryModel, PlayerModel).
State Management: Use Bubble Tea's Update function for handling messages (e.g., KeyMsg for inputs, custom messages for playback events). Employ channels for audio playback to avoid blocking.
Audio Playback: Implement a player service in a separate goroutine; use beep for decoding and streaming audio.
Database Integration: Use github.com/mattn/go-sqlite3 to store indexed metadata; query efficiently for searches.
Testing: Write unit tests for core functions (e.g., metadata parsing) and integration tests for UI flows.
Iteration: Start with MVP (library scan + basic playback), then add features one by one, testing in terminal.
Deployment: Build with go build; consider packaging as a single binary.
