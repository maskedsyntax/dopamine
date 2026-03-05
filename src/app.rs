use crate::audio::AudioEngine;
use crate::db::Db;
use crate::library::scan_library;
use crate::models::Track;
use anyhow::Result;
use ratatui::widgets::TableState;
use tui_input::Input;

pub struct App {
    pub db: Db,
    pub audio: AudioEngine,
    pub tracks: Vec<Track>,
    pub filtered_tracks: Vec<Track>,
    pub table_state: TableState,
    pub search_input: Input,
    pub input_mode: bool,
    pub current_track: Option<Track>,
    pub scanning: bool,
}

impl App {
    pub fn new(db_path: &str) -> Result<Self> {
        let db = Db::new(db_path)?;
        db.init()?;
        let audio = AudioEngine::new()?;
        Ok(Self {
            db,
            audio,
            tracks: Vec::new(),
            filtered_tracks: Vec::new(),
            table_state: TableState::default(),
            search_input: Input::default(),
            input_mode: false,
            current_track: None,
            scanning: false,
        })
    }

    pub fn load_tracks(&mut self) -> Result<()> {
        self.tracks = self.db.get_all_tracks()?;
        self.apply_search();
        Ok(())
    }

    pub fn apply_search(&mut self) {
        let query = self.search_input.value().to_lowercase();
        if query.is_empty() {
            self.filtered_tracks = self.tracks.clone();
        } else {
            self.filtered_tracks = self.tracks
                .iter()
                .filter(|t| t.title.to_lowercase().contains(&query) || t.artist.to_lowercase().contains(&query))
                .cloned()
                .collect();
        }
        if !self.filtered_tracks.is_empty() && self.table_state.selected().is_none() {
            self.table_state.select(Some(0));
        }
    }

    pub fn next(&mut self) {
        let i = match self.table_state.selected() {
            Some(i) => {
                if i >= self.filtered_tracks.len().saturating_sub(1) {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.table_state.select(Some(i));
    }

    pub fn previous(&mut self) {
        let i = match self.table_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.filtered_tracks.len().saturating_sub(1)
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.table_state.select(Some(i));
    }

    pub fn play_selected(&mut self) {
        if let Some(idx) = self.table_state.selected() {
            if let Some(track) = self.filtered_tracks.get(idx) {
                self.current_track = Some(track.clone());
                self.audio.play(&track.path);
            }
        }
    }

    pub fn toggle_playback(&mut self) {
        self.audio.toggle();
    }

    pub fn scan_library(&mut self) {
        let music_dir = dirs::audio_dir().or_else(|| {
            dirs::home_dir().map(|h| h.join("Music"))
        });

        if let Some(dir) = music_dir {
            self.scanning = true;
            let dir_str = dir.to_str().unwrap_or_default();
            let tracks = scan_library(dir_str);
            for t in tracks {
                let _ = self.db.insert_track(&t);
            }
            let _ = self.db.cleanup_stale_tracks();
            let _ = self.load_tracks();
            self.scanning = false;
        }
    }
}
