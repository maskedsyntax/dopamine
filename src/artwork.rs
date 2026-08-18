//! UI-neutral loading and fallback metadata for album artwork.
//!
//! [`ArtworkService::load`] performs filesystem and tag I/O and should be called
//! off the render thread. [`ArtworkService::load_async`] is a convenience for
//! callers that do not already own a worker pool.

use lofty::file::TaggedFileExt;
use lofty::picture::{Picture, PictureType};
use lofty::probe::Probe;
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::time::SystemTime;

const SIDECAR_STEMS: &[&str] = &["cover", "folder", "front"];
const SIDECAR_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp", "tif", "tiff"];

/// Text used to produce deterministic artwork when no image exists.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ArtworkMetadata {
    pub title: String,
    pub artist: String,
    pub album: String,
}

/// Where image bytes were found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArtworkSource {
    Embedded,
    Sidecar(PathBuf),
}

/// Encoded image bytes. Decoding and resizing are deliberately left to the UI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtworkData {
    pub bytes: Arc<[u8]>,
    pub mime_type: String,
    pub source: ArtworkSource,
}

/// Renderer-independent values for drawing a generated cover.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedFallback {
    pub background_rgb: [u8; 3],
    pub foreground_rgb: [u8; 3],
    pub initials: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Artwork {
    Data(ArtworkData),
    Generated(GeneratedFallback),
}

/// Cache identity for an audio file.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ArtworkCacheKey {
    pub canonical_path: PathBuf,
    pub modified: Option<SystemTime>,
}

#[derive(Debug)]
pub enum ArtworkError {
    Io { path: PathBuf, source: io::Error },
    WorkerDisconnected,
}

impl fmt::Display for ArtworkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(
                    formatter,
                    "artwork I/O failed for {}: {source}",
                    path.display()
                )
            }
            Self::WorkerDisconnected => formatter.write_str("artwork worker disconnected"),
        }
    }
}

impl std::error::Error for ArtworkError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::WorkerDisconnected => None,
        }
    }
}

pub type ArtworkResult = Result<Arc<Artwork>, ArtworkError>;

/// A pending asynchronous load. Poll with [`ArtworkRequest::try_recv`] from a
/// render loop, or consume it with [`ArtworkRequest::recv`] on a non-UI thread.
pub struct ArtworkRequest {
    receiver: mpsc::Receiver<ArtworkResult>,
}

impl ArtworkRequest {
    pub fn try_recv(&self) -> Result<Option<ArtworkResult>, ArtworkError> {
        match self.receiver.try_recv() {
            Ok(result) => Ok(Some(result)),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err(ArtworkError::WorkerDisconnected),
        }
    }

    pub fn recv(self) -> ArtworkResult {
        self.receiver
            .recv()
            .unwrap_or(Err(ArtworkError::WorkerDisconnected))
    }
}

#[derive(Clone)]
pub struct ArtworkService {
    inner: Arc<ServiceInner>,
}

struct ServiceInner {
    capacity: usize,
    cache: Mutex<CacheState>,
}

#[derive(Default)]
struct CacheState {
    clock: u64,
    entries: HashMap<ArtworkCacheKey, CacheEntry>,
}

struct CacheEntry {
    // None is a cached confirmation that neither embedded nor sidecar art exists.
    data: Option<Arc<ArtworkData>>,
    last_used: u64,
}

impl ArtworkService {
    /// Creates a service retaining at most `capacity` file lookup results.
    /// A capacity of zero disables caching.
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Arc::new(ServiceInner {
                capacity,
                cache: Mutex::new(CacheState::default()),
            }),
        }
    }

    /// Loads artwork synchronously. This function performs blocking I/O.
    pub fn load(&self, audio_path: impl AsRef<Path>, metadata: &ArtworkMetadata) -> ArtworkResult {
        let audio_path = audio_path.as_ref();
        let key = cache_key(audio_path)?;

        if let Some(data) = self.cached(&key) {
            return Ok(to_artwork(data, metadata));
        }

        let data = load_image(&key.canonical_path)?.map(Arc::new);
        self.insert(key, data.clone());
        Ok(to_artwork(data, metadata))
    }

    /// Spawns a loader thread and immediately returns a pollable request.
    pub fn load_async(
        &self,
        audio_path: impl Into<PathBuf>,
        metadata: ArtworkMetadata,
    ) -> ArtworkRequest {
        let service = self.clone();
        let audio_path = audio_path.into();
        let (sender, receiver) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let _ = sender.send(service.load(audio_path, &metadata));
        });
        ArtworkRequest { receiver }
    }

    fn cached(&self, key: &ArtworkCacheKey) -> Option<Option<Arc<ArtworkData>>> {
        let mut cache = self
            .inner
            .cache
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        cache.clock = cache.clock.wrapping_add(1);
        let now = cache.clock;
        let entry = cache.entries.get_mut(key)?;
        entry.last_used = now;
        Some(entry.data.clone())
    }

    fn insert(&self, key: ArtworkCacheKey, data: Option<Arc<ArtworkData>>) {
        if self.inner.capacity == 0 {
            return;
        }

        let mut cache = self
            .inner
            .cache
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        cache.clock = cache.clock.wrapping_add(1);
        let now = cache.clock;

        // A changed modification time supersedes older entries for this path.
        cache.entries.retain(|existing, _| {
            existing.canonical_path != key.canonical_path || existing == &key
        });
        cache.entries.insert(
            key,
            CacheEntry {
                data,
                last_used: now,
            },
        );

        while cache.entries.len() > self.inner.capacity {
            let oldest = cache
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, _)| key.clone());
            if let Some(oldest) = oldest {
                cache.entries.remove(&oldest);
            }
        }
    }
}

