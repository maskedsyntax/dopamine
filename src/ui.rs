use crate::app::{App, Confirmation, InputMode, View};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect, Alignment},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Row, Table, Paragraph, BorderType, List, ListItem, ListState, Clear},
    Frame,
};

fn c(rgb: (u8, u8, u8)) -> Color {
    Color::Rgb(rgb.0, rgb.1, rgb.2)
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let size = f.area();
    let theme = app.config.get_theme();
    let fg = c(theme.fg);
    let bg = c(theme.bg);
    let primary = c(theme.primary);
    let accent = c(theme.accent);
    let secondary = c(theme.secondary);
    let inactive = c(theme.inactive);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(size);

    draw_search(f, app, chunks[0], fg, accent, inactive);
    
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(25),
            Constraint::Min(0),
        ])
        .split(chunks[1]);

    draw_sidebar(f, app, main_chunks[0], fg, accent, inactive, secondary);
    
    match app.view {
        View::Home => draw_table(f, app, main_chunks[1], fg, primary, secondary, inactive),
        View::Artists => draw_list(f, &app.filtered_artists, &mut app.list_state, main_chunks[1], " Artists ", fg, accent, inactive),
        View::Albums => draw_list(f, &app.filtered_albums, &mut app.list_state, main_chunks[1], " Albums ", fg, accent, inactive),
        View::Genres => draw_list(f, &app.filtered_genres, &mut app.list_state, main_chunks[1], " Genres ", fg, accent, inactive),
        View::Years => {
            let items: Vec<String> = app.filtered_years.iter().map(|y| y.to_string()).collect();
            draw_list(f, &items, &mut app.list_state, main_chunks[1], " Years ", fg, accent, inactive);
        },
        View::Playlists => draw_list(f, &app.filtered_playlists, &mut app.list_state, main_chunks[1], " Playlists ", fg, accent, inactive),
        View::PlaylistDetail => draw_table(f, app, main_chunks[1], fg, primary, secondary, inactive),
        View::Queue => draw_queue(f, app, main_chunks[1], fg, primary, secondary, inactive),
        View::Lyrics => draw_lyrics(f, app, main_chunks[1], fg, accent, inactive),
        View::Equalizer => draw_equalizer(f, app, main_chunks[1], fg, accent, inactive),
        View::Devices => draw_devices(f, app, main_chunks[1], fg, accent, inactive),
        View::Dashboard => draw_dashboard(f, app, main_chunks[1], fg, primary, secondary, accent, inactive),
    }
    
    if let InputMode::SelectPlaylist(_) = &app.input_mode {
        draw_select_playlist(f, app, fg, bg, accent);
    }

    if let InputMode::Confirm(conf) = &app.input_mode {
        draw_confirmation(f, conf, fg, bg, accent);
    }

    if let InputMode::EditMetadata(_, field_idx) = &app.input_mode {
        draw_metadata_editor(f, app, *field_idx, fg, bg, accent, inactive);
    }

    if app.input_mode == InputMode::Help {
        draw_help(f, fg, bg, accent, primary);
    }

    draw_notifications(f, app, bg, accent);
    
    draw_player(f, app, chunks[2], fg, primary, accent, secondary, inactive);
}

fn draw_search(f: &mut Frame, app: &App, area: Rect, fg: Color, accent: Color, inactive: Color) {
    let search_style = match &app.input_mode {
        InputMode::Normal => Style::default().fg(inactive),
        _ => Style::default().fg(accent).add_modifier(Modifier::BOLD),
    };

    let title = match &app.input_mode {
        InputMode::Search => " Search (Active) ",
        InputMode::CreatePlaylist => " Create Playlist ",
        InputMode::SelectPlaylist(_) => " Select Playlist ",
        InputMode::Confirm(_) => " Confirm Action ",
        InputMode::Normal => " Search ('/' to focus) ",
        InputMode::EditMetadata(_, _) => " Edit Metadata ",
        InputMode::Help => " Help ",
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
        InputMode::Help => "Press ? or Esc to close help",
        _ => app.search_input.value(),
    };
    
    let text = if val.is_empty() && app.input_mode == InputMode::Normal {
        "..."
    } else {
        val
    };

    let p = Paragraph::new(text)
        .style(Style::default().fg(fg))
        .block(block);

    f.render_widget(p, area);
    
    match &app.input_mode {
        InputMode::Search => {
            let cursor = app.search_input.visual_cursor() as u16;
            let max_width = area.width.saturating_sub(2);
            f.set_cursor_position((
                area.x + 1 + cursor.min(max_width),
                area.y + 1,
            ));
        }
        InputMode::CreatePlaylist => {
            let cursor = app.playlist_input.visual_cursor() as u16;
            let max_width = area.width.saturating_sub(2);
            f.set_cursor_position((
                area.x + 1 + cursor.min(max_width),
                area.y + 1,
            ));
        }
        _ => {}
    }
}

