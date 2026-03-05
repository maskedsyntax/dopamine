use crate::app::{App, InputMode, View};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect, Alignment},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Row, Table, Paragraph, BorderType, List, ListItem},
    Frame,
};

const FG: Color = Color::Rgb(205, 214, 244);
const PRIMARY: Color = Color::Rgb(137, 180, 250); // Blue
const ACCENT: Color = Color::Rgb(203, 166, 247); // Mauve
const SECONDARY: Color = Color::Rgb(166, 227, 161); // Green
const INACTIVE: Color = Color::Rgb(88, 91, 112); // Surface2

pub fn draw(f: &mut Frame, app: &mut App) {
    let size = f.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
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
    }
    
    draw_player(f, app, chunks[2]);
}

fn draw_search(f: &mut Frame, app: &App, area: Rect) {
    let search_style = if app.input_mode != InputMode::Normal {
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(INACTIVE)
    };

    let title = match app.input_mode {
        InputMode::Search => " Search (Active) ",
        InputMode::CreatePlaylist => " Create Playlist ",
        InputMode::Normal => " Search ('/' to focus) ",
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(search_style)
        .title(title);

    let val = match app.input_mode {
        InputMode::CreatePlaylist => app.playlist_input.value(),
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
    
    match app.input_mode {
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
        InputMode::Normal => {}
    }
}

fn draw_sidebar(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(INACTIVE))
        .title(" Dopamine ");

    let mut sidebar_items = vec![
        Line::from(vec![Span::styled(if app.view == View::Home { "❯ Home" } else { "  Home" }, if app.view == View::Home { Style::default().fg(ACCENT).bold() } else { Style::default().fg(FG) })]),
        Line::from(vec![Span::styled(if app.view == View::Artists { "❯ Artists" } else { "  Artists" }, if app.view == View::Artists { Style::default().fg(ACCENT).bold() } else { Style::default().fg(FG) })]),
        Line::from(vec![Span::styled(if app.view == View::Albums { "❯ Albums" } else { "  Albums" }, if app.view == View::Albums { Style::default().fg(ACCENT).bold() } else { Style::default().fg(FG) })]),
        Line::from(vec![Span::styled(if app.view == View::Playlists { "❯ Playlists" } else { "  Playlists" }, if app.view == View::Playlists { Style::default().fg(ACCENT).bold() } else { Style::default().fg(FG) })]),
        Line::from(vec![Span::styled(" ", Style::default())]),
    ];

    if app.scanning {
        sidebar_items.push(Line::from(vec![Span::styled("  Scanning library...", Style::default().fg(SECONDARY).bold())]));
    } else {
        sidebar_items.push(Line::from(vec![Span::styled("  Press 's' to scan", Style::default().fg(INACTIVE))]));
    }
    
    sidebar_items.push(Line::from(vec![Span::styled(" ", Style::default())]));
    sidebar_items.push(Line::from(vec![Span::styled("  n/p: Next/Prev track", Style::default().fg(INACTIVE))]));
    sidebar_items.push(Line::from(vec![Span::styled("  +/-: Volume", Style::default().fg(INACTIVE))]));
    sidebar_items.push(Line::from(vec![Span::styled("  q: Quit", Style::default().fg(INACTIVE))]));

    let p = Paragraph::new(sidebar_items).block(block);
    f.render_widget(p, area);
}

fn draw_table(f: &mut Frame, app: &mut App, area: Rect) {
    let title = match app.view {
        View::Home => " All Tracks ",
        _ => " Tracks ",
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
    .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(INACTIVE)).title(title))
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
    let mut items: Vec<ListItem> = app.filtered_playlists.iter().map(|p| {
        ListItem::new(p.as_str()).style(Style::default().fg(FG))
    }).collect();

    if items.is_empty() {
        items.push(ListItem::new("No playlists. Press '+' to create one.").style(Style::default().fg(INACTIVE)));
    }

    let l = List::new(items)
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(INACTIVE)).title(" Playlists "))
        .highlight_style(Style::default().bg(Color::Rgb(49, 50, 68)).fg(ACCENT).add_modifier(Modifier::BOLD))
        .highlight_symbol("❯ ");

    f.render_stateful_widget(l, area, &mut app.list_state);
}

fn draw_player(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(PRIMARY));

    let content = if let Some(track) = &app.current_track {
        let state = if app.audio.is_paused() { "⏸" } else { "▶" };
        
        let pos = app.audio.position().as_secs();
        let total = track.duration_secs.max(0) as u64;
        
        let pos_mins = pos / 60;
        let pos_secs = pos % 60;
        let total_mins = total / 60;
        let total_secs = total % 60;

        let progress = if total > 0 { (pos as f64 / total as f64).clamp(0.0, 1.0) } else { 0.0 };
        let bar_width: usize = 25;
        let filled = (progress * bar_width as f64).round() as usize;
        let empty = bar_width.saturating_sub(filled);
        
        let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));

        format!(" {}  {} - {}  [{}] {:02}:{:02} / {:02}:{:02}  [Vol: {:>3}%]", 
            state, track.title, track.artist, bar, pos_mins, pos_secs, total_mins, total_secs, (app.audio.volume() * 100.0) as i32)
    } else {
        format!(" No track playing  [Vol: {:>3}%]", (app.audio.volume() * 100.0) as i32)
    };

    let p = Paragraph::new(content)
        .style(Style::default().fg(FG).bold())
        .block(block)
        .alignment(Alignment::Left);

    f.render_widget(p, area);
}
