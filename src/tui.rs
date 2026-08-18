use crate::{
    app::{App, DownloadStage, Hit, Overlay, View, filtered_commands},
    queue::RepeatMode,
};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Margin, Rect},
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, Paragraph, Row, Table, Wrap},
};

#[derive(Clone, Copy)]
struct Palette {
    bg: Color,
    surface: Color,
    text: Color,
    muted: Color,
    accent: Color,
    secondary: Color,
    error: Color,
    selected: Color,
}

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    app.hits.clear();
    let p = palette(app);
    frame.render_widget(
        Block::default().style(Style::default().bg(p.bg).fg(p.text)),
        area,
    );
    if area.width < 80 || area.height < 24 {
        draw_too_small(frame, app, area, p);
        return;
    }

    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(8),
        Constraint::Length(if area.width >= 110 { 4 } else { 3 }),
        Constraint::Length(1),
    ])
    .split(area);
    draw_header(frame, app, rows[0], p);
    if area.width >= 120 {
        let cols = Layout::horizontal([Constraint::Length(22), Constraint::Min(45)]).split(rows[1]);
        draw_sidebar(frame, app, cols[0], p);
        draw_content(frame, app, cols[1], p);
    } else {
        draw_content(frame, app, rows[1], p);
    }
    draw_player(frame, app, rows[2], p);
    draw_footer(frame, app, rows[3], p);
    draw_overlay(frame, app, area, p);
}

fn palette(app: &App) -> Palette {
    let t = app.config.get_theme();
    let c = |v: (u8, u8, u8)| Color::Rgb(v.0, v.1, v.2);
    Palette {
        bg: c(t.bg),
        surface: Color::Rgb(
            t.bg.0.saturating_add(14),
            t.bg.1.saturating_add(14),
            t.bg.2.saturating_add(14),
        ),
        text: c(t.fg),
        muted: c(t.inactive),
        accent: c(t.primary),
        secondary: c(t.secondary),
        error: Color::Rgb(242, 100, 100),
        selected: c(t.accent),
    }
}

fn draw_too_small(frame: &mut Frame, app: &mut App, area: Rect, p: Palette) {
    let playing = app
        .queue
        .current()
        .map(|t| format!("{} — {}", t.title, t.artist))
        .unwrap_or_else(|| "Nothing playing".into());
    let text = Text::from(vec![
        Line::styled("DOPAMINE", Style::default().fg(p.accent).bold()),
        Line::raw(""),
        Line::raw(format!("Terminal: {}×{}", area.width, area.height)),
        Line::raw("Minimum: 80×24"),
        Line::raw(""),
        Line::raw(playing),
        Line::raw("Space play/pause  q quit"),
    ]);
    frame.render_widget(
        Paragraph::new(text).alignment(Alignment::Center).block(
            Block::bordered()
                .title(" Resize terminal ")
                .style(Style::default().fg(p.text).bg(p.bg)),
        ),
        area,
    );
}

fn draw_header(frame: &mut Frame, app: &mut App, area: Rect, p: Palette) {
    let title = if app.view == View::Detail {
        format!("Library / {}", app.detail_title)
    } else {
        app.view.title().to_string()
    };
    let right = if app.query.is_empty() {
        "/ Search   Ctrl+P Commands".to_string()
    } else {
        format!("Search: {}", app.query)
    };
    let top = Rect::new(area.x, area.y, area.width, 1);
    let chunks = Layout::horizontal([
        Constraint::Min(20),
        Constraint::Length(right.chars().count().min(area.width as usize / 2) as u16),
    ])
    .split(top);
    frame.render_widget(
        Paragraph::new(format!("  {title}"))
            .style(Style::default().fg(p.text).bg(p.surface).bold()),
        chunks[0],
    );
    frame.render_widget(
        Paragraph::new(right).alignment(Alignment::Right).style(
            Style::default()
                .fg(if app.query.is_empty() {
                    p.muted
                } else {
                    p.accent
                })
                .bg(p.surface),
        ),
        chunks[1],
    );
    if area.width < 120 && area.height > 1 {
        let tabs = [
            (View::Home, "Home"),
            (View::Tracks, "Library"),
            (View::Playlists, "Playlists"),
            (View::Downloads, "Download"),
            (View::Queue, "Queue"),
            (View::Settings, "Settings"),
        ];
        let widths = tabs
            .iter()
            .map(|(_, label)| Constraint::Length(label.len() as u16 + 2));
        let rects = Layout::horizontal(widths).split(Rect::new(area.x, area.y + 1, area.width, 1));
        for ((view, label), rect) in tabs.into_iter().zip(rects.iter().copied()) {
            frame.render_widget(
                Paragraph::new(format!(" {label} ")).style(if app.view == view {
                    Style::default().fg(p.text).bg(p.selected).bold()
                } else {
                    Style::default().fg(p.muted).bg(p.surface)
                }),
                rect,
            );
            app.hits.push((rect, Hit::Route(view)));
        }
    }
}