fn draw_sidebar(f: &mut Frame, app: &App, area: Rect, fg: Color, accent: Color, inactive: Color, secondary: Color) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(inactive))
        .title(" Dopamine ");

    let home_highlight = app.view == View::Home;
    let artists_highlight = app.view == View::Artists;
    let albums_highlight = app.view == View::Albums;
    let genres_highlight = app.view == View::Genres;
    let years_highlight = app.view == View::Years;
    let playlists_highlight = app.view == View::Playlists || app.view == View::PlaylistDetail;
    let queue_highlight = app.view == View::Queue;
    let lyrics_highlight = app.view == View::Lyrics;
    let equalizer_highlight = app.view == View::Equalizer;
    let devices_highlight = app.view == View::Devices;

    let mut sidebar_items = vec![
        Line::from(vec![Span::styled(if home_highlight { "❯ Home" } else { "  Home" }, if home_highlight { Style::default().fg(accent).bold() } else { Style::default().fg(fg) })]),
        Line::from(vec![Span::styled(if artists_highlight { "❯ Artists" } else { "  Artists" }, if artists_highlight { Style::default().fg(accent).bold() } else { Style::default().fg(fg) })]),
        Line::from(vec![Span::styled(if albums_highlight { "❯ Albums" } else { "  Albums" }, if albums_highlight { Style::default().fg(accent).bold() } else { Style::default().fg(fg) })]),
        Line::from(vec![Span::styled(if genres_highlight { "❯ Genres" } else { "  Genres" }, if genres_highlight { Style::default().fg(accent).bold() } else { Style::default().fg(fg) })]),
        Line::from(vec![Span::styled(if years_highlight { "❯ Years" } else { "  Years" }, if years_highlight { Style::default().fg(accent).bold() } else { Style::default().fg(fg) })]),
        Line::from(vec![Span::styled(if playlists_highlight { "❯ Playlists" } else { "  Playlists" }, if playlists_highlight { Style::default().fg(accent).bold() } else { Style::default().fg(fg) })]),
        Line::from(vec![Span::styled(if queue_highlight { "❯ Queue" } else { "  Queue" }, if queue_highlight { Style::default().fg(accent).bold() } else { Style::default().fg(fg) })]),
        Line::from(vec![Span::styled(if lyrics_highlight { "❯ Lyrics" } else { "  Lyrics" }, if lyrics_highlight { Style::default().fg(accent).bold() } else { Style::default().fg(fg) })]),
        Line::from(vec![Span::styled(if equalizer_highlight { "❯ Equalizer" } else { "  Equalizer" }, if equalizer_highlight { Style::default().fg(accent).bold() } else { Style::default().fg(fg) })]),
        Line::from(vec![Span::styled(if devices_highlight { "❯ Devices" } else { "  Devices" }, if devices_highlight { Style::default().fg(accent).bold() } else { Style::default().fg(fg) })]),
        Line::from(vec![Span::styled(" ", Style::default())]),
    ];

    if app.scanning {
        let (curr, total) = app.scan_progress;
        let progress = if total > 0 { format!(" ({}/{})", curr, total) } else { "".to_string() };
        sidebar_items.push(Line::from(vec![Span::styled(format!("  Scanning{}...", progress), Style::default().fg(secondary).bold())]));
    } else {
        sidebar_items.push(Line::from(vec![Span::styled("  Press 's' to scan", Style::default().fg(inactive))]));
    }
    
    sidebar_items.push(Line::from(vec![Span::styled(" ", Style::default())]));
    sidebar_items.push(Line::from(vec![Span::styled("  n/p: Next/Prev track", Style::default().fg(inactive))]));
    sidebar_items.push(Line::from(vec![Span::styled("  z/r: Shuffle/Repeat", Style::default().fg(inactive))]));
    sidebar_items.push(Line::from(vec![Span::styled("  [/]: Speed -/+", Style::default().fg(inactive))]));
    sidebar_items.push(Line::from(vec![Span::styled("  h/l: Seek -/+ 10s", Style::default().fg(inactive))]));
    sidebar_items.push(Line::from(vec![Span::styled("  +/-: Volume", Style::default().fg(inactive))]));
    sidebar_items.push(Line::from(vec![Span::styled("  Ctrl-n: New Playlist", Style::default().fg(inactive))]));
    sidebar_items.push(Line::from(vec![Span::styled("  e: Edit | f: Fav | T: Timer", Style::default().fg(inactive))]));
    sidebar_items.push(Line::from(vec![Span::styled("  Del: Delete Playlist", Style::default().fg(inactive))]));
    sidebar_items.push(Line::from(vec![Span::styled("  q: Quit", Style::default().fg(inactive))]));

    let p = Paragraph::new(sidebar_items).block(block);
    f.render_widget(p, area);
}

