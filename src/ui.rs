use crate::app::{App, Confirmation, InputMode, View};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect, Alignment},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Row, Table, Paragraph, BorderType, List, ListItem, Clear},
    Frame,
};

const FG: Color = Color::Rgb(205, 214, 244);
const PRIMARY: Color = Color::Rgb(137, 180, 250); // Blue
const ACCENT: Color = Color::Rgb(203, 166, 247); // Mauve
const SECONDARY: Color = Color::Rgb(166, 227, 161); // Green
const INACTIVE: Color = Color::Rgb(88, 91, 112); // Surface2
const BG: Color = Color::Rgb(30, 30, 46);

pub fn draw(f: &mut Frame, app: &mut App) {
    let size = f.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(2), // Visualizer
            Constraint::Length(3),
        ])
        .split(size);

    draw_search(f, app, chunks[0]);
    
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(25),
            Constraint::Min(0),
        ])
        .split(chunks[1]);

    draw_sidebar(f, app, main_chunks[0]);
    
    match app.view {
        View::Home => draw_table(f, app, main_chunks[1]),
        View::Artists => draw_artists(f, app, main_chunks[1]),
        View::Albums => draw_albums(f, app, main_chunks[1]),
        View::Playlists => draw_playlists(f, app, main_chunks[1]),
        View::PlaylistDetail => draw_table(f, app, main_chunks[1]),
    }
    
    if let InputMode::SelectPlaylist(_) = &app.input_mode {
        draw_select_playlist(f, app);
    }

    if let InputMode::Confirm(conf) = &app.input_mode {
        draw_confirmation(f, conf);
    }
    
    draw_visualizer(f, app, chunks[2]);
    draw_player(f, app, chunks[3]);
}

fn draw_search(f: &mut Frame, app: &App, area: Rect) {
    let search_style = match &app.input_mode {
        InputMode::Normal => Style::default().fg(INACTIVE),
        _ => Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
    };

    let title = match &app.input_mode {
        InputMode::Search => " Search (Active) ",
        InputMode::CreatePlaylist => " Create Playlist ",
        InputMode::SelectPlaylist(_) => " Select Playlist ",
        InputMode::Confirm(_) => " Confirm Action ",
        InputMode::Normal => " Search ('/' to focus) ",
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(search_style)
        .title(title);

    let val = match &app.input_mode {
        InputMode::CreatePlaylist => app.playlist_input.value(),
        InputMode::SelectPlaylist(_) => "Use arrows to select playlist, Enter to confirm, Esc to cancel",
        InputMode::Confirm(_) => "Are you sure? (y/n)",
        _ => app.search_input.value(),
    };
    
    let text = if val.is_empty() && app.input_mode == InputMode::Normal {
        "..."
    } else {
        val
    };

    let p = Paragraph::new(text)
        .style(Style::default().fg(FG))
        .block(block);

    f.render_widget(p, area);
    
    match &app.input_mode {
        InputMode::Search => {
            f.set_cursor_position((
                area.x + 1 + app.search_input.visual_cursor() as u16,
                area.y + 1,
            ));
        }
        InputMode::CreatePlaylist => {
            f.set_cursor_position((
                area.x + 1 + app.playlist_input.visual_cursor() as u16,
                area.y + 1,
            ));
        }
        _ => {}
    }
}

fn draw_sidebar(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(INACTIVE))
        .title(" Dopamine ");

    let home_highlight = app.view == View::Home;
    let artists_highlight = app.view == View::Artists;
    let albums_highlight = app.view == View::Albums;
    let playlists_highlight = app.view == View::Playlists || app.view == View::PlaylistDetail;

    let mut sidebar_items = vec![
        Line::from(vec![Span::styled(if home_highlight { "❯ Home" } else { "  Home" }, if home_highlight { Style::default().fg(ACCENT).bold() } else { Style::default().fg(FG) })]),
        Line::from(vec![Span::styled(if artists_highlight { "❯ Artists" } else { "  Artists" }, if artists_highlight { Style::default().fg(ACCENT).bold() } else { Style::default().fg(FG) })]),
        Line::from(vec![Span::styled(if albums_highlight { "❯ Albums" } else { "  Albums" }, if albums_highlight { Style::default().fg(ACCENT).bold() } else { Style::default().fg(FG) })]),
        Line::from(vec![Span::styled(if playlists_highlight { "❯ Playlists" } else { "  Playlists" }, if playlists_highlight { Style::default().fg(ACCENT).bold() } else { Style::default().fg(FG) })]),
        Line::from(vec![Span::styled(" ", Style::default())]),
    ];

    if app.scanning {
        sidebar_items.push(Line::from(vec![Span::styled("  Scanning library...", Style::default().fg(SECONDARY).bold())]));
    } else {
        sidebar_items.push(Line::from(vec![Span::styled("  Press 's' to scan", Style::default().fg(INACTIVE))]));
    }
    
    sidebar_items.push(Line::from(vec![Span::styled(" ", Style::default())]));
    sidebar_items.push(Line::from(vec![Span::styled("  n/p: Next/Prev track", Style::default().fg(INACTIVE))]));
    sidebar_items.push(Line::from(vec![Span::styled("  h/l: Seek -/+ 10s", Style::default().fg(INACTIVE))]));
    sidebar_items.push(Line::from(vec![Span::styled("  +/-: Volume", Style::default().fg(INACTIVE))]));
    sidebar_items.push(Line::from(vec![Span::styled("  Ctrl-n: New Playlist", Style::default().fg(INACTIVE))]));
    sidebar_items.push(Line::from(vec![Span::styled("  Del: Delete Playlist", Style::default().fg(INACTIVE))]));
    sidebar_items.push(Line::from(vec![Span::styled("  q: Quit", Style::default().fg(INACTIVE))]));

    let p = Paragraph::new(sidebar_items).block(block);
    f.render_widget(p, area);
}

