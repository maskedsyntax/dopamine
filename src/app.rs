use crate::config::Config;

use crate::audio::AudioEngine;
use crate::db::Db;
use crate::models::Track;
use anyhow::Result;
use ratatui::widgets::{TableState, ListState};
use tui_input::Input;
use rustfft::{FftPlanner, num_complex::Complex, Fft};
use std::time::{Duration, Instant};
use std::sync::{Arc, atomic::Ordering};
use rand::seq::SliceRandom;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum View {
    Home,
    Artists,
    Albums,
    Genres,
    Years,
    Playlists,
    PlaylistDetail,
    Queue,
    Lyrics,
    Equalizer,
    Devices,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RepeatMode {
    None,
    One,
    All,
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
    EditMetadata(Track, usize), // track, field_index
    Help,
}

pub struct App {
    pub db: Db,
    pub audio: AudioEngine,
    pub config: Config,
    pub mpris: crate::mpris::MprisEngine,
    pub tx: Sender<Message>,
    pub view: View,
    pub tracks: Vec<Track>,
    pub artists: Vec<String>,
    pub albums: Vec<String>,
    pub genres: Vec<String>,
    pub years: Vec<i32>,
    pub playlists: Vec<String>,
    pub filtered_tracks: Vec<Track>,
    pub filtered_artists: Vec<String>,
    pub filtered_albums: Vec<String>,
    pub filtered_genres: Vec<String>,
    pub filtered_years: Vec<i32>,
    pub filtered_playlists: Vec<String>,
    pub selected_playlist: Option<String>,
    pub queue: Vec<Track>,
    pub queue_index: usize,
    pub shuffle: bool,
    pub repeat_mode: RepeatMode,
    pub shuffled_indices: Vec<usize>,
    pub shuffle_ptr: usize,
    pub table_state: TableState,
    pub list_state: ListState,
    pub playlist_select_state: ListState,
    pub search_input: Input,
    pub playlist_input: Input,
    pub edit_inputs: Vec<Input>,
    pub input_mode: InputMode,
    pub current_track: Option<Track>,
    pub scanning: bool,
    pub scan_progress: (usize, usize),
    pub marquee_offset: usize,
    pub notifications: Vec<(String, Instant)>,
    pub sleep_timer: Option<(Instant, Duration)>, // (start_time, total_duration)
    pub preloaded_path: Option<String>,
    pub crossfading: bool,
    pub audio_devices: Vec<String>,
    pub visualizer_data: Vec<f32>,
    pub fft_plan: Arc<dyn Fft<f32>>,
    pub fft_buffer: Vec<Complex<f32>>,
}

use std::sync::mpsc::Sender;
use crate::Message;

impl App {
    pub fn new(db_path: &str, tx: Sender<Message>) -> Result<Self> {
        let db = Db::new(db_path)?;
        db.init()?;
        let audio = AudioEngine::new()?;
        let config = Config::load();
        let mut planner = FftPlanner::new();
        let fft_plan = planner.plan_fft_forward(1024);
        
        let mpris = crate::mpris::MprisEngine::new(tx.clone());
        
        Ok(Self {
            db,
            audio,
            config,
            mpris,
            tx: tx.clone(),
            view: View::Home,
            tracks: Vec::new(),
            artists: Vec::new(),
            albums: Vec::new(),
            genres: Vec::new(),
            years: Vec::new(),
            playlists: Vec::new(),
            filtered_tracks: Vec::new(),
            filtered_artists: Vec::new(),
            filtered_albums: Vec::new(),
            filtered_genres: Vec::new(),
            filtered_years: Vec::new(),
            filtered_playlists: Vec::new(),
            selected_playlist: None,
            queue: Vec::new(),
            queue_index: 0,
            shuffle: false,
            repeat_mode: RepeatMode::None,
            shuffled_indices: Vec::new(),
            shuffle_ptr: 0,
            table_state: TableState::default(),
            list_state: ListState::default(),
            playlist_select_state: ListState::default(),
            search_input: Input::default(),
            playlist_input: Input::default(),
            edit_inputs: vec![Input::default(), Input::default(), Input::default(), Input::default(), Input::default()],
            input_mode: InputMode::Normal,
            current_track: None,
            scanning: false,
            scan_progress: (0, 0),
            marquee_offset: 0,
            notifications: Vec::new(),
            sleep_timer: None,
            preloaded_path: None,
            crossfading: false,
            audio_devices: AudioEngine::list_devices(),
            visualizer_data: vec![0.0; 20],
            fft_plan,
            fft_buffer: vec![Complex { re: 0.0, im: 0.0 }; 1024],
        })
    }

    pub fn load_tracks(&mut self) -> Result<()> {
        self.tracks = self.db.get_all_tracks()?;
        self.artists = self.db.get_artists()?;
        self.albums = self.db.get_albums()?;
        self.genres = self.db.get_genres()?;
        self.years = self.db.get_years()?;
        let mut playlists = self.db.get_playlists()?;
        playlists.insert(0, "⭐ Favorites".to_string());
        playlists.insert(1, "🕒 Recently Played".to_string());
        playlists.insert(2, "🔥 Most Played".to_string());
        self.playlists = playlists;
        self.apply_search();
        Ok(())
    }

    pub fn apply_search(&mut self) {
        let query = self.search_input.value();
        use fuzzy_matcher::FuzzyMatcher;
        use fuzzy_matcher::skim::SkimMatcherV2;
        let matcher = SkimMatcherV2::default();
        
        match self.view {
            View::Home | View::PlaylistDetail => {
                let base_tracks = if self.view == View::PlaylistDetail {
                    if let Some(p) = &self.selected_playlist {
                        match p.as_str() {
                            "⭐ Favorites" => self.db.get_favorites().unwrap_or_default(),
                            "🕒 Recently Played" => self.db.get_recently_played().unwrap_or_default(),
                            "🔥 Most Played" => self.db.get_most_played().unwrap_or_default(),
                            _ => self.db.get_tracks_by_playlist(p).unwrap_or_default(),
                        }
                    } else {
                        Vec::new()
                    }
                } else {
                    self.tracks.clone()
                };

                if query.is_empty() {
                    self.filtered_tracks = base_tracks;
                } else {
                    let mut scored: Vec<(i64, Track)> = base_tracks.into_iter()
                        .filter_map(|t| {
                            let text = format!("{} {} {}", t.title, t.artist, t.album);
                            matcher.fuzzy_match(&text, query).map(|score| (score, t))
                        })
                        .collect();
                    scored.sort_by(|a, b| b.0.cmp(&a.0));
                    self.filtered_tracks = scored.into_iter().map(|(_, t)| t).collect();
                }
            }
            View::Artists => {
                if query.is_empty() {
                    self.filtered_artists = self.artists.clone();
                } else {
                    let mut scored: Vec<(i64, String)> = self.artists.iter()
                        .filter_map(|a| matcher.fuzzy_match(a, query).map(|score| (score, a.clone())))
                        .collect();
                    scored.sort_by(|a, b| b.0.cmp(&a.0));
                    self.filtered_artists = scored.into_iter().map(|(_, a)| a).collect();
                }
            }
            View::Albums => {
                if query.is_empty() {
                    self.filtered_albums = self.albums.clone();
                } else {
                    let mut scored: Vec<(i64, String)> = self.albums.iter()
                        .filter_map(|a| matcher.fuzzy_match(a, query).map(|score| (score, a.clone())))
                        .collect();
                    scored.sort_by(|a, b| b.0.cmp(&a.0));
                    self.filtered_albums = scored.into_iter().map(|(_, a)| a).collect();
                }
            }
            View::Genres => {
                if query.is_empty() {
                    self.filtered_genres = self.genres.clone();
                } else {
                    let mut scored: Vec<(i64, String)> = self.genres.iter()
                        .filter_map(|g| matcher.fuzzy_match(g, query).map(|score| (score, g.clone())))
                        .collect();
                    scored.sort_by(|a, b| b.0.cmp(&a.0));
                    self.filtered_genres = scored.into_iter().map(|(_, g)| g).collect();
                }
            }
            View::Years => {
                if query.is_empty() {
                    self.filtered_years = self.years.clone();
                } else {
                    let mut scored: Vec<(i64, i32)> = self.years.iter()
                        .filter_map(|y| matcher.fuzzy_match(&y.to_string(), query).map(|score| (score, *y)))
                        .collect();
                    scored.sort_by(|a, b| b.0.cmp(&a.0));
                    self.filtered_years = scored.into_iter().map(|(_, y)| y).collect();
                }
            }
            View::Playlists => {
                if query.is_empty() {
                    self.filtered_playlists = self.playlists.clone();
                } else {
                    let mut scored: Vec<(i64, String)> = self.playlists.iter()
                        .filter_map(|p| matcher.fuzzy_match(p, query).map(|score| (score, p.clone())))
                        .collect();
                    scored.sort_by(|a, b| b.0.cmp(&a.0));
                    self.filtered_playlists = scored.into_iter().map(|(_, p)| p).collect();
                }
            }
            _ => {}
        }
// ... (rest of apply_search)

        if self.table_state.selected().is_none() {
            self.table_state.select(Some(0));
        }
        if self.list_state.selected().is_none() {
            self.list_state.select(Some(0));
        }
    }

    pub fn set_view(&mut self, view: View) {
        self.view = view;
        self.search_input = Input::default(); 
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
            View::Genres => {
                let len = self.filtered_genres.len();
                let i = match self.list_state.selected() {
                    Some(i) => if i >= len.saturating_sub(1) { 0 } else { i + 1 },
                    None => 0,
                };
                self.list_state.select(Some(i));
            }
            View::Years => {
                let len = self.filtered_years.len();
                let i = match self.list_state.selected() {
                    Some(i) => if i >= len.saturating_sub(1) { 0 } else { i + 1 },
                    None => 0,
                };
                self.list_state.select(Some(i));
            }
            View::Queue => {
                let len = self.queue.len();
                let i = match self.table_state.selected() {
                    Some(i) => if i >= len.saturating_sub(1) { 0 } else { i + 1 },
                    None => 0,
                };
                self.table_state.select(Some(i));
            }
            _ => {}
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
            View::Genres => {
                let len = self.filtered_genres.len();
                let i = match self.list_state.selected() {
                    Some(i) => if i == 0 { len.saturating_sub(1) } else { i - 1 },
                    None => 0,
                };
                self.list_state.select(Some(i));
            }
            View::Years => {
                let len = self.filtered_years.len();
                let i = match self.list_state.selected() {
                    Some(i) => if i == 0 { len.saturating_sub(1) } else { i - 1 },
                    None => 0,
                };
                self.list_state.select(Some(i));
            }
            View::Queue => {
                let len = self.queue.len();
                let i = match self.table_state.selected() {
                    Some(i) => if i == 0 { len.saturating_sub(1) } else { i - 1 },
                    None => 0,
                };
                self.table_state.select(Some(i));
            }
            View::Playlists => {
                let len = self.filtered_playlists.len();
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
            View::Home | View::PlaylistDetail => {
                if let Some(idx) = self.table_state.selected() {
                    if let Some(track) = self.filtered_tracks.get(idx) {
                        self.queue = self.filtered_tracks.clone();
                        self.queue_index = idx;
                        self.current_track = Some(track.clone());
                        self.audio.play(&track.path);
                        self.preloaded_path = None;
                        let _ = self.db.record_play(&track.path);
                        self.on_track_change(track.clone());
                        self.update_shuffled_indices();
                        if self.shuffle {
                            if let Some(pos) = self.shuffled_indices.iter().position(|&r| r == idx) {
                                self.shuffle_ptr = pos;
                            }
                        }
                        let _ = self.save_state();
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
            View::Genres => {
                if let Some(idx) = self.list_state.selected() {
                    if let Some(genre) = self.filtered_genres.get(idx).cloned() {
                        if let Ok(tracks) = self.db.get_tracks_by_genre(&genre) {
                            self.filtered_tracks = tracks;
                            self.view = View::Home;
                            self.table_state.select(Some(0));
                        }
                    }
                }
            }
            View::Years => {
                if let Some(idx) = self.list_state.selected() {
                    if let Some(year) = self.filtered_years.get(idx) {
                        if let Ok(tracks) = self.db.get_tracks_by_year(*year) {
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
            View::Devices => {
                if let Some(idx) = self.list_state.selected() {
                    if let Some(device_name) = self.audio_devices.get(idx).cloned() {
                        if let Err(e) = self.audio.set_device(&device_name) {
                            self.notify(format!("Failed to set device: {}", e));
                        } else {
                            self.notify(format!("Output: {}", device_name));
                        }
                    }
                }
            }
            View::Queue => {
                if let Some(idx) = self.table_state.selected() {
                    if let Some(track) = self.queue.get(idx).cloned() {
                        self.queue_index = idx;
                        self.current_track = Some(track.clone());
                        self.audio.play(&track.path);
                        let _ = self.db.record_play(&track.path);
                        if self.shuffle {
                            if let Some(pos) = self.shuffled_indices.iter().position(|&r| r == idx) {
                                self.shuffle_ptr = pos;
                            }
                        }
                        let _ = self.save_state();
                    }
                }
            }
            _ => {}
        }
    }

    pub fn move_queue_up(&mut self) {
        if self.view != View::Queue { return; }
        if let Some(idx) = self.table_state.selected() {
            if idx > 0 {
                self.queue.swap(idx, idx - 1);
                self.table_state.select(Some(idx - 1));
                if self.queue_index == idx { self.queue_index = idx - 1; }
                else if self.queue_index == idx - 1 { self.queue_index = idx; }
                self.update_shuffled_indices();
                let _ = self.save_state();
            }
        }
    }

    pub fn move_queue_down(&mut self) {
        if self.view != View::Queue { return; }
        if let Some(idx) = self.table_state.selected() {
            if idx < self.queue.len().saturating_sub(1) {
                self.queue.swap(idx, idx + 1);
                self.table_state.select(Some(idx + 1));
                if self.queue_index == idx { self.queue_index = idx + 1; }
                else if self.queue_index == idx + 1 { self.queue_index = idx; }
                self.update_shuffled_indices();
                let _ = self.save_state();
            }
        }
    }

    pub fn update_shuffled_indices(&mut self) {
        let mut indices: Vec<usize> = (0..self.queue.len()).collect();
        if self.shuffle {
            let mut rng = rand::rng();
            indices.shuffle(&mut rng);
        }
        self.shuffled_indices = indices;
    }

    pub fn toggle_shuffle(&mut self) {
        self.shuffle = !self.shuffle;
        if self.shuffle {
            self.update_shuffled_indices();
            if let Some(pos) = self.shuffled_indices.iter().position(|&idx| idx == self.queue_index) {
                self.shuffle_ptr = pos;
            }
        }
        let _ = self.save_state();
    }

    pub fn toggle_repeat(&mut self) {
        self.repeat_mode = match self.repeat_mode {
            RepeatMode::None => RepeatMode::All,
            RepeatMode::All => RepeatMode::One,
            RepeatMode::One => RepeatMode::None,
        };
        let _ = self.save_state();
    }

    pub fn increase_speed(&mut self) {
        let s = self.audio.playback_speed() + 0.1;
        self.audio.set_speed(s);
        let _ = self.save_state();
    }

    pub fn decrease_speed(&mut self) {
        let s = self.audio.playback_speed() - 0.1;
        self.audio.set_speed(s);
        let _ = self.save_state();
    }

    pub fn load_state(&mut self) -> Result<()> {
        if let Some(vol) = self.db.get_setting("volume")? {
            if let Ok(v) = vol.parse::<f32>() {
                self.audio.set_volume(v);
            }
        }
        if let Some(speed) = self.db.get_setting("speed")? {
            if let Ok(s) = speed.parse::<f32>() {
                self.audio.set_speed(s);
            }
        }
        if let Some(shuffle) = self.db.get_setting("shuffle")? {
            self.shuffle = shuffle == "true";
        }
        if let Some(repeat) = self.db.get_setting("repeat")? {
            self.repeat_mode = match repeat.as_str() {
                "one" => RepeatMode::One,
                "all" => RepeatMode::All,
                _ => RepeatMode::None,
            };
        }
        if let Some(queue_json) = self.db.get_setting("queue")? {
            if let Ok(queue) = serde_json::from_str::<Vec<Track>>(&queue_json) {
                self.queue = queue;
            }
        }
        if let Some(q_idx) = self.db.get_setting("queue_index")? {
            if let Ok(idx) = q_idx.parse::<usize>() {
                self.queue_index = idx;
            }
        }
        if let Some(current_path) = self.db.get_setting("current_track")? {
            if let Some(track) = self.queue.iter().find(|t| t.path == current_path).cloned() {
                self.current_track = Some(track);
            }
        }
        if self.shuffle {
            self.update_shuffled_indices();
            if let Some(pos) = self.shuffled_indices.iter().position(|&idx| idx == self.queue_index) {
                self.shuffle_ptr = pos;
            }
        }
        Ok(())
    }

    pub fn save_state(&self) -> Result<()> {
        self.db.set_setting("volume", &self.audio.volume().to_string())?;
        self.db.set_setting("speed", &self.audio.playback_speed().to_string())?;
        self.db.set_setting("shuffle", &self.shuffle.to_string())?;
        self.db.set_setting("repeat", match self.repeat_mode {
            RepeatMode::None => "none",
            RepeatMode::One => "one",
            RepeatMode::All => "all",
        })?;
        self.db.set_setting("queue", &serde_json::to_string(&self.queue)?)?;
        self.db.set_setting("queue_index", &self.queue_index.to_string())?;
        if let Some(track) = &self.current_track {
            self.db.set_setting("current_track", &track.path)?;
        }
        Ok(())
    }

    pub fn start_edit_metadata(&mut self) {
        if let Some(idx) = self.table_state.selected() {
            if let Some(track) = self.filtered_tracks.get(idx).cloned() {
                self.edit_inputs[0] = Input::new(track.title.clone());
                self.edit_inputs[1] = Input::new(track.artist.clone());
                self.edit_inputs[2] = Input::new(track.album.clone());
                self.edit_inputs[3] = Input::new(track.genre.clone());
                self.edit_inputs[4] = Input::new(track.year.to_string());
                self.input_mode = InputMode::EditMetadata(track, 0);
            }
        }
    }

    pub fn confirm_edit_metadata(&mut self, original_track: Track) {
        let new_title = self.edit_inputs[0].value().to_string();
        let new_artist = self.edit_inputs[1].value().to_string();
        let new_album = self.edit_inputs[2].value().to_string();
        let new_genre = self.edit_inputs[3].value().to_string();
        let new_year = self.edit_inputs[4].value().parse::<i32>().unwrap_or(0);

        let mut updated_track = original_track.clone();
        updated_track.title = new_title;
        updated_track.artist = new_artist;
        updated_track.album = new_album;
        updated_track.genre = new_genre;
        updated_track.year = new_year;

        // Save to file
        if let Ok(_) = crate::library::save_metadata(&updated_track) {
            // Update DB
            let _ = self.db.insert_track(&updated_track);
            let _ = self.load_tracks();
            self.notify(format!("Updated: {}", updated_track.title));
        } else {
            self.notify("Failed to save metadata".to_string());
        }
        self.input_mode = InputMode::Normal;
    }

    pub fn toggle_favorite(&mut self) {
        if let Some(idx) = self.table_state.selected() {
            if let Some(track) = self.filtered_tracks.get(idx) {
                let _ = self.db.toggle_favorite(&track.path);
                let _ = self.load_tracks();
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

        if self.shuffle {
            self.shuffle_ptr += 1;
            if self.shuffle_ptr >= self.shuffled_indices.len() {
                if self.repeat_mode == RepeatMode::All {
                    self.shuffle_ptr = 0;
                    let mut rng = rand::rng();
                    self.shuffled_indices.shuffle(&mut rng);
                } else {
                    return;
                }
            }
            self.queue_index = self.shuffled_indices[self.shuffle_ptr];
        } else {
            if self.queue_index >= self.queue.len() - 1 {
                if self.repeat_mode == RepeatMode::All {
                    self.queue_index = 0;
                } else {
                    return;
                }
            } else {
                self.queue_index += 1;
            }
        }

        if let Some(track) = self.queue.get(self.queue_index).cloned() {
            self.current_track = Some(track.clone());
            self.audio.play(&track.path);
            self.preloaded_path = None;
            let _ = self.db.record_play(&track.path);
            self.on_track_change(track.clone());
        }
        let _ = self.save_state();
    }

    pub fn play_prev(&mut self) {
        if self.queue.is_empty() { return; }

        if self.shuffle {
            if self.shuffle_ptr == 0 {
                if self.repeat_mode == RepeatMode::All {
                    self.shuffle_ptr = self.shuffled_indices.len() - 1;
                } else {
                    return;
                }
            } else {
                self.shuffle_ptr -= 1;
            }
            self.queue_index = self.shuffled_indices[self.shuffle_ptr];
        } else {
            if self.queue_index == 0 {
                if self.repeat_mode == RepeatMode::All {
                    self.queue_index = self.queue.len() - 1;
                } else {
                    return;
                }
            } else {
                self.queue_index -= 1;
            }
        }

        if let Some(track) = self.queue.get(self.queue_index).cloned() {
            self.current_track = Some(track.clone());
            self.audio.play(&track.path);
            self.preloaded_path = None;
            let _ = self.db.record_play(&track.path);
            self.on_track_change(track.clone());
        }
        let _ = self.save_state();
    }

    pub fn tick(&mut self) {
        if let Some(track) = &self.current_track {
            if !self.audio.is_paused() {
                if self.audio.is_empty() {
                    // Current track finished
                    if self.repeat_mode == RepeatMode::One {
                        if let Some(track) = self.current_track.clone() {
                            self.audio.play(&track.path);
                            let _ = self.db.record_play(&track.path);
                            return;
                        }
                    }

                    // Check if we have a preloaded track
                    if let Some(next_path) = self.preloaded_path.take() {
                        // Find the track in queue to update current_track info
                        if let Some(next_track) = self.queue.iter().find(|t| t.path == next_path).cloned() {
                            self.current_track = Some(next_track);
                            // We need to update queue_index too
                            if let Some(idx) = self.queue.iter().position(|t| t.path == next_path) {
                                self.queue_index = idx;
                            }
                        }
                        self.audio.swap_players(next_path);
                        let _ = self.save_state();
                    } else {
                        self.play_next();
                    }
                } else {
                    // Check if we should preload next track
                    let pos = self.audio.position().as_secs();
                    let total = track.duration_secs.max(0) as u64;
                    if total > 0 && self.repeat_mode != RepeatMode::One {
                        if total.saturating_sub(pos) <= 5 && self.preloaded_path.is_none() {
                            // Preload next track (same logic as before)
                            let next_idx = if self.shuffle {
                                let current_shuffle_pos = self.shuffled_indices.iter().position(|&i| i == self.queue_index).unwrap_or(0);
                                if current_shuffle_pos < self.shuffled_indices.len() - 1 {
                                    Some(self.shuffled_indices[current_shuffle_pos + 1])
                                } else if self.repeat_mode == RepeatMode::All {
                                    Some(self.shuffled_indices[0])
                                } else {
                                    None
                                }
                            } else {
                                if self.queue_index < self.queue.len() - 1 {
                                    Some(self.queue_index + 1)
                                } else if self.repeat_mode == RepeatMode::All {
                                    Some(0)
                                } else {
                                    None
                                }
                            };

                            if let Some(idx) = next_idx {
                                if let Some(next_track) = self.queue.get(idx) {
                                    let path = next_track.path.clone();
                                    self.audio.preload(&path);
                                    self.preloaded_path = Some(path);
                                }
                            }
                        }

                        // Trigger Crossfade 2 seconds before end
                        if total.saturating_sub(pos) <= 2 && self.preloaded_path.is_some() && !self.crossfading {
                            self.crossfading = true;
                            if let Some(next_path) = self.preloaded_path.take() {
                                if let Some(next_track) = self.queue.iter().find(|t| t.path == next_path).cloned() {
                                    self.current_track = Some(next_track.clone());
                                    if let Some(idx) = self.queue.iter().position(|t| t.path == next_path) {
                                        self.queue_index = idx;
                                    }
                                    self.on_track_change(next_track);
                                }
                                self.audio.swap_players(next_path);
                                let _ = self.save_state();
                            }
                        }
                    }
                }
            }
        }
        
        if self.crossfading {
            // We finished the transition or the new track is well underway
            // In a real implementation we'd ramp volume here, but swap_players handles the hard cut.
            // Let's just reset the flag for now.
            self.crossfading = false;
        }
        
        self.marquee_offset = self.marquee_offset.wrapping_add(1);
        self.update_visualizer();
        
        // Clean up notifications older than 3 seconds
        self.notifications.retain(|(_, time)| time.elapsed() < Duration::from_secs(3));

        // Handle sleep timer
        if let Some((start, duration)) = self.sleep_timer {
            if start.elapsed() >= duration {
                self.audio.toggle(); // This will pause if playing
                self.sleep_timer = None;
                self.notify("Sleep timer expired".to_string());
            }
        }

        // Sync MPRIS
        self.mpris.update(self.audio.is_paused(), &self.current_track, self.audio.position());
    }

    pub fn update_visualizer(&mut self) {
        if self.audio.is_paused() || self.audio.is_empty() {
            self.visualizer_data.iter_mut().for_each(|v| *v *= 0.8);
            return;
        }

        for (i, s) in self.audio.samples.iter().enumerate() {
            if i < self.fft_buffer.len() {
                self.fft_buffer[i] = Complex { 
                    re: s.load(Ordering::Relaxed) as f32 / 1000000.0, 
                    im: 0.0 
                };
            }
        }

        self.fft_plan.process(&mut self.fft_buffer);

        let num_bars = self.visualizer_data.len();
        let half = self.fft_buffer.len() / 2;
        let min_freq: f32 = 1.0;
        let max_freq = half as f32;
        let log_min = min_freq.ln();
        let log_max = max_freq.ln();

        for i in 0..num_bars {
            let lo = ((log_min + (log_max - log_min) * i as f32 / num_bars as f32).exp()) as usize;
            let hi = ((log_min + (log_max - log_min) * (i + 1) as f32 / num_bars as f32).exp()) as usize;
            let lo = lo.clamp(0, half - 1);
            let hi = hi.clamp(lo + 1, half);

            let sum: f32 = self.fft_buffer[lo..hi]
                .iter()
                .map(|c| (c.re * c.re + c.im * c.im).sqrt())
                .sum();
            let avg = sum / (hi - lo) as f32;

            let weight = 0.2 + 0.8 * (i as f32 / num_bars as f32);
            let db = (1.0 + avg * weight).ln() * 0.8;
            let val = db.clamp(0.0, 1.0);
            self.visualizer_data[i] = (val * 0.5) + (self.visualizer_data[i] * 0.5);
        }
    }

    pub fn toggle_playback(&mut self) {
        self.audio.toggle();
    }

    pub fn seek_forward(&mut self) {
        let pos = self.audio.position();
        let new_pos = pos + Duration::from_secs(10);
        if let Err(e) = self.audio.seek(new_pos) {
            self.notify(format!("Seek Error: {}", e));
        } else {
            self.notify(format!("Seek: {:02}:{:02} -> {:02}:{:02}", 
                pos.as_secs() / 60, pos.as_secs() % 60,
                new_pos.as_secs() / 60, new_pos.as_secs() % 60));
        }
    }

    pub fn seek_backward(&mut self) {
        let pos = self.audio.position();
        let new_pos = pos.saturating_sub(Duration::from_secs(10));
        if let Err(e) = self.audio.seek(new_pos) {
            self.notify(format!("Seek Error: {}", e));
        } else {
            self.notify(format!("Seek: {:02}:{:02} -> {:02}:{:02}", 
                pos.as_secs() / 60, pos.as_secs() % 60,
                new_pos.as_secs() / 60, new_pos.as_secs() % 60));
        }
    }

    pub fn delete_playlist(&mut self, name: String) {
        let _ = self.db.delete_playlist(&name);
        let _ = self.load_tracks();
        self.input_mode = InputMode::Normal;
    }

    pub fn export_playlist(&mut self) {
        if self.view == View::PlaylistDetail {
            if let Some(name) = &self.selected_playlist {
                let mut content = String::from("#EXTM3U\n");
                for track in &self.filtered_tracks {
                    content.push_str(&format!("#EXTINF:{},{}\n", track.duration_secs, track.title));
                    content.push_str(&format!("{}\n", track.path));
                }
                
                let export_path = dirs::home_dir()
                    .unwrap_or_default()
                    .join(format!("{}.m3u", name.replace(" ", "_")));
                
                let _ = std::fs::write(&export_path, content);
                self.notify(format!("Exported: {}", export_path.display()));
            }
        } else if self.view == View::Queue {
            let mut content = String::from("#EXTM3U\n");
            for track in &self.queue {
                content.push_str(&format!("#EXTINF:{},{}\n", track.duration_secs, track.title));
                content.push_str(&format!("{}\n", track.path));
            }
            let export_path = dirs::home_dir().unwrap_or_default().join("queue.m3u");
            let _ = std::fs::write(&export_path, content);
            self.notify(format!("Exported: {}", export_path.display()));
        }
    }

    pub fn cycle_sleep_timer(&mut self) {
        let current_dur = self.sleep_timer.map(|(_, d)| d.as_secs() / 60);
        let next = match current_dur {
            None => Some(15),
            Some(15) => Some(30),
            Some(30) => Some(60),
            _ => None,
        };

        if let Some(mins) = next {
            self.sleep_timer = Some((Instant::now(), Duration::from_secs(mins * 60)));
            self.notify(format!("Sleep timer set: {}m", mins));
        } else {
            self.sleep_timer = None;
            self.notify("Sleep timer disabled".to_string());
        }
    }

    pub fn notify(&mut self, message: String) {
        self.notifications.push((message, Instant::now()));
        if self.notifications.len() > 5 {
            self.notifications.remove(0);
        }
    }

    pub fn toggle_equalizer(&mut self) {
        self.audio.eq_enabled = !self.audio.eq_enabled;
        if let Some(track) = self.current_track.clone() {
            // Re-play current track to apply/remove filters
            let pos = self.audio.position();
            self.audio.play(&track.path);
            let _ = self.audio.seek(pos);
        }
        self.notify(format!("Equalizer: {}", if self.audio.eq_enabled { "ON" } else { "OFF" }));
    }

    pub fn adjust_eq(&mut self, band: usize, amount: f32) {
        if band < 10 {
            self.audio.eq_bands[band] = (self.audio.eq_bands[band] + amount).clamp(-10.0, 10.0);
            if self.audio.eq_enabled {
                if let Some(track) = self.current_track.clone() {
                    let pos = self.audio.position();
                    self.audio.play(&track.path);
                    let _ = self.audio.seek(pos);
                }
            }
        }
    }

    pub fn on_track_change(&mut self, track: Track) {
        let lastfm_config = self.config.lastfm.clone();
        let track_clone = track.clone();

        // Scrobble to Last.fm
        tokio::spawn(async move {
            let _ = crate::network::scrobble_to_lastfm(&lastfm_config, &track_clone).await;
        });

        // Fetch online lyrics if not present locally
        if track.lyrics.is_none() {
            let tx_lyrics = self.tx.clone();
            let track_lyrics = track.clone();
            tokio::spawn(async move {
                if let Some(content) = crate::network::fetch_online_lyrics(&track_lyrics).await {
                    let _ = tx_lyrics.send(Message::LyricsFetched(track_lyrics.path, content));
                }
            });
        }
    }

    pub fn back(&mut self) {
        match self.view {
            View::PlaylistDetail => self.set_view(View::Playlists),
            View::Artists | View::Albums | View::Playlists | View::Genres | View::Years | View::Queue | View::Lyrics | View::Equalizer | View::Devices => self.set_view(View::Home),
            View::Home => {}
        }
    }
}
