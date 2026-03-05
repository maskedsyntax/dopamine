mod app;
mod audio;
mod db;
mod library;
mod models;
mod ui;

use anyhow::Result;
use app::App;
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
        terminal.draw(|f| ui::draw(f, app))?;

        // Check for background messages
        while let Ok(msg) = rx.try_recv() {
            match msg {
                Message::ScanStarted => {
                    app.scanning = true;
                }
                Message::ScanFinished => {
                    app.scanning = false;
                    let _ = app.load_tracks();
                }
            }
        }

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if app.input_mode {
                    match key.code {
                        KeyCode::Enter | KeyCode::Esc => {
                            app.input_mode = false;
                            app.apply_search();
                        }
                        _ => {
                            app.search_input.handle_event(&Event::Key(key));
                            app.apply_search();
                        }
                    }
                } else {
                    match key.code {
                        KeyCode::Char('q') => return Ok(()),
                        KeyCode::Char('/') => app.input_mode = true,
                        KeyCode::Char('s') => {
                            if !app.scanning {
                                app.scanning = true;
                                let tx_clone = tx.clone();
                                let db_path = dirs::config_dir().unwrap_or_default().join("dopamine").join("library.db");
                                let db_path_str = db_path.to_str().unwrap().to_string();
                                
                                std::thread::spawn(move || {
                                    let _ = tx_clone.send(Message::ScanStarted);
                                    if let Ok(db) = db::Db::new(&db_path_str) {
                                        let _ = db.clear_db(); // Nuke the DB before scanning
                                        let music_dir = dirs::audio_dir().or_else(|| {
                                            dirs::home_dir().map(|h| h.join("Music"))
                                        });
                                        if let Some(dir) = music_dir {
                                            let tracks = library::scan_library(dir.to_str().unwrap());
                                            for t in tracks {
                                                let _ = db.insert_track(&t);
                                            }
                                        }
                                    }
                                    let _ = tx_clone.send(Message::ScanFinished);
                                });
                            }
                        }
                        KeyCode::Up | KeyCode::Char('k') => app.previous(),
                        KeyCode::Down | KeyCode::Char('j') => app.next(),
                        KeyCode::Enter => app.play_selected(),
                        KeyCode::Char(' ') => app.toggle_playback(),
                        _ => {}
                    }
                }
            }
        }
    }
}