fn draw_table(f: &mut Frame, app: &mut App, area: Rect) {
    let title = match app.view {
        View::Home => " All Tracks ".to_string(),
        View::PlaylistDetail => format!(" Playlist: {} ", app.selected_playlist.as_deref().unwrap_or("Unknown")),
        _ => " Tracks ".to_string(),
    };

    let header_cells = ["Title", "Artist", "Album", "Time"]
        .iter()
        .map(|h| Cell::from(*h).style(Style::default().fg(PRIMARY).bold()));
    let header = Row::new(header_cells).height(1).bottom_margin(1);

    let rows = app.filtered_tracks.iter().map(|t| {
        let is_playing = app.current_track.as_ref().map_or(false, |ct| ct.path == t.path);
        let style = if is_playing {
            Style::default().fg(SECONDARY).bold()
        } else {
            Style::default().fg(FG)
        };

        let mins = t.duration_secs / 60;
        let secs = t.duration_secs % 60;
        let time_str = format!("{:02}:{:02}", mins, secs);

        Row::new(vec![
            Cell::from(t.title.clone()),
            Cell::from(t.artist.clone()),
            Cell::from(t.album.clone()),
            Cell::from(time_str),
        ]).style(style)
    });

    let t = Table::new(rows, [
        Constraint::Percentage(40),
        Constraint::Percentage(25),
        Constraint::Percentage(25),
        Constraint::Percentage(10),
    ])
    .header(header)
    .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(INACTIVE)).title(title.as_str()))
    .row_highlight_style(Style::default().bg(Color::Rgb(49, 50, 68)).add_modifier(Modifier::BOLD))
    .highlight_symbol("❯ ");

    f.render_stateful_widget(t, area, &mut app.table_state);
}

fn draw_artists(f: &mut Frame, app: &mut App, area: Rect) {
    let items: Vec<ListItem> = app.filtered_artists.iter().map(|a| {
        ListItem::new(a.as_str()).style(Style::default().fg(FG))
    }).collect();

    let l = List::new(items)
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(INACTIVE)).title(" Artists "))
        .highlight_style(Style::default().bg(Color::Rgb(49, 50, 68)).fg(ACCENT).add_modifier(Modifier::BOLD))
        .highlight_symbol("❯ ");

    f.render_stateful_widget(l, area, &mut app.list_state);
}

fn draw_albums(f: &mut Frame, app: &mut App, area: Rect) {
    let items: Vec<ListItem> = app.filtered_albums.iter().map(|a| {
        ListItem::new(a.as_str()).style(Style::default().fg(FG))
    }).collect();

    let l = List::new(items)
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(INACTIVE)).title(" Albums "))
        .highlight_style(Style::default().bg(Color::Rgb(49, 50, 68)).fg(ACCENT).add_modifier(Modifier::BOLD))
        .highlight_symbol("❯ ");

    f.render_stateful_widget(l, area, &mut app.list_state);
}

fn draw_playlists(f: &mut Frame, app: &mut App, area: Rect) {
    let items: Vec<ListItem> = app.filtered_playlists.iter().map(|p| {
        ListItem::new(p.as_str()).style(Style::default().fg(FG))
    }).collect();

    let l = List::new(items)
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(INACTIVE)).title(" Playlists "))
        .highlight_style(Style::default().bg(Color::Rgb(49, 50, 68)).fg(ACCENT).add_modifier(Modifier::BOLD))
        .highlight_symbol("❯ ");

    f.render_stateful_widget(l, area, &mut app.list_state);
}