fn draw_table(f: &mut Frame, app: &mut App, area: Rect, fg: Color, primary: Color, secondary: Color, inactive: Color) {
    let title = match app.view {
        View::Home => " All Tracks ".to_string(),
        View::PlaylistDetail => format!(" Playlist: {} ", app.selected_playlist.as_deref().unwrap_or("Unknown")),
        _ => " Tracks ".to_string(),
    };

    let header_cells = ["Title", "Artist", "Album", "Time"]
        .iter()
        .map(|h| Cell::from(*h).style(Style::default().fg(primary).bold()));
    let header = Row::new(header_cells).height(1).bottom_margin(1);

    let rows = app.filtered_tracks.iter().map(|t| {
        let is_playing = app.current_track.as_ref().map_or(false, |ct| ct.path == t.path);
        let style = if is_playing {
            Style::default().fg(secondary).bold()
        } else {
            Style::default().fg(fg)
        };

        let mins = t.duration_secs / 60;
        let secs = t.duration_secs % 60;
        let time_str = format!("{:02}:{:02}", mins, secs);
        let fav = if t.favorite { "⭐ " } else { "   " };

        Row::new(vec![
            Cell::from(format!("{}{}", fav, t.title)),
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
    .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(inactive)).title(title.as_str()))
    .row_highlight_style(Style::default().bg(Color::Rgb(49, 50, 68)).add_modifier(Modifier::BOLD))
    .highlight_symbol("❯ ");

    f.render_stateful_widget(t, area, &mut app.table_state);
}

fn draw_list(f: &mut Frame, items: &[String], state: &mut ListState, area: Rect, title: &str, fg: Color, accent: Color, inactive: Color) {
    let list_items: Vec<ListItem> = items.iter().map(|i| {
        ListItem::new(i.as_str()).style(Style::default().fg(fg))
    }).collect();

    let l = List::new(list_items)
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(inactive)).title(title))
        .highlight_style(Style::default().bg(Color::Rgb(49, 50, 68)).fg(accent).add_modifier(Modifier::BOLD))
        .highlight_symbol("❯ ");

    f.render_stateful_widget(l, area, state);
}

