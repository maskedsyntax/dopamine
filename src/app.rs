use crate::audio::AudioEngine;
use crate::db::Db;
use crate::library::scan_library;
use crate::models::Track;
use anyhow::Result;
use ratatui::widgets::{TableState, ListState};
use tui_input::Input;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum View {
    Home,
    Artists,
    Albums,
    Playlists,
}

pub struct App {
    pub db: Db,
    pub audio: AudioEngine,
    pub view: View,
    pub tracks: Vec<Track>,
    pub artists: Vec<String>,
    pub albums: Vec<String>,
    pub filtered_tracks: Vec<Track>,
    pub filtered_artists: Vec<String>,
    pub filtered_albums: Vec<String>,
    pub table_state: TableState,
    pub list_state: ListState,
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
            view: View::Home,
            tracks: Vec::new(),
            artists: Vec::new(),
            albums: Vec::new(),
            filtered_tracks: Vec::new(),
            filtered_artists: Vec::new(),
            filtered_albums: Vec::new(),
            table_state: TableState::default(),
            list_state: ListState::default(),
            search_input: Input::default(),
            input_mode: false,
            current_track: None,
            scanning: false,
        })
    }

    pub fn load_tracks(&mut self) -> Result<()> {
        self.tracks = self.db.get_all_tracks()?;
        self.artists = self.db.get_artists()?;
        self.albums = self.db.get_albums()?;
        self.apply_search();
        Ok(())
    }

    pub fn apply_search(&mut self) {
        let query = self.search_input.value().to_lowercase();
        
        match self.view {
            View::Home => {
                if query.is_empty() {
                    self.filtered_tracks = self.tracks.clone();
                } else {
                    self.filtered_tracks = self.tracks
                        .iter()
                        .filter(|t| t.title.to_lowercase().contains(&query) || t.artist.to_lowercase().contains(&query))
                        .cloned()
                        .collect();
                }
            }
            View::Artists => {
                if query.is_empty() {
                    self.filtered_artists = self.artists.clone();
                } else {
                    self.filtered_artists = self.artists
                        .iter()
                        .filter(|a| a.to_lowercase().contains(&query))
                        .cloned()
                        .collect();
                }
            }
            View::Albums => {
                if query.is_empty() {
                    self.filtered_albums = self.albums.clone();
                } else {
                    self.filtered_albums = self.albums
                        .iter()
                        .filter(|a| a.to_lowercase().contains(&query))
                        .cloned()
                        .collect();
                }
            }
            _ => {}
        }

        if self.table_state.selected().is_none() {
            self.table_state.select(Some(0));
        }
        if self.list_state.selected().is_none() {
            self.list_state.select(Some(0));
        }
    }

    pub fn set_view(&mut self, view: View) {
        self.view = view;
        self.search_input = Input::default(); // Clear search on view switch
        self.table_state.select(Some(0));
        self.list_state.select(Some(0));
        self.apply_search();
    }

    pub fn next(&mut self) {
        match self.view {
            View::Home => {
                let len = self.filtered_tracks.len();
                let i = match self.table_state.selected() {
                    Some(i) => if i >= len.saturating_sub(1) { 0 } else { i + 1 },
                    None => 0,
                };
                self.table_state.select(Some(i));
            }
            View::Artists => {
                let len = self.filtered_artists.len();
                let i = match self.list_state.selected() {
                    Some(i) => if i >= len.saturating_sub(1) { 0 } else { i + 1 },
                    None => 0,
                };
                self.list_state.select(Some(i));
            }
            View::Albums => {
                let len = self.filtered_albums.len();
                let i = match self.list_state.selected() {
                    Some(i) => if i >= len.saturating_sub(1) { 0 } else { i + 1 },
                    None => 0,
                };
                self.list_state.select(Some(i));
            }
            _ => {}
        }
    }

    pub fn previous(&mut self) {
        match self.view {
            View::Home => {
                let len = self.filtered_tracks.len();
                let i = match self.table_state.selected() {
                    Some(i) => if i == 0 { len.saturating_sub(1) } else { i - 1 },
                    None => 0,
                };
                self.table_state.select(Some(i));
            }
            View::Artists => {
                let len = self.filtered_artists.len();
                let i = match self.list_state.selected() {
                    Some(i) => if i == 0 { len.saturating_sub(1) } else { i - 1 },
                    None => 0,
                };
                self.list_state.select(Some(i));
            }
            View::Albums => {
                let len = self.filtered_albums.len();
                let i = match self.list_state.selected() {
                    Some(i) => if i == 0 { len.saturating_sub(1) } else { i - 1 },
                    None => 0,
                };
                self.list_state.select(Some(i));
            }
            _ => {}
        }
    }

    pub fn play_selected(&mut self) {
        match self.view {
            View::Home => {
                if let Some(idx) = self.table_state.selected() {
                    if let Some(track) = self.filtered_tracks.get(idx) {
                        self.current_track = Some(track.clone());
                        self.audio.play(&track.path);
                    }
                }
            }
            View::Artists => {
                if let Some(idx) = self.list_state.selected() {
                    if let Some(artist) = self.filtered_artists.get(idx).cloned() {
                        if let Ok(tracks) = self.db.get_tracks_by_artist(&artist) {
                            self.filtered_tracks = tracks;
                            self.view = View::Home;
                            self.table_state.select(Some(0));
                        }
                    }
                }
            }
            View::Albums => {
                if let Some(idx) = self.list_state.selected() {
                    if let Some(album) = self.filtered_albums.get(idx).cloned() {
                        if let Ok(tracks) = self.db.get_tracks_by_album(&album) {
                            self.filtered_tracks = tracks;
                            self.view = View::Home;
                            self.table_state.select(Some(0));
                        }
                    }
                }
            }
            _ => {}
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