impl Default for ArtworkService {
    fn default() -> Self {
        Self::new(128)
    }
}

/// Builds the canonical-path and modification-time cache identity.
pub fn cache_key(audio_path: impl AsRef<Path>) -> Result<ArtworkCacheKey, ArtworkError> {
    let input = audio_path.as_ref();
    let canonical_path = fs::canonicalize(input).map_err(|source| ArtworkError::Io {
        path: input.to_path_buf(),
        source,
    })?;
    let metadata = fs::metadata(&canonical_path).map_err(|source| ArtworkError::Io {
        path: canonical_path.clone(),
        source,
    })?;

    Ok(ArtworkCacheKey {
        canonical_path,
        modified: metadata.modified().ok(),
    })
}

/// Finds a sidecar without reading it. Matching is case-insensitive and the
/// order is cover, folder, front, then the extension order documented above.
pub fn discover_sidecar(audio_path: impl AsRef<Path>) -> Result<Option<PathBuf>, ArtworkError> {
    let audio_path = audio_path.as_ref();
    let directory = audio_path.parent().unwrap_or_else(|| Path::new("."));
    let entries = fs::read_dir(directory).map_err(|source| ArtworkError::Io {
        path: directory.to_path_buf(),
        source,
    })?;
    let mut matches = Vec::new();

    for entry in entries {
        let entry = entry.map_err(|source| ArtworkError::Io {
            path: directory.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
            continue;
        };
        let stem = stem.to_ascii_lowercase();
        let extension = extension.to_ascii_lowercase();
        let Some(stem_rank) = SIDECAR_STEMS
            .iter()
            .position(|candidate| *candidate == stem)
        else {
            continue;
        };
        let Some(extension_rank) = SIDECAR_EXTENSIONS
            .iter()
            .position(|candidate| *candidate == extension)
        else {
            continue;
        };
        matches.push((stem_rank, extension_rank, path));
    }

    matches.sort_by(|left, right| {
        (left.0, left.1)
            .cmp(&(right.0, right.1))
            .then_with(|| left.2.cmp(&right.2))
    });
    Ok(matches.into_iter().next().map(|(_, _, path)| path))
}

/// Produces the same fallback for the same metadata on every process and platform.
pub fn generated_fallback(metadata: &ArtworkMetadata) -> GeneratedFallback {
    let identity = [&metadata.album, &metadata.artist, &metadata.title]
        .into_iter()
        .find(|value| !value.trim().is_empty())
        .map_or("?", |value| value.trim());
    let hash = identity
        .as_bytes()
        .iter()
        .fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        });

    // Keep saturation and lightness in a range that works as a cover background.
    let hue = (hash % 360) as f32;
    let saturation = 0.48 + (((hash >> 9) & 0xff) as f32 / 255.0) * 0.24;
    let lightness = 0.30 + (((hash >> 17) & 0xff) as f32 / 255.0) * 0.22;
    let background_rgb = hsl_to_rgb(hue, saturation, lightness);
    let luminance = (u32::from(background_rgb[0]) * 299
        + u32::from(background_rgb[1]) * 587
        + u32::from(background_rgb[2]) * 114)
        / 1000;

    GeneratedFallback {
        background_rgb,
        foreground_rgb: if luminance > 145 {
            [24, 24, 24]
        } else {
            [248, 248, 248]
        },
        initials: initials(identity),
    }
}

fn load_image(audio_path: &Path) -> Result<Option<ArtworkData>, ArtworkError> {
    if let Ok(tagged_file) = Probe::open(audio_path).and_then(|probe| probe.read()) {
        let picture = tagged_file
            .primary_tag()
            .and_then(|tag| tag.get_picture_type(PictureType::CoverFront))
            .or_else(|| {
                tagged_file
                    .tags()
                    .iter()
                    .find_map(|tag| tag.get_picture_type(PictureType::CoverFront))
            });
        if let Some(picture) = picture {
            return Ok(Some(artwork_from_picture(picture)));
        }
    }

    let Some(path) = discover_sidecar(audio_path)? else {
        return Ok(None);
    };
    let bytes = fs::read(&path).map_err(|source| ArtworkError::Io {
        path: path.clone(),
        source,
    })?;
    Ok(Some(ArtworkData {
        mime_type: mime_from_path(&path).to_owned(),
        bytes: bytes.into(),
        source: ArtworkSource::Sidecar(path),
    }))
}