fn draw_queue(f: &mut Frame, app: &mut App, area: Rect, fg: Color, primary: Color, secondary: Color, inactive: Color) {
    let header_cells = ["#", "Title", "Artist", "Album", "Time"]
        .iter()
        .map(|h| Cell::from(*h).style(Style::default().fg(primary).bold()));
    let header = Row::new(header_cells).height(1).bottom_margin(1);

    let rows = app.queue.iter().enumerate().map(|(idx, t)| {
        let is_playing = app.current_track.as_ref().map_or(false, |ct| ct.path == t.path);
        let style = if is_playing {
            Style::default().fg(secondary).bold()
        } else {
            Style::default().fg(fg)
        };

        let mins = t.duration_secs / 60;
        let secs = t.duration_secs % 60;
        let time_str = format!("{:02}:{:02}", mins, secs);
        let fav = if t.favorite { "⭐ " } else { "   " };

        Row::new(vec![
            Cell::from((idx + 1).to_string()),
            Cell::from(format!("{}{}", fav, t.title)),
            Cell::from(t.artist.clone()),
            Cell::from(t.album.clone()),
            Cell::from(time_str),
        ]).style(style)
    });

    let t = Table::new(rows, [
        Constraint::Length(4),
        Constraint::Percentage(36),
        Constraint::Percentage(25),
        Constraint::Percentage(25),
        Constraint::Percentage(10),
    ])
    .header(header)
    .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(inactive)).title(" Playback Queue (J/K to move) "))
    .row_highlight_style(Style::default().bg(Color::Rgb(49, 50, 68)).add_modifier(Modifier::BOLD))
    .highlight_symbol("❯ ");

    f.render_stateful_widget(t, area, &mut app.table_state);
}

