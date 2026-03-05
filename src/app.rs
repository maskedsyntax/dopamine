use crate::audio::AudioEngine;
use crate::db::Db;
use crate::models::Track;
use anyhow::Result;
use ratatui::widgets::{TableState, ListState};
use tui_input::Input;
use rustfft::{FftPlanner, num_complex::Complex};
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum View {
    Home,
    Artists,
    Albums,
    Playlists,
    PlaylistDetail,
}

#[derive(Clone, PartialEq, Eq)]
pub enum Confirmation {
    Quit,
    DeletePlaylist(String),
}

#[derive(Clone, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Search,
    CreatePlaylist,
    SelectPlaylist(Track),
    Confirm(Confirmation),
}

pub struct App {
    pub db: Db,
    pub audio: AudioEngine,
    pub view: View,
    pub tracks: Vec<Track>,
    pub artists: Vec<String>,
    pub albums: Vec<String>,
    pub playlists: Vec<String>,
    pub filtered_tracks: Vec<Track>,
    pub filtered_artists: Vec<String>,
    pub filtered_albums: Vec<String>,
    pub filtered_playlists: Vec<String>,
    pub selected_playlist: Option<String>,
    pub queue: Vec<Track>,
    pub queue_index: usize,
    pub table_state: TableState,
    pub list_state: ListState,
    pub playlist_select_state: ListState,
    pub search_input: Input,
    pub playlist_input: Input,
    pub input_mode: InputMode,
    pub current_track: Option<Track>,
    pub scanning: bool,
    pub marquee_offset: usize,
    pub visualizer_data: Vec<f32>,
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
            playlists: Vec::new(),
            filtered_tracks: Vec::new(),
            filtered_artists: Vec::new(),
            filtered_albums: Vec::new(),
            filtered_playlists: Vec::new(),
            selected_playlist: None,
            queue: Vec::new(),
            queue_index: 0,
            table_state: TableState::default(),
            list_state: ListState::default(),
            playlist_select_state: ListState::default(),
            search_input: Input::default(),
            playlist_input: Input::default(),
            input_mode: InputMode::Normal,
            current_track: None,
            scanning: false,
            marquee_offset: 0,
            visualizer_data: vec![0.0; 20],
        })
    }

    pub fn load_tracks(&mut self) -> Result<()> {
        self.tracks = self.db.get_all_tracks()?;
        self.artists = self.db.get_artists()?;
        self.albums = self.db.get_albums()?;
        self.playlists = self.db.get_playlists()?;
        self.apply_search();
        Ok(())
    }

    pub fn apply_search(&mut self) {
        let query = self.search_input.value().to_lowercase();
        
        match self.view {
            View::Home | View::PlaylistDetail => {
                let base_tracks = if self.view == View::PlaylistDetail {
                    if let Some(p) = &self.selected_playlist {
                        self.db.get_tracks_by_playlist(p).unwrap_or_default()
                    } else {
                        Vec::new()
                    }
                } else {
                    self.tracks.clone()
                };

                if query.is_empty() {
                    self.filtered_tracks = base_tracks;
                } else {
                    self.filtered_tracks = base_tracks
                        .into_iter()
                        .filter(|t| t.title.to_lowercase().contains(&query) || t.artist.to_lowercase().contains(&query))
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
            View::Playlists => {
                if query.is_empty() {
                    self.filtered_playlists = self.playlists.clone();
                } else {
                    self.filtered_playlists = self.playlists
                        .iter()
                        .filter(|p| p.to_lowercase().contains(&query))
                        .cloned()
                        .collect();
                }
            }
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
        if view != View::PlaylistDetail {
            self.selected_playlist = None;
        }
        self.table_state.select(Some(0));
        self.list_state.select(Some(0));
        self.apply_search();
    }

    pub fn next(&mut self) {
        if let InputMode::SelectPlaylist(_) = &self.input_mode {
            let len = self.playlists.len();
            let i = match self.playlist_select_state.selected() {
                Some(i) => if i >= len.saturating_sub(1) { 0 } else { i + 1 },
                None => 0,
            };
            self.playlist_select_state.select(Some(i));
            return;
        }
        match self.view {
            View::Home | View::PlaylistDetail => {
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
            View::Playlists => {
                let len = self.filtered_playlists.len();
                let i = match self.list_state.selected() {
                    Some(i) => if i >= len.saturating_sub(1) { 0 } else { i + 1 },
                    None => 0,
                };
                self.list_state.select(Some(i));
            }
        }
    }

    pub fn previous(&mut self) {
        if let InputMode::SelectPlaylist(_) = &self.input_mode {
            let len = self.playlists.len();
            let i = match self.playlist_select_state.selected() {
                Some(i) => if i == 0 { len.saturating_sub(1) } else { i - 1 },
                None => 0,
            };
            self.playlist_select_state.select(Some(i));
            return;
        }
        match self.view {
            View::Home | View::PlaylistDetail => {
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
            View::Playlists => {
                let len = self.filtered_playlists.len();
                let i = match self.list_state.selected() {
                    Some(i) => if i == 0 { len.saturating_sub(1) } else { i - 1 },
                    None => 0,
                };
                self.list_state.select(Some(i));
            }
        }
    }

    pub fn play_selected(&mut self) {
        match self.view {
            View::Home | View::PlaylistDetail => {
                if let Some(idx) = self.table_state.selected() {
                    if let Some(track) = self.filtered_tracks.get(idx) {
                        self.queue = self.filtered_tracks.clone();
                        self.queue_index = idx;
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
            View::Playlists => {
                if let Some(idx) = self.list_state.selected() {
                    if let Some(playlist) = self.filtered_playlists.get(idx).cloned() {
                        self.selected_playlist = Some(playlist);
                        self.view = View::PlaylistDetail;
                        self.table_state.select(Some(0));
                        self.apply_search();
                    }
                }
            }
        }
    }

    pub fn start_add_to_playlist(&mut self) {
        if let Some(idx) = self.table_state.selected() {
            if let Some(track) = self.filtered_tracks.get(idx).cloned() {
                self.input_mode = InputMode::SelectPlaylist(track);
                self.playlist_select_state.select(Some(0));
            }
        }
    }

    pub fn confirm_add_to_playlist(&mut self, track: Track) {
        if let Some(idx) = self.playlist_select_state.selected() {
            if let Some(playlist) = self.playlists.get(idx) {
                let _ = self.db.add_track_to_playlist(playlist, &track.path);
            }
        }
        self.input_mode = InputMode::Normal;
    }

    pub fn play_next(&mut self) {
        if self.queue.is_empty() { return; }
        self.queue_index = (self.queue_index + 1) % self.queue.len();
        if let Some(track) = self.queue.get(self.queue_index).cloned() {
            self.current_track = Some(track.clone());
            self.audio.play(&track.path);
        }
    }

    pub fn play_prev(&mut self) {
        if self.queue.is_empty() { return; }
        if self.queue_index == 0 {
            self.queue_index = self.queue.len() - 1;
        } else {
            self.queue_index -= 1;
        }
        if let Some(track) = self.queue.get(self.queue_index).cloned() {
            self.current_track = Some(track.clone());
            self.audio.play(&track.path);
        }
    }

    pub fn tick(&mut self) {
        if self.current_track.is_some() && !self.audio.is_paused() && self.audio.is_empty() {
            self.play_next();
        }
        self.marquee_offset = self.marquee_offset.wrapping_add(1);
        self.update_visualizer();
    }

    pub fn update_visualizer(&mut self) {
        if self.audio.is_paused() || self.audio.is_empty() {
            self.visualizer_data.iter_mut().for_each(|v| *v *= 0.8); // Faster fade out
            return;
        }

        let mut samples = Vec::new();
        if let Ok(buf) = self.audio.samples.lock() {
            samples = buf.iter().cloned().collect();
        }

        if samples.len() < 1024 {
            return;
        }

        let n = 1024;
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(n);

        let mut buffer: Vec<Complex<f32>> = samples.iter().take(n).map(|&s| Complex { re: s, im: 0.0 }).collect();
        fft.process(&mut buffer);

        let num_bars = self.visualizer_data.len();
        let chunk_size = (n / 2) / num_bars;
        
        for i in 0..num_bars {
            let sum: f32 = buffer[i * chunk_size..(i + 1) * chunk_size]
                .iter()
                .map(|c| (c.re * c.re + c.im * c.im).sqrt())
                .sum();
            let val = (sum / chunk_size as f32) * 4.0;
            // High smoothing: 20% new, 80% old for "chilled" look
            self.visualizer_data[i] = (val.clamp(0.0, 1.0) * 0.2) + (self.visualizer_data[i] * 0.8);
        }
    }

    pub fn toggle_playback(&mut self) {
        self.audio.toggle();
    }

    pub fn seek_forward(&mut self) {
        let pos = self.audio.position();
        self.audio.seek(pos + Duration::from_secs(10));
    }

    pub fn seek_backward(&mut self) {
        let pos = self.audio.position();
        self.audio.seek(pos.saturating_sub(Duration::from_secs(10)));
    }

    pub fn delete_playlist(&mut self, name: String) {
        let _ = self.db.delete_playlist(&name);
        let _ = self.load_tracks();
        self.input_mode = InputMode::Normal;
    }

    pub fn back(&mut self) {
        match self.view {
            View::PlaylistDetail => self.set_view(View::Playlists),
            View::Artists | View::Albums | View::Playlists => self.set_view(View::Home),
            View::Home => {}
        }
    }
}