fn nav_items() -> [(View, &'static str); 10] {
    [
        (View::Home, "Home"),
        (View::Tracks, "Library"),
        (View::Playlists, "Playlists"),
        (View::Downloads, "SoundSnatch"),
        (View::Queue, "Queue"),
        (View::Lyrics, "Lyrics"),
        (View::Statistics, "Statistics"),
        (View::Equalizer, "Equalizer"),
        (View::Devices, "Devices"),
        (View::Settings, "Settings"),
    ]
}
fn draw_sidebar(frame: &mut Frame, app: &mut App, area: Rect, p: Palette) {
    frame.render_widget(
        Block::default()
            .borders(Borders::RIGHT)
            .border_style(Style::default().fg(p.muted))
            .style(Style::default().bg(p.surface)),
        area,
    );
    let inner = area.inner(Margin {
        horizontal: 1,
        vertical: 1,
    });
    let mut y = inner.y;
    frame.render_widget(
        Paragraph::new(" DOPAMINE ").style(Style::default().fg(p.accent).bold()),
        Rect::new(inner.x, y, inner.width, 2),
    );
    y += 3;
    for (view, label) in nav_items() {
        let r = Rect::new(inner.x, y, inner.width, 1);
        let marker = if app.view == view { "›" } else { " " };
        let style = if app.view == view {
            Style::default().fg(p.text).bg(p.selected).bold()
        } else {
            Style::default().fg(p.muted)
        };
        frame.render_widget(Paragraph::new(format!("{marker} {label}")).style(style), r);
        app.hits.push((r, Hit::Route(view)));
        y += 2;
    }
}

fn draw_content(frame: &mut Frame, app: &mut App, area: Rect, p: Palette) {
    let inner = area.inner(Margin {
        horizontal: 1,
        vertical: 0,
    });
    let library_view = matches!(
        app.view,
        View::Tracks
            | View::Artists
            | View::Albums
            | View::Genres
            | View::Favorites
            | View::Recent
            | View::MostPlayed
            | View::Detail
    );
    let content = if library_view && app.view != View::Detail {
        let rows = Layout::vertical([Constraint::Length(1), Constraint::Min(4)]).split(inner);
        draw_library_tabs(frame, app, rows[0], p);
        rows[1]
    } else {
        inner
    };
    match app.view {
        View::Home => draw_home(frame, app, content, p),
        v if v.is_track_list() => draw_tracks(frame, app, content, p),
        View::Artists | View::Albums | View::Genres | View::Playlists => {
            draw_collections(frame, app, content, p)
        }
        View::Queue => draw_queue(frame, app, content, p),
        View::Lyrics => draw_lyrics(frame, app, content, p),
        View::Statistics => draw_statistics(frame, app, content, p),
        View::Equalizer => draw_equalizer(frame, app, content, p),
        View::Devices => draw_devices(frame, app, content, p),
        View::Scan => draw_scan(frame, app, content, p),
        View::Downloads => draw_downloads(frame, app, content, p),
        View::Settings => draw_settings(frame, app, content, p),
        _ => {}
    }
}

fn draw_library_tabs(frame: &mut Frame, app: &mut App, area: Rect, p: Palette) {
    let tabs = [
        (View::Tracks, "Tracks"),
        (View::Artists, "Artists"),
        (View::Albums, "Albums"),
        (View::Genres, "Genres"),
        (View::Favorites, "Favorites"),
        (View::Recent, "Recent"),
        (View::MostPlayed, "Top"),
    ];
    let rects = Layout::horizontal(
        tabs.iter()
            .map(|(_, label)| Constraint::Length(label.len() as u16 + 2)),
    )
    .split(area);
    for ((view, label), rect) in tabs.into_iter().zip(rects.iter().copied()) {
        frame.render_widget(
            Paragraph::new(format!(" {label} ")).style(if app.view == view {
                Style::default().fg(p.text).bg(p.selected).bold()
            } else {
                Style::default().fg(p.muted)
            }),
            rect,
        );
        app.hits.push((rect, Hit::Route(view)));
    }
}

fn draw_home(frame: &mut Frame, app: &mut App, area: Rect, p: Palette) {
    let rows = Layout::vertical([
        Constraint::Length(5),
        Constraint::Length(4),
        Constraint::Min(5),
    ])
    .split(area);
    let duration: i64 = app.tracks.iter().map(|t| t.duration_secs.max(0)).sum();
    let hero = Text::from(vec![
        Line::from(vec![Span::styled(
            "Your music. Fully local.",
            Style::default().fg(p.accent).bold(),
        )]),
        Line::raw(format!(
            "{} tracks  •  {} artists  •  {} albums  •  {}h {}m",
            app.tracks.len(),
            app.artists.len(),
            app.albums.len(),
            duration / 3600,
            (duration % 3600) / 60
        )),
        Line::styled(
            "Enter opens a section  •  S scans the library",
            Style::default().fg(p.muted),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(hero)
            .block(
                Block::bordered()
                    .title(" Welcome to Dopamine ")
                    .border_style(Style::default().fg(p.accent)),
            )
            .wrap(Wrap { trim: true }),
        rows[0],
    );
    let quick = [
        (View::Tracks, "All Tracks"),
        (View::Favorites, "Favorites"),
        (View::Recent, "Recent"),
        (View::MostPlayed, "Most Played"),
    ];
    let cols = Layout::horizontal(quick.iter().map(|_| Constraint::Ratio(1, 4))).split(rows[1]);
    for ((view, label), rect) in quick.into_iter().zip(cols.iter().copied()) {
        frame.render_widget(
            Paragraph::new(label)
                .alignment(Alignment::Center)
                .block(Block::bordered())
                .style(Style::default().fg(p.text).bg(p.surface)),
            rect,
        );
        app.hits.push((rect, Hit::Route(view)));
    }
    let mut lines = vec![Line::styled(
        "Rediscover",
        Style::default().fg(p.secondary).bold(),
    )];
    if app.tracks.is_empty() {
        lines.push(Line::raw(
            "Your library is empty. Configure a music folder in Settings, then press S.",
        ));
    } else {
        for t in app
            .tracks
            .iter()
            .take((rows[2].height.saturating_sub(2)) as usize)
        {
            lines.push(Line::from(vec![
                Span::styled("♪ ", Style::default().fg(p.accent)),
                Span::raw(&t.title),
                Span::styled(format!(" — {}", t.artist), Style::default().fg(p.muted)),
            ]));
        }
    }
    frame.render_widget(Paragraph::new(lines).block(Block::bordered()), rows[2]);
}

fn visible_window(len: usize, selected: usize, height: usize) -> (usize, usize) {
    if len <= height {
        return (0, len);
    }
    let start = selected.saturating_sub(height / 2).min(len - height);
    (start, (start + height).min(len))
}
fn draw_tracks(frame: &mut Frame, app: &mut App, area: Rect, p: Palette) {
    let tracks = app.visible_tracks();
    let table_height = area.height.saturating_sub(3) as usize;
    let (start, end) = visible_window(tracks.len(), app.selected, table_height);
    let header = Row::new(if area.width >= 110 {
        vec!["", "Title", "Artist", "Album", "Duration"]
    } else if area.width >= 90 {
        vec!["", "Title", "Artist", "Duration"]
    } else {
        vec!["", "Title", "Artist"]
    })
    .style(Style::default().fg(p.accent).bold())
    .bottom_margin(1);
    let rows = tracks[start..end].iter().enumerate().map(|(offset, t)| {
        let i = start + offset;
        let playing = app.queue.current().is_some_and(|q| q.path == t.path);
        let mark = if playing {
            "▶"
        } else if t.favorite {
            "♥"
        } else {
            " "
        };
        let mut cells = vec![mark.to_string(), t.title.clone(), t.artist.clone()];
        if area.width >= 110 {
            cells.push(t.album.clone());
            cells.push(format_duration(t.duration_secs));
        } else if area.width >= 90 {
            cells.push(format_duration(t.duration_secs));
        }
        Row::new(cells)
            .style(if i == app.selected {
                Style::default().fg(p.text).bg(p.selected).bold()
            } else if playing {
                Style::default().fg(p.secondary)
            } else {
                Style::default()
            })
            .height(1)
    });
    let widths = if area.width >= 110 {
        vec![
            Constraint::Length(2),
            Constraint::Percentage(28),
            Constraint::Percentage(24),
            Constraint::Percentage(28),
            Constraint::Length(7),
        ]
    } else if area.width >= 90 {
        vec![
            Constraint::Length(2),
            Constraint::Percentage(40),
            Constraint::Percentage(35),
            Constraint::Length(7),
        ]
    } else {
        vec![
            Constraint::Length(2),
            Constraint::Percentage(52),
            Constraint::Percentage(45),
        ]
    };
    frame.render_widget(
        Table::new(rows, widths)
            .header(header)
            .column_spacing(1)
            .block(Block::bordered().title(format!(" {} tracks ", tracks.len()))),
        area,
    );
    for (offset, _) in tracks[start..end].iter().enumerate() {
        let y = area.y + 3 + offset as u16;
        if y < area.bottom() {
            app.hits.push((
                Rect::new(area.x + 1, y, area.width.saturating_sub(2), 1),
                Hit::Row(start + offset),
            ));
        }
    }
}

fn draw_collections(frame: &mut Frame, app: &mut App, area: Rect, p: Palette) {
    let names = app.visible_names();
    let h = area.height.saturating_sub(2) as usize;
    let (start, end) = visible_window(names.len(), app.selected, h);
    let items = names[start..end].iter().enumerate().map(|(o, name)| {
        let i = start + o;
        let subtitle = match app.view {
            View::Playlists => app
                .playlists
                .iter()
                .find(|x| &x.name == name)
                .map(|x| {
                    format!(
                        "  {} tracks • {}",
                        x.track_count,
                        format_duration(x.duration_secs)
                    )
                })
                .unwrap_or_default(),
            _ => String::new(),
        };
        ListItem::new(format!("  {name}{subtitle}")).style(if i == app.selected {
            Style::default().fg(p.text).bg(p.selected).bold()
        } else {
            Style::default().fg(p.text)
        })
    });
    frame.render_widget(
        List::new(items)
            .block(Block::bordered().title(format!(" {} • Enter open • a actions ", names.len()))),
        area,
    );
    for o in 0..end - start {
        app.hits.push((
            Rect::new(
                area.x + 1,
                area.y + 1 + o as u16,
                area.width.saturating_sub(2),
                1,
            ),
            Hit::Row(start + o),
        ));
    }
}

fn draw_queue(frame: &mut Frame, app: &mut App, area: Rect, p: Palette) {
    let h = area.height.saturating_sub(2) as usize;
    let (start, end) = visible_window(app.queue.items.len(), app.selected, h);
    let items = app.queue.items[start..end]
        .iter()
        .enumerate()
        .map(|(o, t)| {
            let i = start + o;
            let mark = if i == app.queue.current_index {
                "▶"
            } else {
                " "
            };
            ListItem::new(format!("{mark} {:>3}. {} — {}", i + 1, t.title, t.artist)).style(
                if i == app.selected {
                    Style::default().fg(p.text).bg(p.selected).bold()
                } else {
                    Style::default()
                },
            )
        });
    frame.render_widget(
        List::new(items).block(Block::bordered().title(format!(
            " {} items • J/K reorder • Del remove • a actions ",
            app.queue.items.len()
        ))),
        area,
    );
    for o in 0..end - start {
        app.hits.push((
            Rect::new(
                area.x + 1,
                area.y + 1 + o as u16,
                area.width.saturating_sub(2),
                1,
            ),
            Hit::Row(start + o),
        ));
    }
}

fn parse_lyrics(value: &str) -> Vec<(Option<i64>, String)> {
    value
        .lines()
        .filter_map(|line| {
            if let Some(end) = line.find(']')
                && line.starts_with('[')
            {
                let stamp = &line[1..end];
                let mut p = stamp.split(':');
                if let (Some(m), Some(s)) = (p.next(), p.next())
                    && let (Ok(m), Ok(s)) = (m.parse::<i64>(), s.parse::<f64>())
                {
                    return Some((
                        Some(m * 60_000 + (s * 1000.0) as i64),
                        line[end + 1..].trim().to_string(),
                    ));
                }
            }
            (!line.trim().is_empty()).then(|| (None, line.to_string()))
        })
        .collect()
}
fn draw_lyrics(frame: &mut Frame, app: &mut App, area: Rect, p: Palette) {
    let Some(track) = app.queue.current() else {
        frame.render_widget(empty("Play a track to view lyrics", p), area);
        return;
    };
    let parsed = track
        .lyrics
        .as_deref()
        .map(parse_lyrics)
        .unwrap_or_default();
    if parsed.is_empty() {
        frame.render_widget(
            empty(
                "No lyrics stored • playback automatically tries online lookup",
                p,
            ),
            area,
        );
        return;
    }
    let now = app.position.as_millis() as i64 + track.lyrics_offset_ms;
    let current = parsed
        .iter()
        .rposition(|(t, _)| t.is_some_and(|t| t <= now))
        .unwrap_or(0);
    let h = area.height.saturating_sub(4) as usize;
    let (start, end) = visible_window(parsed.len(), current, h);
    let lines = parsed[start..end]
        .iter()
        .enumerate()
        .map(|(o, (_, line))| {
            let i = start + o;
            Line::styled(
                format!("  {line}"),
                if i == current {
                    Style::default().fg(p.accent).bold()
                } else {
                    Style::default().fg(p.muted)
                },
            )
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .block(Block::bordered().title(format!(
                " {} — {} • offset {:+}ms • [ ] adjust ",
                track.title, track.artist, track.lyrics_offset_ms
            ))),
        area,
    );
}

fn draw_statistics(frame: &mut Frame, app: &mut App, area: Rect, p: Palette) {
    let (stats, top) = (
        app.db.get_total_stats().unwrap_or_default(),
        app.db.get_top_artists().unwrap_or_default(),
    );
    let rows = Layout::vertical([Constraint::Length(5), Constraint::Min(5)]).split(area);
    let cols = Layout::horizontal([
        Constraint::Ratio(1, 3),
        Constraint::Ratio(1, 3),
        Constraint::Ratio(1, 3),
    ])
    .split(rows[0]);
    for (rect, (label, value)) in cols.iter().copied().zip([
        ("TOTAL PLAYS", stats.0.to_string()),
        (
            "LISTENING TIME",
            format!("{}h {}m", stats.1 / 3600, (stats.1 % 3600) / 60),
        ),
        ("LIBRARY", format!("{} tracks", app.tracks.len())),
    ]) {
        frame.render_widget(
            Paragraph::new(value)
                .alignment(Alignment::Center)
                .style(Style::default().fg(p.accent).bold())
                .block(Block::bordered().title(label)),
            rect,
        );
    }
    let lines = top
        .iter()
        .enumerate()
        .map(|(i, (artist, count))| {
            Line::from(vec![
                Span::raw(format!("{:>2}. {artist}", i + 1)),
                Span::styled(format!("  {count} plays"), Style::default().fg(p.muted)),
            ])
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(" Top Artists ")),
        rows[1],
    );
}

fn draw_equalizer(frame: &mut Frame, app: &mut App, area: Rect, p: Palette) {
    let labels = [
        "60", "170", "310", "600", "1k", "3k", "6k", "12k", "14k", "16k",
    ];
    let Some(audio) = app.audio.as_ref() else {
        frame.render_widget(empty("Audio engine unavailable", p), area);
        return;
    };
    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(8),
        Constraint::Length(2),
    ])
    .split(area);
    frame.render_widget(
        Paragraph::new(format!(
            "EQ {}  •  ←/→ select  ↑/↓ adjust  •  Enter toggle",
            if audio.eq_enabled { "ON" } else { "OFF" }
        ))
        .style(Style::default().fg(if audio.eq_enabled {
            p.secondary
        } else {
            p.muted
        })),
        rows[0],
    );
    let cols = Layout::horizontal((0..10).map(|_| Constraint::Ratio(1, 10))).split(rows[1]);
    for (i, rect) in cols.iter().copied().enumerate() {
        let gain = audio.eq_bands[i];
        let level = ((gain + 10.0) / 20.0).clamp(0.0, 1.0);
        frame.render_widget(
            Gauge::default()
                .gauge_style(
                    Style::default()
                        .fg(if i == app.eq_band {
                            p.accent
                        } else {
                            p.secondary
                        })
                        .bg(p.surface),
                )
                .ratio(level as f64)
                .label(format!("{gain:+.0}")),
            rect,
        );
    }
    frame.render_widget(
        Paragraph::new(labels.join("     "))
            .alignment(Alignment::Center)
            .style(Style::default().fg(p.muted)),
        rows[2],
    );
}

fn draw_devices(frame: &mut Frame, app: &mut App, area: Rect, p: Palette) {
    if app.devices.is_empty() {
        frame.render_widget(empty("No output devices detected", p), area);
        return;
    }
    draw_collections(frame, app, area, p)
}
fn draw_scan(frame: &mut Frame, app: &mut App, area: Rect, p: Palette) {
    let rows = Layout::vertical([
        Constraint::Length(5),
        Constraint::Length(3),
        Constraint::Min(3),
    ])
    .split(area);
    frame.render_widget(Paragraph::new("Scans configured folders in the background and safely updates SQLite.\nPress S to start or return to Settings to add folders.").wrap(Wrap{trim:true}).block(Block::bordered().title(" Library refresh ")),rows[0]);
    if let Some((d, ds, c, total)) = app.scan_progress {
        let ratio = if total == 0 {
            0.0
        } else {
            c as f64 / total as f64
        };
        frame.render_widget(
            Gauge::default()
                .block(Block::bordered().title(format!(" Directory {d}/{ds} • file {c}/{total} ")))
                .gauge_style(Style::default().fg(p.accent).bg(p.surface))
                .ratio(ratio),
            rows[1],
        );
    }
}

fn draw_downloads(frame: &mut Frame, app: &mut App, area: Rect, p: Palette) {
    let stage = match app.download_stage {
        DownloadStage::Input => "1 SOURCE",
        DownloadStage::Busy => "WORKING",
        DownloadStage::Results => "2 RESULTS",
        DownloadStage::Details => "3 DETAILS",
        DownloadStage::Options => "4 OPTIONS",
        DownloadStage::Downloading => "5 DOWNLOADING",
        DownloadStage::Done => "DONE",
        DownloadStage::Error => "NEEDS ATTENTION",
    };
    let block = Block::bordered()
        .title(format!(" SoundSnatch • {stage} "))
        .border_style(Style::default().fg(p.accent));
    match app.download_stage {
        DownloadStage::Input => frame.render_widget(
            Paragraph::new(vec![
                Line::raw("Paste a YouTube/YouTube Music URL or enter a title/artist:"),
                Line::raw(""),
                Line::styled(
                    format!("> {}_", app.download_input),
                    Style::default().fg(p.accent),
                ),
                Line::raw(""),
                Line::styled(
                    "Enter continues • yt-dlp, ffmpeg and node are external dependencies",
                    Style::default().fg(p.muted),
                ),
            ])
            .block(block),
            area,
        ),
        DownloadStage::Busy => frame.render_widget(
            Paragraph::new(format!("\n  {}", app.download_message)).block(block),
            area,
        ),
        DownloadStage::Results => {
            let items = app.download_results.iter().enumerate().map(|(i, r)| {
                ListItem::new(format!(
                    "{} — {}",
                    r.title,
                    r.artist
                        .as_deref()
                        .or(r.uploader.as_deref())
                        .unwrap_or("Unknown")
                ))
                .style(if i == app.download_selected {
                    Style::default().bg(p.selected).bold()
                } else {
                    Style::default()
                })
            });
            frame.render_widget(List::new(items).block(block), area);
        }
        DownloadStage::Details => {
            let m = app.download_meta.as_ref();
            frame.render_widget(
                Paragraph::new(vec![
                    Line::styled(
                        m.map(|m| m.title.as_str()).unwrap_or("Unknown"),
                        Style::default().bold(),
                    ),
                    Line::raw(
                        m.and_then(|m| m.artist.as_deref())
                            .unwrap_or("Unknown artist"),
                    ),
                    Line::raw(format!("Output name: {}", app.download_name)),
                    Line::raw(""),
                    Line::styled("Enter chooses output options", Style::default().fg(p.muted)),
                ])
                .block(block),
                area,
            );
        }
        DownloadStage::Options => frame.render_widget(
            Paragraph::new(format!(
                "Destination: {}\nFormat: {}\n\n1 MP3  •  2 FLAC  •  3 WAV  •  d change destination\nEnter starts download",
                app.download_settings.last_save_dir.display(),
                app.download_settings.default_format.as_str()
            ))
            .block(block),
            area,
        ),
        DownloadStage::Downloading => {
            let percent = app
                .download_progress
                .as_ref()
                .and_then(|x| x.percent)
                .unwrap_or(0.0);
            frame.render_widget(
                Gauge::default()
                    .block(block)
                    .gauge_style(Style::default().fg(p.accent).bg(p.surface))
                    .ratio((percent / 100.0).clamp(0.0, 1.0))
                    .label(format!("{percent:.1}%")),
                area,
            );
        }
        DownloadStage::Done => frame.render_widget(
            Paragraph::new(format!(
                "Download complete\n\n{}\n\nEnter starts another",
                app.download_message
            ))
            .style(Style::default().fg(p.secondary))
            .block(block),
            area,
        ),
        DownloadStage::Error => frame.render_widget(
            Paragraph::new(format!(
                "{}\n\nEnter returns to source",
                app.download_message
            ))
            .style(Style::default().fg(p.error))
            .block(block),
            area,
        ),
    }
}

fn draw_settings(frame: &mut Frame, app: &mut App, area: Rect, p: Palette) {
    let audio = app.audio.as_ref();
    let sleep = app
        .sleep_deadline
        .map(|d| {
            format!(
                "{} min remaining",
                d.saturating_duration_since(std::time::Instant::now())
                    .as_secs()
                    / 60
            )
        })
        .unwrap_or_else(|| "Off".into());
    let mut lines = vec![
        Line::styled("Appearance", Style::default().fg(p.accent).bold()),
        Line::raw(format!(
            "  Theme: {}  •  Density: {:?}",
            app.config.theme_name, app.config.density
        )),
        Line::raw(""),
        Line::styled("Library folders", Style::default().fg(p.accent).bold()),
    ];
    for dir in &app.config.music_dirs {
        lines.push(Line::raw(format!("  {dir}")));
    }
    if app.config.music_dirs.is_empty() {
        lines.push(Line::styled(
            "  None configured — edit ~/.config/dopamine/config.toml",
            Style::default().fg(p.error),
        ));
    }
    lines.extend([
        Line::raw(""),
        Line::styled("Playback", Style::default().fg(p.accent).bold()),
        Line::raw(format!(
            "  Speed: {:.1}×  •  Volume: {:.0}%  •  Sleep timer: {sleep}",
            audio.map_or(1.0, |a| a.playback_speed()),
            audio.map_or(0.0, |a| a.volume() * 100.0)
        )),
        Line::raw(format!(
            "  Visualizer: {}  •  Reduce motion: {}",
            on_off(app.config.visualizer_enabled),
            on_off(app.config.reduce_motion)
        )),
        Line::raw(""),
        Line::styled("Integrations", Style::default().fg(p.accent).bold()),
        Line::raw(format!(
            "  Last.fm: {}  •  SoundSnatch format: {}",
            on_off(app.config.lastfm.enabled),
            app.download_settings.default_format.as_str()
        )),
        Line::raw(""),
        Line::styled("Terminal", Style::default().fg(p.accent).bold()),
        Line::raw("  Keyboard complete • Mouse enabled • Unicode block artwork fallback"),
        Line::raw(""),
        Line::styled("Controls", Style::default().fg(p.accent).bold()),
        Line::raw("  a add folder  •  t theme  •  v visualizer  •  R reduce motion"),
        Line::raw("  ,/. playback speed  •  y sleep timer (15/30/off)"),
    ]);
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::bordered().title(" Settings • edit config.toml for text values "))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_player(frame: &mut Frame, app: &mut App, area: Rect, p: Palette) {
    let current = app.queue.current();
    let duration = current.map_or(0, |t| t.duration_secs.max(0));
    let ratio = if duration == 0 {
        0.0
    } else {
        (app.position.as_secs_f64() / duration as f64).clamp(0.0, 1.0)
    };
    let rows = Layout::vertical([
        Constraint::Length(area.height.saturating_sub(1)),
        Constraint::Length(1),
    ])
    .split(area);
    let mut info = current
        .map(|t| format!("{} — {}", t.title, t.artist))
        .unwrap_or_else(|| "Nothing playing".into());
    if area.width >= 110
        && app.config.visualizer_enabled
        && !app.config.reduce_motion
        && let Some(audio) = app.audio.as_ref()
    {
        use std::sync::atomic::Ordering;
        let latest = audio.index.load(Ordering::Relaxed);
        let bars = ["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
        let visualizer = (0..18)
            .map(|offset| {
                let index = latest.wrapping_sub(offset * 24 + 1) % audio.samples.len();
                let sample = audio.samples[index].load(Ordering::Relaxed).unsigned_abs();
                bars[((sample as usize * bars.len()) / 1_000_001).min(bars.len() - 1)]
            })
            .collect::<String>();
        info.push_str(&format!("\n{visualizer}"));
    }
    let repeat = match app.queue.repeat_mode {
        RepeatMode::None => "off",
        RepeatMode::All => "all",
        RepeatMode::One => "one",
    };
    let state = if app.playing { "Ⅱ" } else { "▶" };
    let cols = Layout::horizontal([
        Constraint::Percentage(34),
        Constraint::Percentage(32),
        Constraint::Percentage(34),
    ])
    .split(rows[0]);
    frame.render_widget(
        Paragraph::new(info)
            .style(Style::default().fg(p.text).bg(p.surface))
            .block(Block::default().borders(Borders::TOP)),
        cols[0],
    );
    let controls = format!("  ◀   {state}   ▶  ");
    frame.render_widget(
        Paragraph::new(controls)
            .alignment(Alignment::Center)
            .style(Style::default().fg(p.accent).bg(p.surface).bold())
            .block(Block::default().borders(Borders::TOP)),
        cols[1],
    );
    let volume = app.audio.as_ref().map_or(0.0, |a| a.volume() * 100.0);
    frame.render_widget(
        Paragraph::new(format!(
            "Vol {volume:.0}%  Shuffle {}  Repeat {repeat}",
            on_off(app.queue.shuffle)
        ))
        .alignment(Alignment::Right)
        .style(Style::default().fg(p.muted).bg(p.surface))
        .block(Block::default().borders(Borders::TOP)),
        cols[2],
    );
    frame.render_widget(
        Gauge::default()
            .gauge_style(Style::default().fg(p.accent).bg(p.surface))
            .ratio(ratio)
            .label(format!(
                "{} / {}",
                format_duration(app.position.as_secs() as i64),
                format_duration(duration)
            )),
        rows[1],
    );
    app.hits.push((cols[0], Hit::Lyrics));
    app.hits.push((cols[1], Hit::PlayPause));
    app.hits.push((
        Rect::new(cols[1].x, cols[1].y, cols[1].width / 3, cols[1].height),
        Hit::Previous,
    ));
    app.hits.push((
        Rect::new(
            cols[1].x + cols[1].width * 2 / 3,
            cols[1].y,
            cols[1].width / 3,
            cols[1].height,
        ),
        Hit::Next,
    ));
    app.hits.push((cols[2], Hit::Queue));
}

fn draw_footer(frame: &mut Frame, app: &mut App, area: Rect, p: Palette) {
    let text = if !app.status.is_empty() {
        app.status.clone()
    } else {
        match app.view {
            View::Queue => "↑↓ select  J/K move  Del remove  a actions  ? help",
            View::Lyrics => "[ ] sync lyrics  / search  Esc back  ? help",
            _ => "↑↓ navigate  Enter open/play  a actions  Space pause  ? help",
        }
        .into()
    };
    frame.render_widget(
        Paragraph::new(format!(" {text} ")).style(
            Style::default()
                .fg(if app.status_error { p.error } else { p.muted })
                .bg(p.surface),
        ),
        area,
    );
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width.saturating_sub(4));
    let h = h.min(area.height.saturating_sub(2));
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}
fn draw_overlay(frame: &mut Frame, app: &mut App, area: Rect, p: Palette) {
    let (rect, title, body, selected) = match &app.overlay {
        Overlay::None => return,
        Overlay::Help => (centered(area, 72, 20), " Help ", help_text(), None),
        Overlay::ConfirmQuit => (
            centered(area, 44, 7),
            " Quit Dopamine? ",
            Text::from(vec![
                Line::raw("Playback will stop and state will be saved."),
                Line::raw(""),
                Line::styled(
                    "Enter/y confirm  •  Esc/n cancel",
                    Style::default().fg(p.muted),
                ),
            ]),
            None,
        ),
        Overlay::ConfirmDeletePlaylist(name) => (
            centered(area, 50, 7),
            " Delete playlist? ",
            Text::from(vec![
                Line::raw(name),
                Line::raw("Tracks remain in your library."),
                Line::styled(
                    "Enter/y confirm  •  Esc/n cancel",
                    Style::default().fg(p.muted),
                ),
            ]),
            None,
        ),
        Overlay::Search => (
            centered(area, 60, 5),
            " Search current view ",
            Text::from(Line::styled(
                format!("> {}_", app.query),
                Style::default().fg(p.accent),
            )),
            None,
        ),
        Overlay::NewPlaylist(name) => (
            centered(area, 60, 5),
            " New playlist ",
            Text::from(Line::styled(
                format!("> {name}_"),
                Style::default().fg(p.accent),
            )),
            None,
        ),
        Overlay::AddFolder(path) => (
            centered(area, 70, 5),
            " Add music folder ",
            Text::from(Line::styled(
                format!("> {path}_"),
                Style::default().fg(p.accent),
            )),
            None,
        ),
        Overlay::DownloadDestination(path) => (
            centered(area, 70, 5),
            " Download destination ",
            Text::from(Line::styled(
                format!("> {path}_"),
                Style::default().fg(p.accent),
            )),
            None,
        ),
        Overlay::Command { query, selected } => (
            centered(area, 64, 20),
            " Command palette • Ctrl+P ",
            command_text(query, *selected, p),
            Some(*selected),
        ),
        Overlay::Actions { selected } => (
            centered(area, 52, 14),
            " Actions ",
            list_text(&app.context_actions(), *selected, p),
            Some(*selected),
        ),
        Overlay::PlaylistPicker { selected, .. } => {
            let names = app
                .playlists
                .iter()
                .map(|x| x.name.as_str())
                .collect::<Vec<_>>();
            (
                centered(area, 56, 16),
                " Add to playlist ",
                list_text(&names, *selected, p),
                Some(*selected),
            )
        }
        Overlay::Metadata { track, field, year } => (
            centered(area, 70, 16),
            " Edit metadata ",
            metadata_text(track, *field, year, p),
            Some(*field),
        ),
    };
    let _ = selected;
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(body)
            .block(
                Block::bordered()
                    .title(title)
                    .border_style(Style::default().fg(p.accent)),
            )
            .style(Style::default().fg(p.text).bg(p.bg))
            .wrap(Wrap { trim: false }),
        rect,
    );
}

fn command_text(query: &str, selected: usize, p: Palette) -> Text<'static> {
    let mut lines = vec![
        Line::styled(format!("> {query}_"), Style::default().fg(p.accent)),
        Line::raw(""),
    ];
    for (i, (name, _)) in filtered_commands(query).iter().take(14).enumerate() {
        lines.push(Line::styled(
            format!("  {name}"),
            if i == selected {
                Style::default().bg(p.selected).bold()
            } else {
                Style::default()
            },
        ));
    }
    Text::from(lines)
}
fn list_text(items: &[&str], selected: usize, p: Palette) -> Text<'static> {
    Text::from(
        items
            .iter()
            .enumerate()
            .map(|(i, v)| {
                Line::styled(
                    format!("  {v}"),
                    if i == selected {
                        Style::default().bg(p.selected).bold()
                    } else {
                        Style::default()
                    },
                )
            })
            .collect::<Vec<_>>(),
    )
}
fn metadata_text(
    track: &crate::models::Track,
    field: usize,
    year: &str,
    p: Palette,
) -> Text<'static> {
    let values = [
        track.title.clone(),
        track.artist.clone(),
        track.album.clone(),
        track.genre.clone(),
        year.to_string(),
    ];
    let names = ["Title", "Artist", "Album", "Genre", "Year"];
    let mut lines = vec![
        Line::styled(
            "Tab changes field • Enter saves • Esc cancels",
            Style::default().fg(p.muted),
        ),
        Line::raw(""),
    ];
    for (i, (name, value)) in names.into_iter().zip(values).enumerate() {
        lines.push(Line::styled(
            format!("{name:>8}: {value}{}", if i == field { "_" } else { "" }),
            if i == field {
                Style::default().fg(p.accent).bg(p.selected)
            } else {
                Style::default()
            },
        ));
        lines.push(Line::raw(""));
    }
    Text::from(lines)
}
fn help_text() -> Text<'static> {
    Text::from(vec![
        Line::styled("Navigation", Style::default().bold()),
        Line::raw("  ↑/↓ or j/k move   Enter open/play   Esc back   / search"),
        Line::raw("  Ctrl+P commands   a actions   q quit"),
        Line::raw(""),
        Line::styled("Playback", Style::default().bold()),
        Line::raw("  Space play/pause   x stop   p/n previous/next   </> seek 10s"),
        Line::raw("  -/+ volume   m mute   s shuffle   r repeat"),
        Line::raw(""),
        Line::styled("Context", Style::default().bold()),
        Line::raw("  f favorite   S scan   Queue: J/K move, Del remove"),
        Line::raw("  Lyrics: [/] offset -/+500ms"),
        Line::raw(""),
        Line::raw("Every destination and less common feature is available from Ctrl+P."),
        Line::raw("Press any key to close help."),
    ])
}
fn empty(message: &str, p: Palette) -> Paragraph<'static> {
    Paragraph::new(message.to_string())
        .alignment(Alignment::Center)
        .style(Style::default().fg(p.muted))
        .block(Block::bordered())
}
fn on_off(v: bool) -> &'static str {
    if v { "On" } else { "Off" }
}
fn format_duration(seconds: i64) -> String {
    let s = seconds.max(0);
    format!("{}:{:02}", s / 60, s % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn visible_window_keeps_selection_centered() {
        assert_eq!(visible_window(100, 50, 10), (45, 55));
        assert_eq!(visible_window(3, 2, 10), (0, 3));
    }
    #[test]
    fn lrc_parser_handles_synced_and_plain() {
        let got = parse_lyrics("[01:02.50]Hello\nWorld");
        assert_eq!(got[0], (Some(62_500), "Hello".into()));
        assert_eq!(got[1].1, "World");
    }
}