fn draw_lyrics(f: &mut Frame, app: &App, area: Rect, fg: Color, accent: Color, inactive: Color) {
    let track = match &app.current_track {
        Some(t) => t,
        None => {
            f.render_widget(Paragraph::new("No track playing").alignment(Alignment::Center), area);
            return;
        }
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(inactive))
        .title(format!(" Lyrics: {} ", track.title));

    let lyrics_raw = match &track.lyrics {
        Some(l) if l == "No lyrics available" => {
            f.render_widget(Paragraph::new("No lyrics found online").alignment(Alignment::Center).block(block), area);
            return;
        }
        Some(l) => l,
        None => {
            f.render_widget(Paragraph::new("No lyrics found (.lrc file missing)").alignment(Alignment::Center).block(block), area);
            return;
        }
    };

    let mut lines = Vec::new();
    for line in lyrics_raw.lines() {
        if line.starts_with('[') {
            let parts: Vec<&str> = line.splitn(2, ']').collect();
            if parts.len() == 2 {
                let time_str = parts[0].trim_start_matches('[');
                let text = parts[1].trim();
                
                let time_parts: Vec<&str> = time_str.split(':').collect();
                if time_parts.len() == 2 {
                    let mins: u64 = time_parts[0].parse().unwrap_or(0);
                    let secs: f64 = time_parts[1].parse().unwrap_or(0.0);
                    let total_ms = (mins * 60 * 1000) + (secs * 1000.0) as u64;
                    lines.push((total_ms, text));
                }
            }
        }
    }

    if lines.is_empty() {
        f.render_widget(Paragraph::new("No synchronized lyrics found in LRC file").alignment(Alignment::Center).block(block), area);
        return;
    }

    let current_ms = app.audio.position().as_millis() as i64;
    let adjusted_ms = current_ms + track.lyrics_offset_ms;
    
    // Find active line index based on adjusted time
    let active_idx = lines.iter().position(|(ms, _)| *ms as i64 > adjusted_ms).unwrap_or(lines.len()).saturating_sub(1);

    let inner_height = area.height.saturating_sub(2) as usize;
    let center_y = inner_height / 2;
    
    let mut spans = Vec::new();
    for (i, (_, text)) in lines.iter().enumerate() {
        let style = if i == active_idx {
            Style::default().fg(accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(fg)
        };
        spans.push(Line::from(vec![Span::styled(text.to_string(), style)]));
    }

    let scroll = active_idx.saturating_sub(center_y);
    let p = Paragraph::new(spans)
        .block(block)
        .alignment(Alignment::Center)
        .scroll((scroll as u16, 0));

    f.render_widget(p, area);

    // Display offset if not zero
    if track.lyrics_offset_ms != 0 {
        let offset_text = format!(" Offset: {}ms ", track.lyrics_offset_ms);
        let offset_rect = Rect::new(area.x + area.width.saturating_sub(offset_text.len() as u16 + 2), area.y, offset_text.len() as u16, 1);
        f.render_widget(Paragraph::new(offset_text).style(Style::default().fg(accent).bold()), offset_rect);
    }
}

fn draw_equalizer(f: &mut Frame, app: &mut App, area: Rect, fg: Color, accent: Color, inactive: Color) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(inactive))
        .title(format!(" Equalizer ({}) ", if app.audio.eq_enabled { "ON" } else { "OFF" }));

    f.render_widget(block, area);

    let inner = area.inner(ratatui::layout::Margin { vertical: 2, horizontal: 2 });
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(10); 10])
        .split(inner);

    let labels = ["60Hz", "170Hz", "310Hz", "600Hz", "1kHz", "3kHz", "6kHz", "12kHz", "14kHz", "16kHz"];
    let selected_idx = app.list_state.selected().unwrap_or(0);

    for i in 0..10 {
        let val = app.audio.eq_bands[i];
        let normalized = (val + 10.0) / 20.0;
        
        let bar_height = (inner.height.saturating_sub(4)) as f32;
        let filled_height = (normalized * bar_height) as u16;
        
        let style = if i == selected_idx {
            Style::default().fg(accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(fg)
        };

        let mut bar_text = Vec::new();
        for y in 0..bar_height as u16 {
            if y < (bar_height as u16 - filled_height) {
                bar_text.push(Line::from(vec![Span::raw("  ░  ")]));
            } else {
                bar_text.push(Line::from(vec![Span::styled("  █  ", style)]));
            }
        }
        
        let p = Paragraph::new(bar_text).alignment(Alignment::Center);
        f.render_widget(p, chunks[i]);
        
        let label_rect = Rect::new(chunks[i].x, inner.y + inner.height - 2, chunks[i].width, 1);
        let val_rect = Rect::new(chunks[i].x, inner.y + inner.height - 1, chunks[i].width, 1);
        
        f.render_widget(Paragraph::new(labels[i]).style(style).alignment(Alignment::Center), label_rect);
        f.render_widget(Paragraph::new(format!("{:.0}dB", val)).style(style).alignment(Alignment::Center), val_rect);
    }

    let instr_rect = Rect::new(area.x, area.y + area.height - 2, area.width, 1);
    let instructions = Paragraph::new("h/l: Select Band | j/k: Adjust | Enter: Toggle EQ | Backspace: Back")
        .style(Style::default().fg(inactive))
        .alignment(Alignment::Center);
    f.render_widget(instructions, instr_rect);
}

fn draw_devices(f: &mut Frame, app: &mut App, area: Rect, fg: Color, accent: Color, inactive: Color) {
    let items: Vec<ListItem> = app.audio_devices.iter().map(|d| {
        ListItem::new(d.as_str()).style(Style::default().fg(fg))
    }).collect();

    let l = List::new(items)
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(inactive)).title(" Output Devices (Enter to Select) "))
        .highlight_style(Style::default().bg(Color::Rgb(49, 50, 68)).fg(accent).add_modifier(Modifier::BOLD))
        .highlight_symbol("❯ ");

    f.render_stateful_widget(l, area, &mut app.list_state);
}

