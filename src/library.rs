use crate::models::Track;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::config::WriteOptions;
use lofty::probe::Probe;
use lofty::tag::{Accessor, Tag, ItemKey};
use walkdir::WalkDir;

pub fn save_metadata(track: &Track) -> anyhow::Result<()> {
    let path = std::path::Path::new(&track.path);
    let mut tagged_file = Probe::open(path)?.read()?;
    
    let tag = match tagged_file.primary_tag_mut() {
        Some(t) => t,
        None => {
            if let Some(t) = tagged_file.first_tag_mut() {
                t
            } else {
                let tag_type = tagged_file.primary_tag_type();
                tagged_file.insert_tag(Tag::new(tag_type));
                tagged_file.primary_tag_mut().unwrap()
            }
        }
    };

    tag.set_title(track.title.clone());
    tag.set_artist(track.artist.clone());
    tag.set_album(track.album.clone());
    tag.set_genre(track.genre.clone());
    if track.year > 0 {
        tag.insert_text(ItemKey::Year, track.year.to_string());
    }

    tagged_file.save_to_path(path, WriteOptions::default())?;
    Ok(())
}

pub fn scan_library<F>(dir: &str, mut progress: F) -> Vec<Track> 
where F: FnMut(usize, usize) {
    let mut tracks = Vec::new();
    let entries: Vec<_> = WalkDir::new(dir).into_iter().filter_map(Result::ok).collect();
    let total = entries.len();

    for (i, entry) in entries.into_iter().enumerate() {
        progress(i + 1, total);
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if matches!(ext.to_lowercase().as_str(), "mp3" | "flac" | "wav" | "m4a" | "ogg") {
                    if let Ok(probe) = Probe::open(path) {
                        if let Ok(tagged_file) = probe.read() {
                            let properties = tagged_file.properties();
                            let duration = properties.duration().as_secs() as i64;
                            
                            let mut title = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                            let mut artist = "Unknown Artist".to_string();
                            let mut album = "Unknown Album".to_string();
                            let mut genre = "Unknown".to_string();
                            let mut year = 0;

                            if let Some(tag) = tagged_file.primary_tag().or_else(|| tagged_file.first_tag()) {
                                if let Some(t) = tag.title().as_deref() {
                                    title = t.to_string();
                                }
                                if let Some(a) = tag.artist().as_deref() {
                                    artist = a.to_string();
                                }
                                if let Some(al) = tag.album().as_deref() {
                                    album = al.to_string();
                                }
                                if let Some(g) = tag.genre().as_deref() {
                                    genre = g.to_string();
                                }
                                if let Some(item) = tag.get(ItemKey::Year) {
                                    if let Some(y) = item.value().text() {
                                        year = y.parse::<i32>().unwrap_or(0);
                                    }
                                }
                            }

                            // Look for lyrics (.lrc file)
                            let mut lyrics = None;
                            let lrc_path = path.with_extension("lrc");
                            if lrc_path.exists() {
                                if let Ok(content) = std::fs::read_to_string(lrc_path) {
                                    lyrics = Some(content);
                                }
                            }

                            if let Ok(abs_path) = std::fs::canonicalize(path) {
                                tracks.push(Track {
                                    path: abs_path.to_string_lossy().to_string(),
                                    title,
                                    artist,
                                    album,
                                    genre,
                                    year,
                                    favorite: false,
                                    play_count: 0,
                                    last_played: None,
                                    lyrics,
                                    duration_secs: duration,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    tracks
}
