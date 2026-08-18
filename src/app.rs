use crate::{
    audio::AudioEngine,
    config::Config,
    db::{Db, PlaylistSummary},
    library::{save_metadata, scan_library},
    media_controls::{MediaCommand, MediaControlsEngine},
    models::Track,
    network::fetch_online_lyrics,
    queue::{PlaybackQueue, RepeatMode},
    soundsnatch::{
        DownloadProgress, DownloadRequest, MediaMetadata, SearchResult, SoundSnatch,
        SoundSnatchSettings, is_media_url,
    },
};
use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};
use ratatui::layout::Rect;
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum View {
    #[default]
    Home,
    Tracks,
    Artists,
    Albums,
    Genres,
    Playlists,
    Detail,
    Favorites,
    Recent,
    MostPlayed,
    Queue,
    Lyrics,
    Statistics,
    Equalizer,
    Devices,
    Scan,
    Downloads,
    Settings,
}

impl View {
    pub fn title(self) -> &'static str {
        match self {
            Self::Home => "Home",
            Self::Tracks => "Tracks",
            Self::Artists => "Artists",
            Self::Albums => "Albums",
            Self::Genres => "Genres",
            Self::Playlists => "Playlists",
            Self::Detail => "Collection",
            Self::Favorites => "Favorites",
            Self::Recent => "Recently Played",
            Self::MostPlayed => "Most Played",
            Self::Queue => "Queue",
            Self::Lyrics => "Lyrics",
            Self::Statistics => "Statistics",
            Self::Equalizer => "Equalizer",
            Self::Devices => "Output Devices",
            Self::Scan => "Library Scan",
            Self::Downloads => "SoundSnatch",
            Self::Settings => "Settings",
        }
    }
    pub fn is_track_list(self) -> bool {
        matches!(
            self,
            Self::Tracks | Self::Detail | Self::Favorites | Self::Recent | Self::MostPlayed
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Overlay {
    None,
    Search,
    Command {
        query: String,
        selected: usize,
    },
    Help,
    Actions {
        selected: usize,
    },
    ConfirmQuit,
    ConfirmDeletePlaylist(String),
    NewPlaylist(String),
    AddFolder(String),
    DownloadDestination(String),
    PlaylistPicker {
        track: Track,
        selected: usize,
    },
    Metadata {
        track: Track,
        field: usize,
        year: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadStage {
    Input,
    Busy,
    Results,
    Details,
    Options,
    Downloading,
    Done,
    Error,
}

#[derive(Clone, Debug)]
pub enum Hit {
    Route(View),
    Row(usize),
    PlayPause,
    Previous,
    Next,
    Queue,
    Lyrics,
}

#[derive(Clone, Debug)]
enum WorkerEvent {
    ScanProgress(usize, usize, usize, usize),
    ScanDone(Result<usize, String>),
    MetadataDone(Result<(), String>),
    LyricsDone(String, Result<Option<String>, String>),
    SearchDone(Result<Vec<SearchResult>, String>),
    MediaDone(Result<MediaMetadata, String>),
    DownloadProgress(DownloadProgress),
    DownloadDone(Result<PathBuf, String>),
}

pub struct App {
    pub db: Db,
    pub db_path: String,
    pub config: Config,
    pub audio: Option<AudioEngine>,
    pub audio_error: Option<String>,
    pub media_controls: Option<MediaControlsEngine>,
    media_rx: Option<Receiver<MediaCommand>>,
    pub view: View,
    pub history: Vec<View>,
    pub tracks: Vec<Track>,
    pub artists: Vec<String>,
    pub albums: Vec<String>,
    pub genres: Vec<String>,
    pub playlists: Vec<PlaylistSummary>,
    pub detail_title: String,
    pub detail_tracks: Vec<Track>,
    pub detail_origin: View,
    pub detail_playlist: Option<String>,
    pub queue: PlaybackQueue,
    pub selected: usize,
    pub scroll: usize,
    pub query: String,
    pub overlay: Overlay,
    pub status: String,
    pub status_error: bool,
    status_at: Instant,
    pub position: Duration,
    pub playing: bool,
    pub muted_volume: Option<f32>,
    pub devices: Vec<String>,
    pub active_device: Option<usize>,
    pub eq_band: usize,
    pub sleep_deadline: Option<Instant>,
    pub scan_running: bool,
    pub scan_progress: Option<(usize, usize, usize, usize)>,
    pub download_stage: DownloadStage,
    pub download_input: String,
    pub download_results: Vec<SearchResult>,
    pub download_selected: usize,
    pub download_meta: Option<MediaMetadata>,
    pub download_name: String,
    pub download_settings: SoundSnatchSettings,
    pub download_progress: Option<DownloadProgress>,
    pub download_message: String,
    pub hits: Vec<(Rect, Hit)>,
    worker_tx: Sender<WorkerEvent>,
    worker_rx: Receiver<WorkerEvent>,
    pub should_quit: bool,
}

impl App {
    pub fn load() -> anyhow::Result<Self> {
        let db_path = dirs::config_dir()
            .unwrap_or_default()
            .join("dopamine")
            .join("library.db");
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let db_path = db_path.to_string_lossy().into_owned();
        let mut db = Db::new(&db_path)?;
        db.init()?;
        let (audio, audio_error) = match AudioEngine::new() {
            Ok(audio) => (Some(audio), None),
            Err(error) => (None, Some(error.to_string())),
        };
        let (media_controls, media_rx) = match MediaControlsEngine::new() {
            Ok((engine, rx)) => (Some(engine), Some(rx)),
            Err(_) => (None, None),
        };
        let (worker_tx, worker_rx) = mpsc::channel();
        let download_settings = SoundSnatchSettings::load().unwrap_or_default();
        let mut app = Self {
            db,
            db_path,
            config: Config::load(),
            audio,
            audio_error,
            media_controls,
            media_rx,
            view: View::Home,
            history: vec![],
            tracks: vec![],
            artists: vec![],
            albums: vec![],
            genres: vec![],
            playlists: vec![],
            detail_title: String::new(),
            detail_tracks: vec![],
            detail_origin: View::Tracks,
            detail_playlist: None,
            queue: PlaybackQueue::default(),
            selected: 0,
            scroll: 0,
            query: String::new(),
            overlay: Overlay::None,
            status: String::new(),
            status_error: false,
            status_at: Instant::now(),
            position: Duration::ZERO,
            playing: false,
            muted_volume: None,
            devices: AudioEngine::list_devices(),
            active_device: None,
            eq_band: 0,
            sleep_deadline: None,
            scan_running: false,
            scan_progress: None,
            download_stage: DownloadStage::Input,
            download_input: String::new(),
            download_results: vec![],
            download_selected: 0,
            download_meta: None,
            download_name: String::new(),
            download_settings,
            download_progress: None,
            download_message: String::new(),
            hits: vec![],
            worker_tx,
            worker_rx,
            should_quit: false,
        };
        app.reload()?;
        app.restore_playback();
        Ok(app)
    }

    pub fn reload(&mut self) -> anyhow::Result<()> {
        self.tracks = self.db.get_all_tracks()?;
        self.artists = self.db.get_artists()?;
        self.albums = self.db.get_albums()?;
        self.genres = self.db.get_genres()?;
        self.playlists = self.db.get_playlist_summaries()?;
        Ok(())
    }

    fn restore_playback(&mut self) {
        if let Some(audio) = self.audio.as_mut() {
            if let Ok(Some(v)) = self.db.get_setting("volume")
                && let Ok(v) = v.parse()
            {
                audio.set_volume(v);
            }
            if let Ok(Some(v)) = self.db.get_setting("speed")
                && let Ok(v) = v.parse()
            {
                audio.set_speed(v);
            }
        }
        let items = self
            .db
            .get_setting("queue")
            .ok()
            .flatten()
            .and_then(|v| serde_json::from_str(&v).ok())
            .unwrap_or_default();
        let index = self
            .db
            .get_setting("queue_index")
            .ok()
            .flatten()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let shuffle = self.db.get_setting("shuffle").ok().flatten().as_deref() == Some("true");
        let repeat = match self.db.get_setting("repeat").ok().flatten().as_deref() {
            Some("one") => RepeatMode::One,
            Some("all") => RepeatMode::All,
            _ => RepeatMode::None,
        };
        self.queue.restore(items, index, shuffle, repeat);
    }

    fn persist_playback(&mut self) {
        let result = (|| -> anyhow::Result<()> {
            if let Some(audio) = self.audio.as_ref() {
                self.db.set_setting("volume", &audio.volume().to_string())?;
                self.db
                    .set_setting("speed", &audio.playback_speed().to_string())?;
            }
            self.db
                .set_setting("shuffle", &self.queue.shuffle.to_string())?;
            self.db.set_setting(
                "repeat",
                match self.queue.repeat_mode {
                    RepeatMode::None => "none",
                    RepeatMode::All => "all",
                    RepeatMode::One => "one",
                },
            )?;
            self.db
                .set_setting("queue", &serde_json::to_string(&self.queue.items)?)?;
            self.db
                .set_setting("queue_index", &self.queue.current_index.to_string())?;
            Ok(())
        })();
        if let Err(e) = result {
            self.notify_error(format!("Could not save playback state: {e}"));
        }
    }

    pub fn navigate(&mut self, view: View) {
        if self.view != view {
            self.history.push(self.view);
        }
        self.view = view;
        self.selected = 0;
        self.scroll = 0;
        self.query.clear();
        self.overlay = Overlay::None;
        if view == View::Devices {
            self.devices = AudioEngine::list_devices();
        }
        let _ = self.reload();
    }

    pub fn back(&mut self) {
        if let Some(view) = self.history.pop() {
            self.view = view;
            self.selected = 0;
            self.scroll = 0;
            self.query.clear();
        }
    }

    pub fn visible_tracks(&self) -> Vec<Track> {
        let source = match self.view {
            View::Tracks => self.tracks.clone(),
            View::Detail => self.detail_tracks.clone(),
            View::Favorites => self.db.get_favorites().unwrap_or_default(),
            View::Recent => self.db.get_recently_played().unwrap_or_default(),
            View::MostPlayed => self.db.get_most_played().unwrap_or_default(),
            _ => vec![],
        };
        fuzzy_tracks(source, &self.query)
    }

    pub fn visible_names(&self) -> Vec<String> {
        let source = match self.view {
            View::Artists => self.artists.clone(),
            View::Albums => self.albums.clone(),
            View::Genres => self.genres.clone(),
            View::Playlists => self.playlists.iter().map(|p| p.name.clone()).collect(),
            View::Devices => self.devices.clone(),
            _ => vec![],
        };
        fuzzy_names(source, &self.query)
    }

    pub fn current_track_selection(&self) -> Option<Track> {
        if self.view == View::Queue {
            self.queue.items.get(self.selected).cloned()
        } else {
            self.visible_tracks().get(self.selected).cloned()
        }
    }

    fn selection_len(&self) -> usize {
        match self.view {
            v if v.is_track_list() => self.visible_tracks().len(),
            View::Artists | View::Albums | View::Genres | View::Playlists | View::Devices => {
                self.visible_names().len()
            }
            View::Queue => self.queue.items.len(),
            View::Downloads if self.download_stage == DownloadStage::Results => {
                self.download_results.len()
            }
            _ => 0,
        }
    }

    fn move_selection(&mut self, delta: isize) {
        let len = self.selection_len();
        if len == 0 {
            self.selected = 0;
            return;
        }
        self.selected = (self.selected as isize + delta).clamp(0, len as isize - 1) as usize;
    }

    fn activate(&mut self) {
        if self.view.is_track_list() {
            self.play_selected();
            return;
        }
        match self.view {
            View::Queue => {
                if self.queue.select(self.selected).is_some() {
                    self.play_current();
                }
            }
            View::Artists | View::Albums | View::Genres | View::Playlists => self.open_collection(),
            View::Devices => self.switch_device(),
            View::Downloads => self.download_enter(),
            _ => {}
        }
    }

    fn open_collection(&mut self) {
        let Some(name) = self.visible_names().get(self.selected).cloned() else {
            return;
        };
        let tracks = match self.view {
            View::Artists => self.db.get_tracks_by_artist(&name),
            View::Albums => self.db.get_tracks_by_album(&name),
            View::Genres => self.db.get_tracks_by_genre(&name),
            View::Playlists => self.db.get_tracks_by_playlist(&name),
            _ => return,
        };
        match tracks {
            Ok(tracks) => {
                self.detail_origin = self.view;
                self.detail_playlist = (self.view == View::Playlists).then_some(name.clone());
                self.detail_title = name;
                self.detail_tracks = tracks;
                self.navigate(View::Detail);
            }
            Err(e) => self.notify_error(e.to_string()),
        }
    }

    fn play_selected(&mut self) {
        let tracks = self.visible_tracks();
        if tracks.is_empty() {
            return;
        }
        self.queue.replace(tracks, self.selected);
        self.play_current();
    }

    fn play_current(&mut self) {
        let Some(track) = self.queue.current().cloned() else {
            return;
        };
        match self.audio.as_mut().map(|a| a.play(&track.path)) {
            Some(Ok(())) => {
                self.playing = true;
                self.position = Duration::ZERO;
                let _ = self.db.record_play(&track.path);
                self.persist_playback();
                self.fetch_lyrics_if_missing(track);
            }
            Some(Err(e)) => self.notify_error(format!("Could not play {}: {e}", track.title)),
            None => self.notify_error("No audio output device is available"),
        }
    }

    fn toggle_playback(&mut self) {
        if self.queue.current().is_none() {
            if self.view.is_track_list() {
                self.play_selected();
            }
            return;
        }
        if let Some(audio) = self.audio.as_mut() {
            if audio.is_empty() {
                self.play_current();
            } else {
                audio.toggle();
                self.playing = !audio.is_paused();
            }
        }
    }
    fn next_track(&mut self) {
        if self.queue.advance().is_some() {
            self.play_current();
        }
    }
    fn previous_track(&mut self) {
        if self.position >= Duration::from_secs(3) {
            self.seek(-(self.position.as_secs() as i64));
        } else if self.queue.retreat().is_some() {
            self.play_current();
        }
    }
    fn seek(&mut self, delta: i64) {
        let duration = self.queue.current().map_or(0, |t| t.duration_secs.max(0));
        let target = (self.position.as_secs() as i64 + delta).clamp(0, duration) as u64;
        if let Some(a) = self.audio.as_mut()
            && a.seek(Duration::from_secs(target)).is_ok()
        {
            self.position = Duration::from_secs(target);
        }
    }
    fn volume(&mut self, delta: f32) {
        if let Some(a) = self.audio.as_mut() {
            a.set_volume(a.volume() + delta);
        }
        self.persist_playback();
    }
    fn toggle_mute(&mut self) {
        if let Some(a) = self.audio.as_mut() {
            if let Some(v) = self.muted_volume.take() {
                a.set_volume(v);
            } else {
                self.muted_volume = Some(a.volume());
                a.set_volume(0.0);
            }
        }
    }

    fn toggle_favorite(&mut self) {
        if let Some(track) = self
            .current_track_selection()
            .or_else(|| self.queue.current().cloned())
        {
            match self.db.toggle_favorite(&track.path) {
                Ok(()) => {
                    let _ = self.reload();
                    self.notify("Favorite updated");
                }
                Err(e) => self.notify_error(e.to_string()),
            }
        }
    }

    fn switch_device(&mut self) {
        let Some(name) = self.devices.get(self.selected).cloned() else {
            return;
        };
        match self.audio.as_mut().map(|a| a.set_device(&name)) {
            Some(Ok(())) => {
                self.active_device = Some(self.selected);
                self.notify(format!("Output: {name}"));
            }
            Some(Err(e)) => self.notify_error(e.to_string()),
            None => {}
        }
    }

    pub fn notify(&mut self, text: impl Into<String>) {
        self.status = text.into();
        self.status_error = false;
        self.status_at = Instant::now();
    }
    pub fn notify_error(&mut self, text: impl Into<String>) {
        self.status = text.into();
        self.status_error = true;
        self.status_at = Instant::now();
    }
    pub fn dismiss_transient_status(&mut self) {
        if self.status_at.elapsed() > Duration::from_secs(6) {
            self.status.clear();
        }
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press && key.kind != KeyEventKind::Repeat {
            return;
        }
        if self.handle_overlay_key(key) {
            return;
        }
        if self.handle_special_view_key(key) {
            return;
        }
        match (key.code, key.modifiers) {
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => self.overlay = Overlay::ConfirmQuit,
            (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                self.overlay = Overlay::Command {
                    query: String::new(),
                    selected: 0,
                }
            }
            (KeyCode::Char('?'), _) => self.overlay = Overlay::Help,
            (KeyCode::Char('/'), _) => self.overlay = Overlay::Search,
            (KeyCode::Esc, _) => self.back(),
            (KeyCode::Char('q'), _) => self.overlay = Overlay::ConfirmQuit,
            (KeyCode::Up, _) | (KeyCode::Char('k'), _) => self.move_selection(-1),
            (KeyCode::Down, _) | (KeyCode::Char('j'), _) => self.move_selection(1),
            (KeyCode::PageUp, _) => self.move_selection(-10),
            (KeyCode::PageDown, _) => self.move_selection(10),
            (KeyCode::Home, _) => self.selected = 0,
            (KeyCode::End, _) => self.selected = self.selection_len().saturating_sub(1),
            (KeyCode::Enter, _) => self.activate(),
            (KeyCode::Char(' '), _) => self.toggle_playback(),
            (KeyCode::Char('x'), _) => {
                if let Some(audio) = self.audio.as_mut() {
                    audio.stop();
                }
                self.playing = false;
                self.position = Duration::ZERO;
            }
            (KeyCode::Char('p'), _) => self.previous_track(),
            (KeyCode::Char('n'), _) => self.next_track(),
            (KeyCode::Char('<'), _) => self.seek(-10),
            (KeyCode::Char('>'), _) => self.seek(10),
            (KeyCode::Char('-'), _) => self.volume(-0.05),
            (KeyCode::Char('+') | KeyCode::Char('='), _) => self.volume(0.05),
            (KeyCode::Char('m'), _) => self.toggle_mute(),
            (KeyCode::Char('s'), _) => {
                self.queue.toggle_shuffle();
                self.persist_playback();
            }
            (KeyCode::Char('r'), _) => {
                self.queue.cycle_repeat();
                self.persist_playback();
            }
            (KeyCode::Char('f'), _) => self.toggle_favorite(),
            (KeyCode::Char('a'), _) => self.overlay = Overlay::Actions { selected: 0 },
            (KeyCode::Delete, _) if self.view == View::Queue => {
                self.queue.remove(self.selected);
                self.selected = self.selected.min(self.queue.items.len().saturating_sub(1));
                self.persist_playback();
            }
            (KeyCode::Char('J'), _) if self.view == View::Queue => {
                if let Some(i) = self.queue.move_down(self.selected) {
                    self.selected = i;
                    self.persist_playback();
                }
            }
            (KeyCode::Char('K'), _) if self.view == View::Queue => {
                if let Some(i) = self.queue.move_up(self.selected) {
                    self.selected = i;
                    self.persist_playback();
                }
            }
            (KeyCode::Char('['), _) if self.view == View::Lyrics => self.adjust_lyrics(-500),
            (KeyCode::Char(']'), _) if self.view == View::Lyrics => self.adjust_lyrics(500),
            (KeyCode::Char('S'), _) => self.start_scan(),
            _ => {}
        }
    }

    fn handle_special_view_key(&mut self, key: KeyEvent) -> bool {
        if self.view == View::Downloads {
            match self.download_stage {
                DownloadStage::Input => match key.code {
                    KeyCode::Esc => self.back(),
                    KeyCode::Enter => self.download_enter(),
                    KeyCode::Backspace => {
                        self.download_input.pop();
                    }
                    KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.download_input.push(c)
                    }
                    _ => return false,
                },
                DownloadStage::Results => match key.code {
                    KeyCode::Esc => self.download_stage = DownloadStage::Input,
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.download_selected = self.download_selected.saturating_sub(1)
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        self.download_selected = (self.download_selected + 1)
                            .min(self.download_results.len().saturating_sub(1))
                    }
                    KeyCode::Enter => self.download_enter(),
                    _ => return false,
                },
                DownloadStage::Details => match key.code {
                    KeyCode::Esc => self.download_stage = DownloadStage::Input,
                    KeyCode::Enter => self.download_enter(),
                    KeyCode::Backspace => {
                        self.download_name.pop();
                    }
                    KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.download_name.push(c)
                    }
                    _ => return false,
                },
                DownloadStage::Options => match key.code {
                    KeyCode::Esc => self.download_stage = DownloadStage::Details,
                    KeyCode::Char('1') => {
                        self.download_settings.default_format = crate::soundsnatch::AudioFormat::Mp3
                    }
                    KeyCode::Char('2') => {
                        self.download_settings.default_format =
                            crate::soundsnatch::AudioFormat::Flac
                    }
                    KeyCode::Char('3') => {
                        self.download_settings.default_format = crate::soundsnatch::AudioFormat::Wav
                    }
                    KeyCode::Char('d') => {
                        self.overlay = Overlay::DownloadDestination(
                            self.download_settings
                                .last_save_dir
                                .to_string_lossy()
                                .into_owned(),
                        )
                    }
                    KeyCode::Enter => self.download_enter(),
                    _ => return false,
                },
                DownloadStage::Downloading | DownloadStage::Busy => {
                    return key.code == KeyCode::Esc;
                }
                DownloadStage::Done | DownloadStage::Error => match key.code {
                    KeyCode::Enter | KeyCode::Esc => self.download_enter(),
                    _ => return false,
                },
            }
            return true;
        }
        if self.view == View::Equalizer {
            match key.code {
                KeyCode::Left => self.eq_band = self.eq_band.saturating_sub(1),
                KeyCode::Right => self.eq_band = (self.eq_band + 1).min(9),
                KeyCode::Up => {
                    if let Some(audio) = self.audio.as_mut() {
                        audio.eq_bands[self.eq_band] =
                            (audio.eq_bands[self.eq_band] + 1.0).min(10.0)
                    }
                }
                KeyCode::Down => {
                    if let Some(audio) = self.audio.as_mut() {
                        audio.eq_bands[self.eq_band] =
                            (audio.eq_bands[self.eq_band] - 1.0).max(-10.0)
                    }
                }
                KeyCode::Enter => {
                    if let Some(audio) = self.audio.as_mut() {
                        audio.eq_enabled = !audio.eq_enabled
                    }
                }
                _ => return false,
            }
            return true;
        }
        if self.view == View::Settings {
            match key.code {
                KeyCode::Char('a') => self.overlay = Overlay::AddFolder(String::new()),
                KeyCode::Char('t') => {
                    let themes = ["mocha", "dracula", "nord", "monokai"];
                    let index = themes
                        .iter()
                        .position(|name| *name == self.config.theme_name)
                        .unwrap_or(0);
                    self.config.theme_name = themes[(index + 1) % themes.len()].into();
                    let _ = self.config.save();
                }
                KeyCode::Char('v') => {
                    self.config.visualizer_enabled = !self.config.visualizer_enabled;
                    let _ = self.config.save();
                }
                KeyCode::Char('R') => {
                    self.config.reduce_motion = !self.config.reduce_motion;
                    let _ = self.config.save();
                }
                KeyCode::Char('y') => {
                    self.sleep_deadline = match self.sleep_deadline {
                        None => Some(Instant::now() + Duration::from_secs(15 * 60)),
                        Some(deadline)
                            if deadline.saturating_duration_since(Instant::now())
                                < Duration::from_secs(20 * 60) =>
                        {
                            Some(Instant::now() + Duration::from_secs(30 * 60))
                        }
                        Some(_) => None,
                    };
                }
                KeyCode::Char(',') => {
                    if let Some(audio) = self.audio.as_mut() {
                        audio.set_speed(audio.playback_speed() - 0.1)
                    }
                }
                KeyCode::Char('.') => {
                    if let Some(audio) = self.audio.as_mut() {
                        audio.set_speed(audio.playback_speed() + 0.1)
                    }
                }
                _ => return false,
            }
            return true;
        }
        false
    }

    fn handle_overlay_key(&mut self, key: KeyEvent) -> bool {
        let contextual_actions = self.context_actions();
        match &mut self.overlay {
            Overlay::None => return false,
            Overlay::Help => {
                self.overlay = Overlay::None;
            }
            Overlay::ConfirmQuit => match key.code {
                KeyCode::Char('y') | KeyCode::Enter => self.should_quit = true,
                KeyCode::Char('n') | KeyCode::Esc => self.overlay = Overlay::None,
                _ => {}
            },
            Overlay::ConfirmDeletePlaylist(name) => {
                let name = name.clone();
                match key.code {
                    KeyCode::Char('y') | KeyCode::Enter => {
                        if let Err(e) = self.db.delete_playlist(&name) {
                            self.notify_error(e.to_string());
                        } else {
                            let _ = self.reload();
                            self.notify("Playlist deleted");
                        }
                        self.overlay = Overlay::None;
                    }
                    KeyCode::Char('n') | KeyCode::Esc => self.overlay = Overlay::None,
                    _ => {}
                }
            }
            Overlay::Search => match key.code {
                KeyCode::Esc | KeyCode::Enter => self.overlay = Overlay::None,
                KeyCode::Backspace => {
                    self.query.pop();
                    self.selected = 0;
                }
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.query.push(c);
                    self.selected = 0;
                }
                _ => {}
            },
            Overlay::NewPlaylist(name) => match key.code {
                KeyCode::Esc => self.overlay = Overlay::None,
                KeyCode::Backspace => {
                    name.pop();
                }
                KeyCode::Enter => {
                    let value = name.trim().to_string();
                    if !value.is_empty() {
                        match self.db.create_playlist(&value) {
                            Ok(()) => {
                                let _ = self.reload();
                                self.notify("Playlist created");
                            }
                            Err(e) => self.notify_error(e.to_string()),
                        }
                    }
                    self.overlay = Overlay::None;
                }
                KeyCode::Char(c) => name.push(c),
                _ => {}
            },
            Overlay::AddFolder(path) => match key.code {
                KeyCode::Esc => self.overlay = Overlay::None,
                KeyCode::Backspace => {
                    path.pop();
                }
                KeyCode::Enter => {
                    let candidate = PathBuf::from(path.trim());
                    if candidate.is_dir() {
                        let value = candidate.to_string_lossy().into_owned();
                        if !self.config.music_dirs.contains(&value) {
                            self.config.music_dirs.push(value);
                        }
                        let _ = self.config.save();
                        self.overlay = Overlay::None;
                        self.notify("Music folder added; press S to scan");
                    } else {
                        self.notify_error("That path is not an accessible directory");
                    }
                }
                KeyCode::Char(c) => path.push(c),
                _ => {}
            },
            Overlay::DownloadDestination(path) => match key.code {
                KeyCode::Esc => self.overlay = Overlay::None,
                KeyCode::Backspace => {
                    path.pop();
                }
                KeyCode::Enter => {
                    let candidate = PathBuf::from(path.trim());
                    if candidate.is_dir() {
                        self.download_settings.last_save_dir = candidate;
                        let _ = self.download_settings.save();
                        self.overlay = Overlay::None;
                        self.notify("Download destination updated");
                    } else {
                        self.notify_error("That path is not an accessible directory");
                    }
                }
                KeyCode::Char(c) => path.push(c),
                _ => {}
            },
            Overlay::PlaylistPicker { track, selected } => match key.code {
                KeyCode::Esc => self.overlay = Overlay::None,
                KeyCode::Up => *selected = selected.saturating_sub(1),
                KeyCode::Down => {
                    *selected = (*selected + 1).min(self.playlists.len().saturating_sub(1))
                }
                KeyCode::Enter => {
                    if let Some(p) = self.playlists.get(*selected) {
                        if let Err(e) = self.db.add_track_to_playlist(&p.name, &track.path) {
                            self.notify_error(e.to_string());
                        } else {
                            self.notify(format!("Added to {}", p.name));
                        }
                    }
                    self.overlay = Overlay::None;
                }
                _ => {}
            },
            Overlay::Metadata { track, field, year } => match key.code {
                KeyCode::Esc => self.overlay = Overlay::None,
                KeyCode::Tab | KeyCode::Down => *field = (*field + 1) % 5,
                KeyCode::BackTab | KeyCode::Up => *field = (*field + 4) % 5,
                KeyCode::Backspace => metadata_field_mut(track, year, *field)
                    .pop()
                    .map(|_| ())
                    .unwrap_or(()),
                KeyCode::Enter => {
                    track.year = year.parse().unwrap_or(0);
                    let edited = track.clone();
                    let tx = self.worker_tx.clone();
                    thread::spawn(move || {
                        let result = save_metadata(&edited).map_err(|e| e.to_string());
                        let _ = tx.send(WorkerEvent::MetadataDone(result));
                    });
                    self.overlay = Overlay::None;
                    self.notify("Saving metadata…");
                }
                KeyCode::Char(c) => metadata_field_mut(track, year, *field).push(c),
                _ => {}
            },
            Overlay::Command { query, selected } => {
                let commands = filtered_commands(query);
                match key.code {
                    KeyCode::Esc => self.overlay = Overlay::None,
                    KeyCode::Backspace => {
                        query.pop();
                        *selected = 0;
                    }
                    KeyCode::Up => *selected = selected.saturating_sub(1),
                    KeyCode::Down => {
                        *selected = (*selected + 1).min(commands.len().saturating_sub(1))
                    }
                    KeyCode::Enter => {
                        if let Some((_, view)) = commands.get(*selected) {
                            let view = *view;
                            self.overlay = Overlay::None;
                            self.navigate(view);
                        }
                    }
                    KeyCode::Char(c) => {
                        query.push(c);
                        *selected = 0;
                    }
                    _ => {}
                }
            }
            Overlay::Actions { selected } => {
                let actions = contextual_actions;
                match key.code {
                    KeyCode::Esc => self.overlay = Overlay::None,
                    KeyCode::Up => *selected = selected.saturating_sub(1),
                    KeyCode::Down => {
                        *selected = (*selected + 1).min(actions.len().saturating_sub(1))
                    }
                    KeyCode::Enter => {
                        let choice = actions.get(*selected).copied();
                        self.overlay = Overlay::None;
                        if let Some(choice) = choice {
                            self.run_context_action(choice);
                        }
                    }
                    _ => {}
                }
            }
        }
        true
    }

    pub fn context_actions(&self) -> Vec<&'static str> {
        if self.view == View::Playlists {
            return vec!["Open", "Create playlist", "Delete playlist"];
        }
        if self.view == View::Queue {
            return vec![
                "Play",
                "Move up",
                "Move down",
                "Remove",
                "Clear queue",
                "Export M3U",
            ];
        }
        if self.view.is_track_list() {
            let mut actions = vec![
                "Play",
                "Favorite",
                "Add to queue",
                "Add to playlist",
                "Edit metadata",
                "Lyrics",
                "Fetch lyrics",
            ];
            if self.view == View::Detail && self.detail_playlist.is_some() {
                actions.push("Remove from playlist");
                actions.push("Export M3U");
            }
            return actions;
        }
        vec!["Open"]
    }
    fn run_context_action(&mut self, action: &str) {
        match action {
            "Open" | "Play" => self.activate(),
            "Favorite" => self.toggle_favorite(),
            "Add to queue" => {
                if let Some(t) = self.current_track_selection() {
                    self.queue.items.push(t);
                    self.persist_playback();
                    self.notify("Added to queue");
                }
            }
            "Add to playlist" => {
                if let Some(track) = self.current_track_selection() {
                    self.overlay = Overlay::PlaylistPicker { track, selected: 0 };
                }
            }
            "Edit metadata" => {
                if let Some(track) = self.current_track_selection() {
                    let year = if track.year == 0 {
                        String::new()
                    } else {
                        track.year.to_string()
                    };
                    self.overlay = Overlay::Metadata {
                        track,
                        field: 0,
                        year,
                    };
                }
            }
            "Lyrics" => self.navigate(View::Lyrics),
            "Fetch lyrics" => {
                if let Some(track) = self
                    .current_track_selection()
                    .or_else(|| self.queue.current().cloned())
                {
                    self.fetch_lyrics(track);
                    self.notify("Fetching lyrics…");
                }
            }
            "Remove from playlist" => {
                if let (Some(name), Some(track)) =
                    (self.detail_playlist.clone(), self.current_track_selection())
                {
                    match self.db.remove_track_from_playlist(&name, &track.path) {
                        Ok(()) => {
                            self.detail_tracks.retain(|item| item.path != track.path);
                            self.selected = self
                                .selected
                                .min(self.detail_tracks.len().saturating_sub(1));
                            self.notify("Removed from playlist");
                        }
                        Err(error) => self.notify_error(error.to_string()),
                    }
                }
            }
            "Export M3U" => {
                let label = self
                    .detail_playlist
                    .clone()
                    .unwrap_or_else(|| "queue".into());
                let tracks = if self.view == View::Queue {
                    self.queue.items.clone()
                } else {
                    self.detail_tracks.clone()
                };
                self.export_m3u(&label, &tracks);
            }
            "Create playlist" => self.overlay = Overlay::NewPlaylist(String::new()),
            "Delete playlist" => {
                if let Some(name) = self.visible_names().get(self.selected).cloned() {
                    self.overlay = Overlay::ConfirmDeletePlaylist(name);
                }
            }
            "Move up" => {
                if let Some(i) = self.queue.move_up(self.selected) {
                    self.selected = i;
                }
            }
            "Move down" => {
                if let Some(i) = self.queue.move_down(self.selected) {
                    self.selected = i;
                }
            }
            "Remove" => {
                self.queue.remove(self.selected);
            }
            "Clear queue" => {
                self.queue.replace(vec![], 0);
            }
            _ => {}
        }
    }

    fn export_m3u(&mut self, label: &str, tracks: &[Track]) {
        let safe = crate::soundsnatch::sanitize_filename(label);
        let path = self
            .download_settings
            .last_save_dir
            .join(format!("{safe}.m3u"));
        let mut content = String::from("#EXTM3U\n");
        for track in tracks {
            content.push_str(&format!(
                "#EXTINF:{},{} - {}\n{}\n",
                track.duration_secs, track.artist, track.title, track.path
            ));
        }
        match std::fs::write(&path, content) {
            Ok(()) => self.notify(format!("Exported {}", path.display())),
            Err(error) => self.notify_error(format!("M3U export failed: {error}")),
        }
    }

    pub fn on_mouse(&mut self, mouse: MouseEvent, _: Rect) {
        match mouse.kind {
            MouseEventKind::ScrollUp => self.move_selection(-3),
            MouseEventKind::ScrollDown => self.move_selection(3),
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some((_, hit)) = self
                    .hits
                    .iter()
                    .rev()
                    .find(|(r, _)| r.contains((mouse.column, mouse.row).into()))
                    .cloned()
                {
                    match hit {
                        Hit::Route(v) => self.navigate(v),
                        Hit::Row(i) => {
                            self.selected = i;
                        }
                        Hit::PlayPause => self.toggle_playback(),
                        Hit::Previous => self.previous_track(),
                        Hit::Next => self.next_track(),
                        Hit::Queue => self.navigate(View::Queue),
                        Hit::Lyrics => self.navigate(View::Lyrics),
                    }
                }
            }
            _ => {}
        }
    }

    pub fn tick(&mut self) {
        self.dismiss_transient_status();
        while let Ok(event) = self.worker_rx.try_recv() {
            self.handle_worker(event);
        }
        let media = self
            .media_rx
            .as_ref()
            .map(|rx| rx.try_iter().collect::<Vec<_>>())
            .unwrap_or_default();
        for cmd in media {
            self.handle_media(cmd);
        }
        let mut track_ended = false;
        if let Some(audio) = self.audio.as_mut() {
            audio.update_fades();
            self.position = audio.position();
            track_ended = self.playing && audio.is_empty();
            self.playing = !audio.is_paused() && !audio.is_empty();
        }
        if track_ended && self.queue.advance().is_some() {
            self.play_current();
        }
        if self.sleep_deadline.is_some_and(|d| Instant::now() >= d) {
            if let Some(a) = self.audio.as_mut() {
                a.pause();
            }
            self.sleep_deadline = None;
            self.notify("Sleep timer elapsed");
        }
        if let Some(media) = self.media_controls.as_mut() {
            let _ = media.update(self.queue.current(), !self.playing, self.position);
        }
    }

    fn handle_media(&mut self, cmd: MediaCommand) {
        match cmd {
            MediaCommand::Play => {
                if !self.playing {
                    self.toggle_playback()
                }
            }
            MediaCommand::Pause => {
                if self.playing {
                    self.toggle_playback()
                }
            }
            MediaCommand::Toggle => self.toggle_playback(),
            MediaCommand::Next => self.next_track(),
            MediaCommand::Previous => self.previous_track(),
            MediaCommand::Stop => {
                if let Some(a) = self.audio.as_mut() {
                    a.stop()
                }
                self.playing = false;
            }
            MediaCommand::SeekBy(v) => self.seek(v),
            MediaCommand::SetPosition(v) => {
                if let Some(a) = self.audio.as_mut() {
                    let _ = a.seek(v);
                }
            }
            MediaCommand::SetVolume(v) => {
                if let Some(a) = self.audio.as_mut() {
                    a.set_volume(v);
                }
            }
        }
    }

    fn handle_worker(&mut self, event: WorkerEvent) {
        match event {
            WorkerEvent::ScanProgress(a, b, c, d) => self.scan_progress = Some((a, b, c, d)),
            WorkerEvent::ScanDone(result) => {
                self.scan_running = false;
                match result {
                    Ok(n) => {
                        let _ = self.reload();
                        self.notify(format!("Library refreshed: {n} tracks indexed"));
                    }
                    Err(e) => self.notify_error(e),
                }
            }
            WorkerEvent::MetadataDone(result) => match result {
                Ok(()) => {
                    let _ = self.reload();
                    self.notify("Metadata saved");
                }
                Err(e) => self.notify_error(e),
            },
            WorkerEvent::LyricsDone(path, result) => match result {
                Ok(Some(lyrics)) => {
                    let _ = self.db.update_track_lyrics(&path, &lyrics);
                    for t in &mut self.queue.items {
                        if t.path == path {
                            t.lyrics = Some(lyrics.clone());
                        }
                    }
                    self.notify("Lyrics saved");
                }
                Ok(None) => self.notify("No online lyrics found"),
                Err(e) => self.notify_error(e),
            },
            WorkerEvent::SearchDone(result) => match result {
                Ok(items) => {
                    self.download_results = items;
                    self.download_selected = 0;
                    self.download_stage = DownloadStage::Results;
                    self.download_message.clear();
                }
                Err(e) => {
                    self.download_stage = DownloadStage::Error;
                    self.download_message = e;
                }
            },
            WorkerEvent::MediaDone(result) => match result {
                Ok(meta) => {
                    self.download_name = meta.title.clone();
                    self.download_meta = Some(meta);
                    self.download_stage = DownloadStage::Details;
                }
                Err(e) => {
                    self.download_stage = DownloadStage::Error;
                    self.download_message = e;
                }
            },
            WorkerEvent::DownloadProgress(p) => self.download_progress = Some(p),
            WorkerEvent::DownloadDone(result) => match result {
                Ok(path) => {
                    self.download_stage = DownloadStage::Done;
                    self.download_message = format!("Saved to {}", path.display());
                    let _ = self.reload();
                }
                Err(e) => {
                    self.download_stage = DownloadStage::Error;
                    self.download_message = e;
                }
            },
        }
    }

    pub fn start_scan(&mut self) {
        if self.scan_running {
            return;
        }
        let dirs = self.config.music_dirs.clone();
        if dirs.is_empty() {
            self.notify_error("No music directories configured");
            return;
        }
        self.scan_running = true;
        self.scan_progress = Some((0, dirs.len(), 0, 0));
        self.navigate(View::Scan);
        let tx = self.worker_tx.clone();
        let db_path = self.db_path.clone();
        thread::spawn(move || {
            let result = (|| -> Result<usize, String> {
                let mut db = Db::new(&db_path).map_err(|e| e.to_string())?;
                db.init().map_err(|e| e.to_string())?;
                let mut count = 0;
                for (di, dir) in dirs.iter().enumerate() {
                    let tracks = scan_library(dir, |c, t| {
                        let _ = tx.send(WorkerEvent::ScanProgress(di + 1, dirs.len(), c, t));
                    });
                    for track in tracks {
                        db.insert_track(&track).map_err(|e| e.to_string())?;
                        count += 1;
                    }
                }
                db.cleanup_stale_tracks().map_err(|e| e.to_string())?;
                Ok(count)
            })();
            let _ = tx.send(WorkerEvent::ScanDone(result));
        });
    }

    fn fetch_lyrics_if_missing(&mut self, track: Track) {
        if track.lyrics.is_some() {
            return;
        }
        self.fetch_lyrics(track);
    }

    fn fetch_lyrics(&mut self, track: Track) {
        let tx = self.worker_tx.clone();
        thread::spawn(move || {
            let path = track.path.clone();
            let result = tokio::runtime::Runtime::new()
                .map_err(|e| e.to_string())
                .map(|rt| rt.block_on(fetch_online_lyrics(&track)))
                .map_err(|e| e.to_string());
            let _ = tx.send(WorkerEvent::LyricsDone(path, result));
        });
    }
    fn adjust_lyrics(&mut self, delta: i64) {
        if let Some(t) = self.queue.current().cloned() {
            let offset = t.lyrics_offset_ms + delta;
            let _ = self.db.update_lyrics_offset(&t.path, offset);
            if let Some(q) = self.queue.items.iter_mut().find(|q| q.path == t.path) {
                q.lyrics_offset_ms = offset;
            }
            self.notify(format!("Lyrics offset {offset:+} ms"));
        }
    }

    fn download_enter(&mut self) {
        match self.download_stage {
            DownloadStage::Input => {
                let input = self.download_input.trim().to_string();
                if input.is_empty() {
                    self.download_message = "Enter a URL or search query".into();
                    return;
                }
                self.download_stage = DownloadStage::Busy;
                self.download_message = "Contacting yt-dlp…".into();
                let tx = self.worker_tx.clone();
                thread::spawn(move || {
                    let result = (|| -> Result<_, String> {
                        let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
                        let service = rt.block_on(SoundSnatch::new()).map_err(|e| e.to_string())?;
                        if is_media_url(&input) {
                            rt.block_on(service.fetch_metadata(&input))
                                .map_err(|e| e.to_string())
                                .map(Either::Meta)
                        } else {
                            rt.block_on(service.search(&input))
                                .map_err(|e| e.to_string())
                                .map(Either::Search)
                        }
                    })();
                    match result {
                        Ok(Either::Meta(v)) => {
                            let _ = tx.send(WorkerEvent::MediaDone(Ok(v)));
                        }
                        Ok(Either::Search(v)) => {
                            let _ = tx.send(WorkerEvent::SearchDone(Ok(v)));
                        }
                        Err(e) => {
                            let _ = tx.send(WorkerEvent::SearchDone(Err(e)));
                        }
                    }
                });
            }
            DownloadStage::Results => {
                if let Some(item) = self.download_results.get(self.download_selected).cloned() {
                    self.download_stage = DownloadStage::Busy;
                    let tx = self.worker_tx.clone();
                    thread::spawn(move || {
                        let result = (|| -> Result<_, String> {
                            let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
                            let s = rt.block_on(SoundSnatch::new()).map_err(|e| e.to_string())?;
                            rt.block_on(s.fetch_metadata(&item.url))
                                .map_err(|e| e.to_string())
                        })();
                        let _ = tx.send(WorkerEvent::MediaDone(result));
                    });
                }
            }
            DownloadStage::Details => self.download_stage = DownloadStage::Options,
            DownloadStage::Options => self.start_download(),
            DownloadStage::Done | DownloadStage::Error => {
                self.download_stage = DownloadStage::Input;
                self.download_message.clear();
                self.download_progress = None;
            }
            _ => {}
        }
    }
    fn start_download(&mut self) {
        let Some(meta) = self.download_meta.clone() else {
            return;
        };
        let url = meta
            .webpage_url
            .unwrap_or_else(|| self.download_input.clone());
        let request = DownloadRequest {
            url,
            save_dir: self.download_settings.last_save_dir.clone(),
            save_name: self.download_name.clone(),
            format: self.download_settings.default_format,
            archive_path: Some(self.download_settings.archive_path.clone()),
        };
        self.download_stage = DownloadStage::Downloading;
        let tx = self.worker_tx.clone();
        let db_path = self.db_path.clone();
        let import_dir = request.save_dir.clone();
        thread::spawn(move || {
            let result = (|| -> Result<PathBuf, String> {
                let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
                let s = rt.block_on(SoundSnatch::new()).map_err(|e| e.to_string())?;
                let progress_tx = tx.clone();
                let result = rt
                    .block_on(s.download(request, move |p| {
                        let _ = progress_tx.send(WorkerEvent::DownloadProgress(p));
                    }))
                    .map_err(|e| e.to_string())?;
                let mut db = Db::new(&db_path).map_err(|e| e.to_string())?;
                db.init().map_err(|e| e.to_string())?;
                for track in scan_library(&import_dir.to_string_lossy(), |_, _| {}) {
                    db.insert_track(&track).map_err(|e| e.to_string())?;
                }
                Ok(result.output_path)
            })();
            let _ = tx.send(WorkerEvent::DownloadDone(result));
        });
    }

    pub fn shutdown(&mut self) {
        self.persist_playback();
        if let Some(a) = self.audio.as_mut() {
            a.stop();
        }
    }
}