fn draw_dashboard(f: &mut Frame, app: &App, area: Rect, fg: Color, primary: Color, secondary: Color, accent: Color, inactive: Color) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(inactive))
        .title(" Statistics Dashboard ");

    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5), // Summary
            Constraint::Percentage(45), // Top Artists
            Constraint::Percentage(45), // Top Tracks
        ])
        .margin(2)
        .split(area);

    // 1. Summary
    if let Ok((total_plays, total_secs)) = app.db.get_total_stats() {
        let hours = total_secs / 3600;
        let mins = (total_secs % 3600) / 60;
        
        let stats_text = vec![
            Line::from(vec![
                Span::styled(" Total Plays: ", Style::default().fg(fg)),
                Span::styled(total_plays.to_string(), Style::default().fg(accent).bold()),
                Span::styled("   Total Time: ", Style::default().fg(fg)),
                Span::styled(format!("{}h {}m", hours, mins), Style::default().fg(secondary).bold()),
            ]),
        ];
        
        f.render_widget(Paragraph::new(stats_text).block(Block::default().title(" Overall Summary ").borders(Borders::BOTTOM).border_style(Style::default().fg(inactive))), chunks[0]);
    }

    // 2. Top Artists
    if let Ok(artists) = app.db.get_top_artists() {
        let rows = artists.into_iter().enumerate().map(|(i, (name, count))| {
            Row::new(vec![
                Cell::from((i + 1).to_string()),
                Cell::from(name),
                Cell::from(format!("{} plays", count)),
            ]).style(Style::default().fg(fg))
        });
        
        let t = Table::new(rows, [
            Constraint::Length(4),
            Constraint::Percentage(70),
            Constraint::Percentage(20),
        ])
        .block(Block::default().title(" Top Artists ").borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(inactive)))
        .header(Row::new(vec!["#", "Artist", "Plays"]).style(Style::default().fg(primary).bold()));
        
        f.render_widget(t, chunks[1]);
    }

    // 3. Top Tracks
    if let Ok(tracks) = app.db.get_most_played() {
        let rows = tracks.into_iter().take(10).enumerate().map(|(i, t)| {
            Row::new(vec![
                Cell::from((i + 1).to_string()),
                Cell::from(t.title),
                Cell::from(t.artist),
                Cell::from(format!("{} plays", t.play_count)),
            ]).style(Style::default().fg(fg))
        });

        let t = Table::new(rows, [
            Constraint::Length(4),
            Constraint::Percentage(40),
            Constraint::Percentage(30),
            Constraint::Percentage(20),
        ])
        .block(Block::default().title(" Most Played Tracks ").borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(inactive)))
        .header(Row::new(vec!["#", "Title", "Artist", "Plays"]).style(Style::default().fg(primary).bold()));
        
        f.render_widget(t, chunks[2]);
    }
}

fn draw_select_playlist(f: &mut Frame, app: &mut App, fg: Color, bg: Color, accent: Color) {
    let area = centered_rect(60, 40, f.area());
    f.render_widget(Clear, area);

    let items: Vec<ListItem> = app.playlists.iter().map(|p| {
        ListItem::new(p.as_str()).style(Style::default().fg(fg))
    }).collect();

    let l = List::new(items)
        .block(Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(accent))
            .style(Style::default().bg(bg))
            .title(" Add to Playlist "))
        .highlight_style(Style::default().bg(Color::Rgb(49, 50, 68)).fg(accent).add_modifier(Modifier::BOLD))
        .highlight_symbol("❯ ");

    f.render_stateful_widget(l, area, &mut app.playlist_select_state);
}

fn draw_confirmation(f: &mut Frame, conf: &Confirmation, fg: Color, bg: Color, accent: Color) {
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
        .style(Style::default().fg(fg))
        .block(Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(accent))
            .style(Style::default().bg(bg))
            .title(title));

    f.render_widget(p, area);
}

fn draw_metadata_editor(f: &mut Frame, app: &App, field_idx: usize, fg: Color, bg: Color, accent: Color, inactive: Color) {
    let area = centered_rect(60, 40, f.area());
    f.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent))
        .style(Style::default().bg(bg))
        .title(" Edit Metadata ");

    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title
            Constraint::Length(3), // Artist
            Constraint::Length(3), // Album
            Constraint::Length(3), // Genre
            Constraint::Length(3), // Year
            Constraint::Min(0),    // Instructions
        ])
        .margin(2)
        .split(area);

    let fields = ["Title", "Artist", "Album", "Genre", "Year"];
    for i in 0..5 {
        let style = if field_idx == i {
            Style::default().fg(accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(fg)
        };

        let p = Paragraph::new(app.edit_inputs[i].value())
            .block(Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(style)
                .title(format!(" {} ", fields[i])));
        
        f.render_widget(p, chunks[i]);
    }

    let instructions = Paragraph::new("Tab: Switch | Enter: Save | Esc: Cancel")
        .style(Style::default().fg(inactive))
        .alignment(Alignment::Center);
    f.render_widget(instructions, chunks[5]);

    let active_area = chunks[field_idx];
    let cursor = app.edit_inputs[field_idx].visual_cursor() as u16;
    let max_width = active_area.width.saturating_sub(2);
    f.set_cursor_position((
        active_area.x + 1 + cursor.min(max_width),
        active_area.y + 1,
    ));
}