fn artwork_from_picture(picture: &Picture) -> ArtworkData {
    ArtworkData {
        bytes: Arc::from(picture.data()),
        mime_type: picture
            .mime_type()
            .map(|mime| mime.as_str())
            .unwrap_or_else(|| mime_from_bytes(picture.data()))
            .to_owned(),
        source: ArtworkSource::Embedded,
    }
}

fn to_artwork(data: Option<Arc<ArtworkData>>, metadata: &ArtworkMetadata) -> Arc<Artwork> {
    match data {
        Some(data) => Arc::new(Artwork::Data((*data).clone())),
        None => Arc::new(Artwork::Generated(generated_fallback(metadata))),
    }
}

fn mime_from_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        _ => "application/octet-stream",
    }
}

fn mime_from_bytes(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        "image/jpeg"
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        "image/gif"
    } else if bytes.starts_with(b"BM") {
        "image/bmp"
    } else if bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*") {
        "image/tiff"
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else {
        "application/octet-stream"
    }
}

fn initials(value: &str) -> String {
    let words: Vec<_> = value
        .split_whitespace()
        .filter(|word| !word.is_empty())
        .collect();
    match words.as_slice() {
        [] => "?".to_owned(),
        [word] => word.chars().take(2).flat_map(char::to_uppercase).collect(),
        words => [words[0], words[words.len() - 1]]
            .into_iter()
            .filter_map(|word| word.chars().next())
            .flat_map(char::to_uppercase)
            .collect(),
    }
}

fn hsl_to_rgb(hue: f32, saturation: f32, lightness: f32) -> [u8; 3] {
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let segment = hue / 60.0;
    let x = chroma * (1.0 - (segment.rem_euclid(2.0) - 1.0).abs());
    let (red, green, blue) = match segment as u8 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let offset = lightness - chroma / 2.0;
    [red, green, blue].map(|channel| ((channel + offset) * 255.0).round() as u8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "dopamine-artwork-{}-{sequence}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn sidecar_discovery_obeys_name_and_extension_priority() {
        let directory = TestDirectory::new();
        let audio = directory.0.join("track.mp3");
        fs::write(&audio, b"not audio").unwrap();
        fs::write(directory.0.join("front.png"), b"front").unwrap();
        fs::write(directory.0.join("Folder.jpg"), b"folder").unwrap();
        fs::write(directory.0.join("cover.webp"), b"cover").unwrap();
        fs::write(directory.0.join("unrelated.jpg"), b"ignore").unwrap();

        assert_eq!(
            discover_sidecar(&audio).unwrap(),
            Some(directory.0.join("cover.webp"))
        );
    }

    #[test]
    fn fallback_is_deterministic_and_uses_album_initials() {
        let metadata = ArtworkMetadata {
            title: "Track".to_owned(),
            artist: "An Artist".to_owned(),
            album: "Kind of Blue".to_owned(),
        };

        let first = generated_fallback(&metadata);
        assert_eq!(first, generated_fallback(&metadata));
        assert_eq!(first.initials, "KB");

        let mut other = metadata;
        other.album = "Blue Train".to_owned();
        assert_ne!(
            first.background_rgb,
            generated_fallback(&other).background_rgb
        );
    }

    #[test]
    fn cache_key_is_canonical_and_tracks_modification_time() {
        let directory = TestDirectory::new();
        let audio = directory.0.join("track.mp3");
        fs::write(&audio, b"one").unwrap();

        let first = cache_key(directory.0.join(".").join("track.mp3")).unwrap();
        assert_eq!(first.canonical_path, fs::canonicalize(&audio).unwrap());
        assert_eq!(first, cache_key(&audio).unwrap());

        std::thread::sleep(Duration::from_millis(20));
        fs::write(&audio, b"different length").unwrap();
        let second = cache_key(&audio).unwrap();
        assert_eq!(first.canonical_path, second.canonical_path);
        assert_ne!(first.modified, second.modified);
    }

    #[test]
    fn load_returns_sidecar_bytes_and_mime() {
        let directory = TestDirectory::new();
        let audio = directory.0.join("track.mp3");
        fs::write(&audio, b"not audio").unwrap();
        fs::write(directory.0.join("cover.png"), b"png bytes").unwrap();

        let result = ArtworkService::new(2)
            .load(&audio, &ArtworkMetadata::default())
            .unwrap();
        let Artwork::Data(data) = result.as_ref() else {
            panic!("expected sidecar artwork");
        };
        assert_eq!(data.bytes.as_ref(), b"png bytes");
        assert_eq!(data.mime_type, "image/png");
    }
}
