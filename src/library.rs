use crate::models::Track;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::probe::Probe;
use lofty::tag::Accessor;
use walkdir::WalkDir;

pub fn scan_library(dir: &str) -> Vec<Track> {
    let mut tracks = Vec::new();

    for entry in WalkDir::new(dir).into_iter().filter_map(Result::ok) {
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
                            }

                            if let Ok(abs_path) = std::fs::canonicalize(path) {
                                tracks.push(Track {
                                    path: abs_path.to_string_lossy().to_string(),
                                    title,
                                    artist,
                                    album,
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