fn draw_help(f: &mut Frame, fg: Color, bg: Color, accent: Color, primary: Color) {
    let area = centered_rect(80, 80, f.area());
    f.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent))
        .style(Style::default().bg(bg))
        .title(" Help & Keybindings ");

    let help_text = vec![
        Line::from(vec![Span::styled("Navigation", Style::default().fg(primary).bold())]),
        Line::from(vec![Span::raw("  1-9, 0, +: Views (Home, Art, Alb, Gen, Year, Pl, Q, Ly, EQ, Dev, Stats)")]),
        Line::from(vec![Span::raw("  j/k: Navigate list | Enter: Play/Select")]),
        Line::from(vec![Span::raw("  Backspace: Back | /: Search")]),
        Line::from(vec![Span::raw(" ")]),
        Line::from(vec![Span::styled("Playback", Style::default().fg(primary).bold())]),
        Line::from(vec![Span::raw("  Space: Play/Pause | n/p: Next/Prev track")]),
        Line::from(vec![Span::raw("  h/l: Seek -/+ 10s | +/-: Volume")]),
        Line::from(vec![Span::raw("  z: Toggle Shuffle | r: Toggle Repeat")]),
        Line::from(vec![Span::raw("  [/]: Speed -/+    | {/}: Lyrics Sync -/+")]),
        Line::from(vec![Span::raw(" ")]),
        Line::from(vec![Span::styled("Management", Style::default().fg(primary).bold())]),
        Line::from(vec![Span::raw("  Ctrl-n: New Playlist | a: Add to Playlist")]),
        Line::from(vec![Span::raw("  e: Edit Metadata | f: Toggle Favorite")]),
        Line::from(vec![Span::raw("  E: Export Playlist (M3U) | s: Scan Library")]),
        Line::from(vec![Span::raw("  Del: Delete Playlist (in Playlists view)")]),
        Line::from(vec![Span::raw("  J/K: Move track in Queue (in Queue view)")]),
        Line::from(vec![Span::raw(" ")]),
        Line::from(vec![Span::styled("General", Style::default().fg(primary).bold())]),
        Line::from(vec![Span::raw("  ?: Toggle Help | q: Quit")]),
    ];

    let p = Paragraph::new(help_text)
        .style(Style::default().fg(fg))
        .block(block)
        .alignment(Alignment::Left);

    f.render_widget(p, area);
}

fn draw_notifications(f: &mut Frame, app: &App, bg: Color, accent: Color) {
    if app.notifications.is_empty() { return; }

    let area = f.area();
    let max_notifications = (area.height / 3).saturating_sub(1) as usize;
    let mut y = 1;
    
    for (msg, _) in app.notifications.iter().rev().take(max_notifications) {
        let width = (msg.len() + 4) as u16;
        let rect = Rect::new(area.width.saturating_sub(width + 1), y, width, 3);
        f.render_widget(Clear, rect);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(accent))
            .style(Style::default().bg(bg));
        let p = Paragraph::new(msg.as_str())
            .block(block)
            .alignment(Alignment::Center);
        f.render_widget(p, rect);
        y += 3;
    }
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