fn draw_select_playlist(f: &mut Frame, app: &mut App) {
    let area = centered_rect(60, 40, f.area());
    f.render_widget(Clear, area);

    let items: Vec<ListItem> = app.playlists.iter().map(|p| {
        ListItem::new(p.as_str()).style(Style::default().fg(FG))
    }).collect();

    let l = List::new(items)
        .block(Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(ACCENT))
            .style(Style::default().bg(BG))
            .title(" Add to Playlist "))
        .highlight_style(Style::default().bg(Color::Rgb(49, 50, 68)).fg(ACCENT).add_modifier(Modifier::BOLD))
        .highlight_symbol("❯ ");

    f.render_stateful_widget(l, area, &mut app.playlist_select_state);
}

fn draw_confirmation(f: &mut Frame, conf: &Confirmation) {
    let area = centered_rect(50, 20, f.area());
    f.render_widget(Clear, area);

    let title = match conf {
        Confirmation::Quit => " Quit Dopamine? ",
        Confirmation::DeletePlaylist(_name) => " Delete Playlist? ",
    };

    let message = match conf {
        Confirmation::Quit => "Are you sure you want to quit? (y/n)".to_string(),
        Confirmation::DeletePlaylist(name) => format!("Delete '{}'? (y/n)", name),
    };

    let p = Paragraph::new(message)
        .alignment(Alignment::Center)
        .block(Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(ACCENT))
            .style(Style::default().bg(BG))
            .title(title));

    f.render_widget(p, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn draw_visualizer(f: &mut Frame, app: &App, area: Rect) {
    let num_bars = app.visualizer_data.len();
    let width = area.width as usize;
    if width == 0 { return; }

    let mut bars = String::new();
    let bar_chars = [" ", "▂", "▃", "▄", "▅", "▆", "▇", "█"];

    for i in 0..width {
        let data_idx = (i * num_bars) / width;
        let val = app.visualizer_data[data_idx];
        let char_idx = (val * (bar_chars.len() - 1) as f32).round() as usize;
        bars.push_str(bar_chars[char_idx.clamp(0, bar_chars.len() - 1)]);
    }

    let p = Paragraph::new(bars)
        .style(Style::default().fg(ACCENT))
        .alignment(Alignment::Center);
    f.render_widget(p, area);
}

fn draw_player(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(PRIMARY));

    f.render_widget(block, area);

    let inner = Rect::new(area.x + 1, area.y + 1, area.width.saturating_sub(2), 1);
    
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(4),  // Play/Pause icon
            Constraint::Min(10),    // Marquee text
            Constraint::Length(27), // Progress bar
            Constraint::Length(15), // Time
            Constraint::Length(12), // Volume
        ])
        .split(inner);

    // 1. Play/Pause icon
    let state = if app.audio.is_paused() { " ▶ " } else { " ⏸ " };
    f.render_widget(Paragraph::new(state).style(Style::default().fg(FG).bold()), chunks[0]);

    // 5. Volume
    let vol = format!(" Vol: {:>3}%", (app.audio.volume() * 100.0) as i32);
    f.render_widget(Paragraph::new(vol).style(Style::default().fg(FG).bold()).alignment(Alignment::Right), chunks[4]);

    if let Some(track) = &app.current_track {
        // 2. Marquee Text
        let display_text = format!(" {} - {}", track.title, track.artist);
        let max_text_len = chunks[1].width as usize;
        let final_text = if display_text.len() > max_text_len && max_text_len > 0 {
            let padded = format!("{}   ", display_text);
            let start = (app.marquee_offset / 4) % padded.len(); // Slower speed
            let mut result = String::new();
            for i in 0..max_text_len {
                result.push(padded.chars().nth((start + i) % padded.len()).unwrap_or(' '));
            }
            result
        } else {
            display_text
        };
        f.render_widget(Paragraph::new(final_text).style(Style::default().fg(FG).bold()), chunks[1]);

        // 3. Progress Bar
        let pos = app.audio.position().as_secs();
        let total = track.duration_secs.max(0) as u64;
        let progress = if total > 0 { (pos as f64 / total as f64).clamp(0.0, 1.0) } else { 0.0 };
        let bar_width = (chunks[2].width as usize).saturating_sub(2);
        let filled = (progress * bar_width as f64).round() as usize;
        let empty = bar_width.saturating_sub(filled);
        let bar = format!("[{}{}]", "█".repeat(filled), "░".repeat(empty));
        f.render_widget(Paragraph::new(bar).style(Style::default().fg(FG)), chunks[2]);

        // 4. Time
        let pos_mins = pos / 60;
        let pos_secs = pos % 60;
        let total_mins = total / 60;
        let total_secs = total % 60;
        let time_str = format!(" {:02}:{:02} / {:02}:{:02}", pos_mins, pos_secs, total_mins, total_secs);
        f.render_widget(Paragraph::new(time_str).style(Style::default().fg(FG).bold()), chunks[3]);
    } else {
        f.render_widget(Paragraph::new(" No track playing").style(Style::default().fg(INACTIVE)), chunks[1]);
    }
}