enum Either {
    Meta(MediaMetadata),
    Search(Vec<SearchResult>),
}

fn fuzzy_tracks(items: Vec<Track>, query: &str) -> Vec<Track> {
    if query.trim().is_empty() {
        return items;
    }
    let matcher = SkimMatcherV2::default();
    let q = query.to_lowercase();
    let mut scored = items
        .into_iter()
        .filter_map(|t| {
            matcher
                .fuzzy_match(
                    &format!("{} {} {} {}", t.title, t.artist, t.album, t.genre).to_lowercase(),
                    &q,
                )
                .map(|s| (s, t))
        })
        .collect::<Vec<_>>();
    scored.sort_by_key(|item| std::cmp::Reverse(item.0));
    scored.into_iter().map(|(_, t)| t).collect()
}
fn fuzzy_names(items: Vec<String>, query: &str) -> Vec<String> {
    if query.trim().is_empty() {
        return items;
    }
    let matcher = SkimMatcherV2::default();
    let mut scored = items
        .into_iter()
        .filter_map(|v| {
            matcher
                .fuzzy_match(&v.to_lowercase(), &query.to_lowercase())
                .map(|s| (s, v))
        })
        .collect::<Vec<_>>();
    scored.sort_by_key(|item| std::cmp::Reverse(item.0));
    scored.into_iter().map(|(_, v)| v).collect()
}
fn metadata_field_mut<'a>(
    track: &'a mut Track,
    year: &'a mut String,
    field: usize,
) -> &'a mut String {
    match field {
        0 => &mut track.title,
        1 => &mut track.artist,
        2 => &mut track.album,
        3 => &mut track.genre,
        _ => year,
    }
}
pub fn commands() -> Vec<(&'static str, View)> {
    vec![
        ("Home", View::Home),
        ("All Tracks", View::Tracks),
        ("Artists", View::Artists),
        ("Albums", View::Albums),
        ("Genres", View::Genres),
        ("Playlists", View::Playlists),
        ("Favorites", View::Favorites),
        ("Recently Played", View::Recent),
        ("Most Played", View::MostPlayed),
        ("Queue", View::Queue),
        ("Lyrics", View::Lyrics),
        ("Statistics", View::Statistics),
        ("Equalizer", View::Equalizer),
        ("Output Devices", View::Devices),
        ("Library Scan", View::Scan),
        ("SoundSnatch Downloads", View::Downloads),
        ("Settings", View::Settings),
    ]
}
pub fn filtered_commands(query: &str) -> Vec<(&'static str, View)> {
    if query.is_empty() {
        return commands();
    }
    commands()
        .into_iter()
        .filter(|(n, _)| n.to_lowercase().contains(&query.to_lowercase()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn track(title: &str) -> Track {
        Track {
            path: title.into(),
            title: title.into(),
            artist: "Artist".into(),
            album: "Album".into(),
            genre: "Genre".into(),
            year: 2024,
            favorite: false,
            play_count: 0,
            last_played: None,
            duration_secs: 60,
            lyrics: None,
            lyrics_offset_ms: 0,
        }
    }
    #[test]
    fn fuzzy_search_orders_matches() {
        let got = fuzzy_tracks(vec![track("Else"), track("Needle")], "needle");
        assert_eq!(got[0].title, "Needle");
    }
    #[test]
    fn all_routes_are_in_palette() {
        assert_eq!(commands().len(), 17);
    }
}