fn draw_player(f: &mut Frame, app: &App, area: Rect, fg: Color, primary: Color, accent: Color, _secondary: Color, inactive: Color) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(primary));

    f.render_widget(block, area);

    let inner = Rect::new(area.x + 1, area.y + 1, area.width.saturating_sub(2), 1);
    
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(12), // Visualizer
            Constraint::Length(4),  // Play/Pause icon
            Constraint::Min(10),    // Marquee text
            Constraint::Length(15), // Shuffle/Repeat/Speed/Timer
            Constraint::Length(27), // Progress bar
            Constraint::Length(15), // Time
            Constraint::Length(12), // Volume
        ])
        .split(inner);

    let vol = format!(" Vol: {:>3}%", (app.audio.volume() * 100.0) as i32);
    f.render_widget(Paragraph::new(vol).style(Style::default().fg(fg).bold()).alignment(Alignment::Right), chunks[6]);

    if let Some(track) = &app.current_track {
        let pos = app.audio.position().as_secs();
        let total = track.duration_secs.max(0) as u64;
        let pos_mins = pos / 60;
        let pos_secs = pos % 60;
        let total_mins = total / 60;
        let total_secs = total % 60;
        let time_str = format!(" {:02}:{:02} / {:02}:{:02}", pos_mins, pos_secs, total_mins, total_secs);
        f.render_widget(Paragraph::new(time_str).style(Style::default().fg(fg).bold()), chunks[5]);

        let progress = if total > 0 { (pos as f64 / total as f64).clamp(0.0, 1.0) } else { 0.0 };
        let bar_width = (chunks[4].width as usize).saturating_sub(2);
        let filled = (progress * bar_width as f64).round() as usize;
        let empty = bar_width.saturating_sub(filled);
        let bar = format!("[{}{}]", "█".repeat(filled), "░".repeat(empty));
        f.render_widget(Paragraph::new(bar).style(Style::default().fg(fg)), chunks[4]);

        let shuffle_icon = if app.shuffle { "󰒟 " } else { "  " };
        let repeat_icon = match app.repeat_mode {
            crate::app::RepeatMode::None => "  ",
            crate::app::RepeatMode::One => "󰑘 ",
            crate::app::RepeatMode::All => "󰑖 ",
        };
        let speed = format!(" {:.1}x", app.audio.playback_speed());
        let timer = if let Some((start, dur)) = app.sleep_timer {
            let rem = dur.as_secs().saturating_sub(start.elapsed().as_secs());
            format!(" 󱎫 {:02}:{:02}", rem / 60, rem % 60)
        } else {
            "".to_string()
        };
        let status = format!("{} {} {}{}", shuffle_icon, repeat_icon, speed, timer);
        f.render_widget(Paragraph::new(status).style(Style::default().fg(accent).bold()), chunks[3]);

        let display_text = format!(" {} - {}", track.title, track.artist);
        let max_text_len = chunks[2].width as usize;
        let final_text = if display_text.len() > max_text_len && max_text_len > 0 {
            let padded = format!("{}   ", display_text);
            let start = (app.marquee_offset / 4) % padded.len();
            let mut result = String::new();
            for i in 0..max_text_len {
                result.push(padded.chars().nth((start + i) % padded.len()).unwrap_or(' '));
            }
            result
        } else {
            display_text
        };
        f.render_widget(Paragraph::new(final_text).style(Style::default().fg(fg).bold()), chunks[2]);
    } else {
        f.render_widget(Paragraph::new(" No track playing").style(Style::default().fg(inactive)), chunks[2]);
    }

    let num_bars = app.visualizer_data.len();
    let viz_width = chunks[0].width as usize;
    let mut bars = String::new();
    let bar_chars = [" ", "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
    for i in 0..viz_width {
        let data_idx = (i * num_bars) / viz_width.max(1);
        let val = app.visualizer_data[data_idx.clamp(0, num_bars - 1)];
        let char_idx = (val * (bar_chars.len() - 1) as f32).round() as usize;
        bars.push_str(bar_chars[char_idx.clamp(0, bar_chars.len() - 1)]);
    }
    f.render_widget(Paragraph::new(bars).style(Style::default().fg(accent)), chunks[0]);

    let state = if app.audio.is_paused() { " ▶ " } else { " ⏸ " };
    f.render_widget(Paragraph::new(state).style(Style::default().fg(fg).bold()), chunks[1]);
}
