mod app;
mod audio;
mod db;
mod library;
mod models;
mod ui;
mod config;

use anyhow::Result;
use app::{App, Confirmation, InputMode};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::sync::mpsc;
use std::time::Duration;
use tui_input::backend::crossterm::EventHandler;

enum Message {
    ScanStarted,
    ScanProgress(usize, usize),
    ScanFinished,
}

fn main() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let db_path = dirs::config_dir().unwrap_or_default().join("dopamine").join("library.db");
    std::fs::create_dir_all(db_path.parent().unwrap())?;
    
    let mut app = App::new(db_path.to_str().unwrap())?;
    app.load_tracks()?;
    let _ = app.load_state();

    let (tx, rx) = mpsc::channel();

    let res = run_app(&mut terminal, &mut app, tx, rx);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err)
    }

    Ok(())
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    tx: mpsc::Sender<Message>,
    rx: mpsc::Receiver<Message>,
) -> io::Result<()> {
    loop {
        app.tick();
        terminal.draw(|f| ui::draw(f, app))?;

        while let Ok(msg) = rx.try_recv() {
            match msg {
                Message::ScanStarted => {
                    app.scanning = true;
                    app.scan_progress = (0, 0);
                }
                Message::ScanProgress(curr, total) => {
                    app.scan_progress = (curr, total);
                }
                Message::ScanFinished => {
                    app.scanning = false;
                    let _ = app.load_tracks();
                }
            }
        }

        if event::poll(Duration::from_millis(10))? {
            if let Event::Key(key) = event::read()? {
                match &app.input_mode {
                    InputMode::Search => {
                        match key.code {
                            KeyCode::Enter | KeyCode::Esc => {
                                app.input_mode = InputMode::Normal;
                                app.apply_search();
                            }
                            _ => {
                                app.search_input.handle_event(&Event::Key(key));
                                app.apply_search();
                            }
                        }
                    }
                    InputMode::CreatePlaylist => {
                        match key.code {
                            KeyCode::Enter => {
                                let name = app.playlist_input.value().to_string();
                                if !name.is_empty() {
                                    let _ = app.db.create_playlist(&name);
                                    let _ = app.load_tracks();
                                }
                                app.input_mode = InputMode::Normal;
                                app.playlist_input.reset();
                            }
                            KeyCode::Esc => {
                                app.input_mode = InputMode::Normal;
                                app.playlist_input.reset();
                            }
                            _ => {
                                app.playlist_input.handle_event(&Event::Key(key));
                            }
                        }
                    }
                    InputMode::SelectPlaylist(track) => {
                        let track_clone = track.clone();
                        match key.code {
                            KeyCode::Enter => {
                                app.confirm_add_to_playlist(track_clone);
                            }
                            KeyCode::Esc => {
                                app.input_mode = InputMode::Normal;
                            }
                            KeyCode::Up | KeyCode::Char('k') => app.previous(),
                            KeyCode::Down | KeyCode::Char('j') => app.next(),
                            _ => {}
                        }
                    }
                    InputMode::Confirm(conf) => {
                        let conf_clone = conf.clone();
                        match key.code {
                            KeyCode::Char('y') | KeyCode::Char('Y') => {
                                match conf_clone {
                                    Confirmation::Quit => return Ok(()),
                                    Confirmation::DeletePlaylist(name) => {
                                        app.delete_playlist(name);
                                    }
                                }
                            }
                            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                                app.input_mode = InputMode::Normal;
                            }
                            _ => {}
                        }
                    }
                    InputMode::EditMetadata(track, field_idx) => {
                        let track_clone = track.clone();
                        let idx = *field_idx;
                        match key.code {
                            KeyCode::Enter => {
                                app.confirm_edit_metadata(track_clone);
                            }
                            KeyCode::Esc => {
                                app.input_mode = InputMode::Normal;
                            }
                            KeyCode::Tab => {
                                app.input_mode = InputMode::EditMetadata(track_clone, (idx + 1) % 5);
                            }
                            KeyCode::BackTab => {
                                app.input_mode = InputMode::EditMetadata(track_clone, (idx + 4) % 5);
                            }
                            _ => {
                                app.edit_inputs[idx].handle_event(&Event::Key(key));
                            }
                        }
                    }
                    InputMode::Help => {
                        if let KeyCode::Char('?') | KeyCode::Esc = key.code {
                            app.input_mode = InputMode::Normal;
                        }
                    }
                    InputMode::Normal => {
                        match key.code {
                            KeyCode::Char('q') => {
                                app.input_mode = InputMode::Confirm(Confirmation::Quit);
                            }
                            KeyCode::Char('?') => app.input_mode = InputMode::Help,
                            KeyCode::Char('/') => app.input_mode = InputMode::Search,
                            KeyCode::Char('n') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
                                app.input_mode = InputMode::CreatePlaylist;
                            }
                            KeyCode::Char('e') => app.start_edit_metadata(),
                            KeyCode::Char('E') => app.export_playlist(),
                            KeyCode::Char('f') => app.toggle_favorite(),
                            KeyCode::Char('T') => app.cycle_sleep_timer(),
                            KeyCode::Char('a') => app.start_add_to_playlist(),
                            KeyCode::Backspace => app.back(),
                            KeyCode::Delete => {
                                if app.view == app::View::Playlists {
                                    if let Some(idx) = app.list_state.selected() {
                                        if let Some(name) = app.filtered_playlists.get(idx).cloned() {
                                            app.input_mode = InputMode::Confirm(Confirmation::DeletePlaylist(name));
                                        }
                                    }
                                }
                            }
                            KeyCode::Char('s') => {
                                if !app.scanning {
                                    app.scanning = true;
                                    let tx_clone = tx.clone();
                                    let db_path = dirs::config_dir().unwrap_or_default().join("dopamine").join("library.db");
                                    let db_path_str = db_path.to_str().unwrap().to_string();
                                    let music_dirs = app.config.music_dirs.clone();
                                    
                                    std::thread::spawn(move || {
                                        let _ = tx_clone.send(Message::ScanStarted);
                                        if let Ok(db) = db::Db::new(&db_path_str) {
                                            for dir in &music_dirs {
                                                let t_tx = tx_clone.clone();
                                                let tracks = library::scan_library(dir, |curr, total| {
                                                    let _ = t_tx.send(Message::ScanProgress(curr, total));
                                                });
                                                for t in tracks {
                                                    let _ = db.insert_track(&t);
                                                }
                                            }
                                            let _ = db.cleanup_stale_tracks();
                                        }
                                        let _ = tx_clone.send(Message::ScanFinished);
                                    });
                                }
                            }
                            KeyCode::Char('1') => app.set_view(app::View::Home),
                            KeyCode::Char('2') => app.set_view(app::View::Artists),
                            KeyCode::Char('3') => app.set_view(app::View::Albums),
                            KeyCode::Char('4') => app.set_view(app::View::Playlists),
                            KeyCode::Char('5') => app.set_view(app::View::Genres),
                            KeyCode::Char('6') => app.set_view(app::View::Years),
                            KeyCode::Char('7') => app.set_view(app::View::Queue),
                            KeyCode::Char('8') => app.set_view(app::View::Lyrics),
                            KeyCode::Char('9') => app.set_view(app::View::Equalizer),
                            KeyCode::Char('0') => app.set_view(app::View::Devices),
                            KeyCode::Char('n') => app.play_next(),
                            KeyCode::Char('p') => app.play_prev(),
                            KeyCode::Char('J') => app.move_queue_down(),
                            KeyCode::Char('K') => app.move_queue_up(),
                            KeyCode::Char('z') => app.toggle_shuffle(),
                            KeyCode::Char('r') => app.toggle_repeat(),
                            KeyCode::Char('[') => app.decrease_speed(),
                            KeyCode::Char(']') => app.increase_speed(),
                            KeyCode::Char('=') | KeyCode::Char('+') => {
                                let v = app.audio.volume();
                                app.audio.set_volume(v + 0.05);
                                let _ = app.save_state();
                            }
                            KeyCode::Char('-') | KeyCode::Char('_') => {
                                let v = app.audio.volume();
                                app.audio.set_volume(v - 0.05);
                                let _ = app.save_state();
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                if app.view == app::View::Equalizer {
                                    if let Some(idx) = app.list_state.selected() {
                                        app.adjust_eq(idx, 1.0);
                                    }
                                } else {
                                    app.previous();
                                }
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                if app.view == app::View::Equalizer {
                                    if let Some(idx) = app.list_state.selected() {
                                        app.adjust_eq(idx, -1.0);
                                    }
                                } else {
                                    app.next();
                                }
                            }
                            KeyCode::Left | KeyCode::Char('h') => {
                                if app.view == app::View::Equalizer {
                                    let i = match app.list_state.selected() {
                                        Some(i) => if i == 0 { 9 } else { i - 1 },
                                        None => 0,
                                    };
                                    app.list_state.select(Some(i));
                                } else {
                                    app.seek_backward();
                                }
                            }
                            KeyCode::Right | KeyCode::Char('l') => {
                                if app.view == app::View::Equalizer {
                                    let i = match app.list_state.selected() {
                                        Some(i) => if i >= 9 { 0 } else { i + 1 },
                                        None => 0,
                                    };
                                    app.list_state.select(Some(i));
                                } else {
                                    app.seek_forward();
                                }
                            }
                            KeyCode::Enter => {
                                if app.view == app::View::Equalizer {
                                    app.toggle_equalizer();
                                } else {
                                    app.play_selected();
                                }
                            }
                            KeyCode::Char(' ') => app.toggle_playback(),
                            _ => {}
                        }
                    }
                }
            }
        }
    }
}
