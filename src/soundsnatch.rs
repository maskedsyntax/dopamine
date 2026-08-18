//! UI-neutral, shell-free orchestration for SoundSnatch's yt-dlp operations.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::env;
use std::error::Error as StdError;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

const BEST_AUDIO_SELECTOR: &str = "bestaudio[ext=m4a]/bestaudio[ext=webm]/bestaudio/best";
const SINGLE_OUTPUT_SUFFIX: &str = ".%(ext)s";
const PLAYLIST_OUTPUT_TEMPLATE: &str = "%(title)s.%(ext)s";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioFormat {
    #[default]
    Mp3,
    Flac,
    Wav,
}

impl AudioFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mp3 => "mp3",
            Self::Flac => "flac",
            Self::Wav => "wav",
        }
    }
}

impl fmt::Display for AudioFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for AudioFormat {
    type Err = SoundSnatchError;

    fn from_str(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "mp3" => Ok(Self::Mp3),
            "flac" => Ok(Self::Flac),
            "wav" => Ok(Self::Wav),
            other => Err(SoundSnatchError::Settings(format!(
                "unsupported audio format: {other}"
            ))),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadRequest {
    pub url: String,
    pub save_dir: PathBuf,
    pub save_name: String,
    pub format: AudioFormat,
    pub archive_path: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SoundSnatchSettings {
    pub last_save_dir: PathBuf,
    pub default_format: AudioFormat,
    pub archive_path: PathBuf,
}

impl Default for SoundSnatchSettings {
    fn default() -> Self {
        Self {
            last_save_dir: dirs::audio_dir().unwrap_or_else(|| PathBuf::from(".")),
            default_format: AudioFormat::Mp3,
            archive_path: dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".soundsnatch_archive.txt"),
        }
    }
}

impl SoundSnatchSettings {
    pub fn path() -> Result<PathBuf> {
        dirs::home_dir()
            .map(|home| home.join(".soundsnatch.yaml"))
            .ok_or_else(|| SoundSnatchError::Settings("home directory is unavailable".into()))
    }

    pub fn load() -> Result<Self> {
        Self::load_from(&Self::path()?)
    }

    pub fn save(&self) -> Result<()> {
        self.save_to(&Self::path()?)
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        let contents = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => return Err(error.into()),
        };
        parse_settings_yaml(&contents)
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, settings_yaml(self))?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum YtDlpCommand {
    Binary(PathBuf),
    PythonModule(PathBuf),
}

impl YtDlpCommand {
    pub fn program(&self) -> &Path {
        match self {
            Self::Binary(path) | Self::PythonModule(path) => path,
        }
    }

    pub fn prefix_args(&self) -> &'static [&'static str] {
        match self {
            Self::Binary(_) => &[],
            Self::PythonModule(_) => &["-m", "yt_dlp"],
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MediaMetadata {
    pub id: String,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub uploader: Option<String>,
    pub webpage_url: Option<String>,
    pub duration: Option<f64>,
    pub playlist: bool,
    pub entry_count: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SearchResult {
    pub id: String,
    pub title: String,
    pub artist: Option<String>,
    pub uploader: Option<String>,
    pub url: String,
    pub duration: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputStream {
    Stdout,
    Stderr,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DownloadProgress {
    /// Download completion normalized to `0.0..=1.0`.
    pub percent: Option<f64>,
    pub current_item: Option<usize>,
    pub total_items: Option<usize>,
    pub speed: Option<String>,
    pub eta: Option<String>,
    pub raw: String,
    pub stream: OutputStream,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadResult {
    pub output_path: PathBuf,
    pub output_files: Vec<PathBuf>,
    pub playlist: bool,
    pub warning: Option<String>,
}

#[derive(Debug)]
pub enum SoundSnatchError {
    NotInstalled,
    InvalidRequest(String),
    InvalidMetadata(String),
    NoOutput(String),
    Settings(String),
    Io(std::io::Error),
    Json(serde_json::Error),
    Process { code: Option<i32>, message: String },
    MissingPipe(&'static str),
}

impl fmt::Display for SoundSnatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotInstalled => write!(
                f,
                "yt-dlp was not found (tried yt-dlp, then python3 -m yt_dlp)"
            ),
            Self::InvalidRequest(message) => write!(f, "invalid download request: {message}"),
            Self::InvalidMetadata(message) => write!(f, "invalid media metadata: {message}"),
            Self::NoOutput(message) => write!(f, "yt-dlp produced no audio files: {message}"),
            Self::Settings(message) => write!(f, "invalid SoundSnatch settings: {message}"),
            Self::Io(error) => write!(f, "I/O error: {error}"),
            Self::Json(error) => write!(f, "invalid yt-dlp JSON: {error}"),
            Self::Process { code, message } => write!(
                f,
                "yt-dlp failed with exit code {}: {message}",
                code.map_or_else(|| "unknown".into(), |code| code.to_string())
            ),
            Self::MissingPipe(pipe) => write!(f, "failed to capture yt-dlp {pipe}"),
        }
    }
}

impl StdError for SoundSnatchError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for SoundSnatchError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for SoundSnatchError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

pub type Result<T> = std::result::Result<T, SoundSnatchError>;

#[derive(Clone, Debug)]
pub struct SoundSnatch {
    command: YtDlpCommand,
}

impl SoundSnatch {
    pub async fn new() -> Result<Self> {
        Ok(Self {
            command: resolve_yt_dlp().await?,
        })
    }

    pub fn with_command(command: YtDlpCommand) -> Self {
        Self { command }
    }

    pub fn metadata_args(&self, url: &str) -> Vec<String> {
        metadata_args(url)
    }

    pub fn search_args(&self, query: &str) -> Vec<String> {
        search_args(query)
    }

    pub fn download_args(&self, request: &DownloadRequest) -> Result<Vec<String>> {
        download_args(request)
    }

    pub async fn fetch_metadata(&self, url: &str) -> Result<MediaMetadata> {
        let output = self.run_output(metadata_args(url)).await?;
        // yt-dlp --ignore-errors may return nonzero after printing usable metadata.
        match parse_metadata(output.stdout.trim()) {
            Ok(metadata) => return Ok(metadata),
            Err(error) if output.status.success() && output.stderr.trim().is_empty() => {
                return Err(error);
            }
            Err(_) => {}
        }
        Err(process_error(output.status, &output.stderr))
    }

    pub async fn search(&self, query: &str) -> Result<Vec<SearchResult>> {
        let output = self.run_output(search_args(query)).await?;
        let mut results = Vec::new();
        let mut parse_error = None;
        for line in output.stdout.lines().filter(|line| !line.trim().is_empty()) {
            match parse_search_result(line) {
                Ok(result) => results.push(result),
                Err(error) => {
                    parse_error.get_or_insert(error);
                }
            }
        }
        if !results.is_empty() {
            Ok(results)
        } else if !output.status.success() || !output.stderr.trim().is_empty() {
            Err(process_error(output.status, &output.stderr))
        } else if let Some(error) = parse_error {
            Err(error)
        } else {
            Ok(Vec::new())
        }
    }

    pub async fn download<F>(
        &self,
        request: DownloadRequest,
        mut callback: F,
    ) -> Result<DownloadResult>
    where
        F: FnMut(DownloadProgress) + Send,
    {
        validate_request(&request)?;
        let playlist = is_playlist_url(&request.url);
        let output_path = request_output_path(&request);
        if playlist {
            fs::create_dir_all(&output_path)?;
        } else {
            fs::create_dir_all(&request.save_dir)?;
            if let Some(archive) = request.archive_path.as_ref()
                && let Some(parent) = archive.parent().filter(|path| !path.as_os_str().is_empty())
            {
                fs::create_dir_all(parent)?;
            }
        }

        let mut command = self.make_command(download_args(&request)?);
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = command.spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or(SoundSnatchError::MissingPipe("stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or(SoundSnatchError::MissingPipe("stderr"))?;
        let (sender, mut receiver) = mpsc::unbounded_channel();
        let stdout_task = tokio::spawn(read_lines(stdout, OutputStream::Stdout, sender.clone()));
        let stderr_task = tokio::spawn(read_lines(stderr, OutputStream::Stderr, sender));
        let mut current_item = None;
        let mut total_items = None;
        let mut diagnostics = Vec::new();

        while let Some((stream, line)) = receiver.recv().await {
            if let Some((current, total)) = parse_playlist_position(&line) {
                current_item = Some(current);
                total_items = Some(total);
            }
            if let Some(mut progress) = parse_progress_line_from(&line, stream) {
                if progress.current_item.is_none() {
                    progress.current_item = current_item;
                    progress.total_items = total_items;
                }
                callback(progress);
            }
            if stream == OutputStream::Stderr && !line.trim().is_empty() {
                diagnostics.push(line);
            }
        }

        let status = child.wait().await?;
        stdout_task
            .await
            .map_err(|error| SoundSnatchError::Io(std::io::Error::other(error)))??;
        stderr_task
            .await
            .map_err(|error| SoundSnatchError::Io(std::io::Error::other(error)))??;

        let output_files = collect_output_files(&request, &output_path);
        if output_files.is_empty() {
            return Err(SoundSnatchError::NoOutput(diagnostic_message(
                &diagnostics.join("\n"),
            )));
        }
        // --ignore-errors intentionally permits failed entries in a playlist when at least one
        // item completed. A single item has no successful sibling to preserve.
        if !status.success() && !playlist {
            return Err(SoundSnatchError::Process {
                code: status.code(),
                message: diagnostic_message(&diagnostics.join("\n")),
            });
        }
        let warning = (!status.success()).then(|| {
            format!(
                "Some playlist items failed: {}",
                diagnostic_message(&diagnostics.join("\n"))
            )
        });
        Ok(DownloadResult {
            output_path,
            output_files,
            playlist,
            warning,
        })
    }

    pub async fn download_to_channel(
        &self,
        request: DownloadRequest,
        sender: mpsc::UnboundedSender<DownloadProgress>,
    ) -> Result<DownloadResult> {
        self.download(request, move |progress| {
            let _ = sender.send(progress);
        })
        .await
    }

    async fn run_output(&self, args: Vec<String>) -> Result<CapturedOutput> {
        let output = self.make_command(args).output().await?;
        Ok(CapturedOutput {
            status: output.status,
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }

    fn make_command(&self, args: Vec<String>) -> Command {
        let mut command = Command::new(self.command.program());
        command
            .args(self.command.prefix_args())
            .args(args)
            .kill_on_drop(true);
        command
    }
}

struct CapturedOutput {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

pub fn metadata_args(url: &str) -> Vec<String> {
    let mut args = vec![
        "-J".into(),
        "--flat-playlist".into(),
        "--no-warnings".into(),
        "--quiet".into(),
        "--ignore-errors".into(),
        "--ignore-config".into(),
        "--no-check-formats".into(),
        "--js-runtimes".into(),
        "node".into(),
    ];
    args.push(url.into());
    args
}

pub fn search_args(query: &str) -> Vec<String> {
    vec![
        format!("ytsearch5:{query}"),
        "-j".into(),
        "--no-warnings".into(),
        "--quiet".into(),
        "--flat-playlist".into(),
        "--ignore-config".into(),
    ]
}

pub fn download_args(request: &DownloadRequest) -> Result<Vec<String>> {
    validate_request(request)?;
    let playlist = is_playlist_url(&request.url);
    let output_path = request_output_path(request);
    let output = if playlist {
        output_path.join(PLAYLIST_OUTPUT_TEMPLATE)
    } else {
        PathBuf::from(format!(
            "{}{}",
            output_path.to_string_lossy(),
            SINGLE_OUTPUT_SUFFIX
        ))
    };
    let archive = if playlist {
        Some(output_path.join(".archive.txt"))
    } else {
        request.archive_path.clone()
    };
    let mut args = vec![
        "-f".into(),
        BEST_AUDIO_SELECTOR.into(),
        "--extract-audio".into(),
        "--audio-format".into(),
        request.format.as_str().into(),
        "--audio-quality".into(),
        "0".into(),
        "--no-check-formats".into(),
        "--js-runtimes".into(),
        "node".into(),
        "--embed-metadata".into(),
        "--no-warnings".into(),
        "--newline".into(),
        "--progress".into(),
        "--ignore-errors".into(),
        "--no-cache-dir".into(),
        "--lazy-playlist".into(),
        "--no-overwrites".into(),
        "--ignore-config".into(),
    ];
    if let Some(archive) = archive {
        args.extend([
            "--download-archive".into(),
            archive.to_string_lossy().into_owned(),
        ]);
    }
    args.extend(["--output".into(), output.to_string_lossy().into_owned()]);
    if !playlist {
        args.push("--no-playlist".into());
    }
    args.push(request.url.clone());
    Ok(args)
}

fn validate_request(request: &DownloadRequest) -> Result<()> {
    if request.url.trim().is_empty() {
        return Err(SoundSnatchError::InvalidRequest(
            "url cannot be empty".into(),
        ));
    }
    if !is_media_url(&request.url) {
        return Err(SoundSnatchError::InvalidRequest(
            "url must start with http:// or https://".into(),
        ));
    }
    if request.save_name.trim().is_empty() {
        return Err(SoundSnatchError::InvalidRequest(
            "save_name cannot be empty".into(),
        ));
    }
    Ok(())
}

fn request_output_path(request: &DownloadRequest) -> PathBuf {
    request.save_dir.join(sanitize_filename(&request.save_name))
}

fn collect_output_files(request: &DownloadRequest, output_path: &Path) -> Vec<PathBuf> {
    if is_playlist_url(&request.url) {
        let mut files = fs::read_dir(output_path)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(std::result::Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_file()
                    && path
                        .extension()
                        .and_then(|extension| extension.to_str())
                        .is_some_and(|extension| {
                            extension.eq_ignore_ascii_case(request.format.as_str())
                        })
            })
            .collect::<Vec<_>>();
        files.sort();
        files
    } else {
        let path = PathBuf::from(format!(
            "{}.{}",
            output_path.to_string_lossy(),
            request.format.as_str()
        ));
        path.is_file().then_some(path).into_iter().collect()
    }
}

pub fn is_playlist_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.contains("list=") || lower.contains("playlist")
}

pub fn is_media_url(url: &str) -> bool {
    let url = url.trim().to_ascii_lowercase();
    let remainder = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"));
    remainder.is_some_and(|remainder| {
        let host = remainder.split(['/', '?', '#']).next().unwrap_or("");
        !host.is_empty()
            && host.contains('.')
            && !host.chars().any(char::is_whitespace)
            && !remainder.chars().any(char::is_whitespace)
    })
}

async fn read_lines<R>(
    reader: R,
    stream: OutputStream,
    sender: mpsc::UnboundedSender<(OutputStream, String)>,
) -> std::io::Result<()>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut lines = BufReader::new(reader).lines();
    while let Some(line) = lines.next_line().await? {
        if sender.send((stream, line)).is_err() {
            break;
        }
    }
    Ok(())
}

fn process_error(status: ExitStatus, stderr: &str) -> SoundSnatchError {
    SoundSnatchError::Process {
        code: status.code(),
        message: diagnostic_message(stderr),
    }
}

fn diagnostic_message(stderr: &str) -> String {
    let message = stderr
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| line.starts_with("ERROR:"))
        .or_else(|| {
            stderr
                .lines()
                .rev()
                .map(str::trim)
                .find(|line| !line.is_empty())
        })
        .unwrap_or("")
        .trim_start_matches("ERROR:")
        .trim();
    if message.is_empty() {
        "no diagnostic output".into()
    } else if message.contains("Sign in to confirm") || message.contains("not a bot") {
        "The site requires authentication or bot verification for this media".into()
    } else if message.contains("Private video") {
        "This media is private and cannot be downloaded without access".into()
    } else if message.contains("Video unavailable") || message.contains("not available") {
        "This media is unavailable or restricted in your region".into()
    } else if message.contains("is not a valid URL") {
        "Enter a complete media URL, or use Search for titles and artists".into()
    } else if message.contains("Unsupported URL") {
        "This URL is not supported by yt-dlp".into()
    } else if message.contains("ffmpeg") && message.contains("not found") {
        "FFmpeg is required for audio conversion but was not found".into()
    } else {
        let mut message = message.to_string();
        if message.len() > 500 {
            message.truncate(497);
            message.push_str("...");
        }
        message
    }
}

pub async fn resolve_yt_dlp() -> Result<YtDlpCommand> {
    if let Some(path) = executable_on_path("yt-dlp") {
        return Ok(YtDlpCommand::Binary(path));
    }
    if let Some(python) = executable_on_path("python3") {
        let status = Command::new(&python)
            .args(["-m", "yt_dlp", "--version"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
        if status.is_ok_and(|status| status.success()) {
            return Ok(YtDlpCommand::PythonModule(python));
        }
    }
    Err(SoundSnatchError::NotInstalled)
}

fn executable_on_path(name: &str) -> Option<PathBuf> {
    env::var_os("PATH").and_then(|path| {
        env::split_paths(&path)
            .map(|directory| directory.join(name))
            .find(|candidate| is_executable(candidate))
    })
}

fn is_executable(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

pub fn parse_metadata(json: &str) -> Result<MediaMetadata> {
    let value: Value = serde_json::from_str(json)?;
    if !value.is_object() {
        return Err(SoundSnatchError::InvalidMetadata(
            "yt-dlp returned no usable metadata".into(),
        ));
    }
    let title = string_field(&value, "title").ok_or_else(|| {
        SoundSnatchError::InvalidMetadata("yt-dlp did not include a title".into())
    })?;
    let entries = value.get("entries").and_then(Value::as_array);
    Ok(MediaMetadata {
        id: string_field(&value, "id").unwrap_or_default(),
        title,
        artist: string_field(&value, "artist").or_else(|| string_field(&value, "creator")),
        album: string_field(&value, "album"),
        uploader: string_field(&value, "uploader").or_else(|| string_field(&value, "channel")),
        webpage_url: string_field(&value, "webpage_url")
            .or_else(|| string_field(&value, "original_url")),
        duration: number_field(&value, "duration"),
        playlist: value
            .get("_type")
            .and_then(Value::as_str)
            .is_some_and(|kind| kind == "playlist" || kind == "multi_video")
            || entries.is_some(),
        entry_count: entries.map(Vec::len).or_else(|| {
            value
                .get("playlist_count")
                .and_then(Value::as_u64)
                .map(|count| count as usize)
        }),
    })
}

pub fn parse_search_result(json: &str) -> Result<SearchResult> {
    let value: Value = serde_json::from_str(json)?;
    if !value.is_object() {
        return Err(SoundSnatchError::InvalidMetadata(
            "yt-dlp returned an invalid search result".into(),
        ));
    }
    let id = string_field(&value, "id").unwrap_or_default();
    let url = string_field(&value, "webpage_url")
        .or_else(|| {
            string_field(&value, "url")
                .filter(|url| url.starts_with("http://") || url.starts_with("https://"))
        })
        .or_else(|| (!id.is_empty()).then(|| format!("https://www.youtube.com/watch?v={id}")))
        .ok_or_else(|| {
            SoundSnatchError::InvalidMetadata("search result did not include a URL".into())
        })?;
    let title = string_field(&value, "title").ok_or_else(|| {
        SoundSnatchError::InvalidMetadata("search result did not include a title".into())
    })?;
    Ok(SearchResult {
        id,
        title,
        artist: string_field(&value, "artist").or_else(|| string_field(&value, "creator")),
        uploader: string_field(&value, "uploader").or_else(|| string_field(&value, "channel")),
        url,
        duration: number_field(&value, "duration"),
    })
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn number_field(value: &Value, key: &str) -> Option<f64> {
    value
        .get(key)
        .and_then(|value| value.as_f64().or_else(|| value.as_str()?.parse().ok()))
}

pub fn parse_progress_line(line: &str) -> Option<DownloadProgress> {
    parse_progress_line_from(line, OutputStream::Stderr)
}

fn parse_progress_line_from(line: &str, stream: OutputStream) -> Option<DownloadProgress> {
    if let Some((current_item, total_items)) = parse_playlist_position(line) {
        return Some(DownloadProgress {
            percent: None,
            current_item: Some(current_item),
            total_items: Some(total_items),
            speed: None,
            eta: None,
            raw: line.into(),
            stream,
        });
    }
    let body = line.trim().strip_prefix("[download]")?.trim();
    let percent = body
        .split_once('%')
        .and_then(|(prefix, _)| prefix.split_whitespace().last()?.parse::<f64>().ok())
        .map(|percent| (percent / 100.0).clamp(0.0, 1.0));
    if percent.is_none() && !body.contains(" ETA ") && !body.contains(" at ") {
        return None;
    }
    Some(DownloadProgress {
        percent,
        current_item: None,
        total_items: None,
        speed: value_after(body, " at "),
        eta: value_after(body, " ETA "),
        raw: line.into(),
        stream,
    })
}

fn parse_playlist_position(line: &str) -> Option<(usize, usize)> {
    let body = line.trim().strip_prefix("[download]")?.trim();
    let rest = body
        .strip_prefix("Downloading item ")
        .or_else(|| body.strip_prefix("Downloading video "))?;
    let (current, total) = rest.split_once(" of ")?;
    let current = current.trim().parse().ok()?;
    let total = total.split_whitespace().next()?.parse().ok()?;
    Some((current, total))
}

fn value_after(value: &str, marker: &str) -> Option<String> {
    Some(
        value
            .split_once(marker)?
            .1
            .split_whitespace()
            .next()?
            .into(),
    )
}

pub fn sanitize_filename(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut previous_space = false;
    for character in value.chars() {
        if character.is_control() {
            continue;
        }
        let character = if matches!(
            character,
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
        ) {
            '_'
        } else {
            character
        };
        if character.is_whitespace() {
            if !previous_space {
                result.push(' ');
            }
            previous_space = true;
        } else {
            result.push(character);
            previous_space = false;
        }
    }
    let sanitized = result.trim().trim_end_matches(['.', ' ']).to_owned();
    let stem = sanitized
        .split('.')
        .next()
        .unwrap_or("")
        .to_ascii_uppercase();
    let reserved = matches!(
        stem.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    );
    if sanitized.is_empty() {
        "untitled".into()
    } else if reserved {
        format!("_{sanitized}")
    } else {
        sanitized
    }
}

fn settings_yaml(settings: &SoundSnatchSettings) -> String {
    format!(
        "last_save_dir: {}\ndefault_format: {}\narchive_path: {}\n",
        yaml_string(&settings.last_save_dir.to_string_lossy()),
        settings.default_format,
        yaml_string(&settings.archive_path.to_string_lossy()),
    )
}

fn yaml_string(value: &str) -> String {
    serde_json::to_string(value).expect("serializing a string cannot fail")
}

fn parse_settings_yaml(contents: &str) -> Result<SoundSnatchSettings> {
    let mut settings = SoundSnatchSettings::default();
    for (index, raw_line) in contents.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, raw_value) = line.split_once(':').ok_or_else(|| {
            SoundSnatchError::Settings(format!("invalid YAML on line {}", index + 1))
        })?;
        let value = parse_yaml_scalar(raw_value.trim()).map_err(|message| {
            SoundSnatchError::Settings(format!("line {}: {message}", index + 1))
        })?;
        match key.trim() {
            "last_save_dir" => settings.last_save_dir = PathBuf::from(value),
            "default_format" => settings.default_format = value.parse()?,
            // Ignore the removed setting so previously saved files still load.
            "browser" => {}
            "archive_path" => settings.archive_path = PathBuf::from(value),
            unknown => {
                return Err(SoundSnatchError::Settings(format!(
                    "unknown key `{unknown}` on line {}",
                    index + 1
                )));
            }
        }
    }
    Ok(settings)
}

fn parse_yaml_scalar(value: &str) -> std::result::Result<String, String> {
    if value.starts_with('"') {
        serde_json::from_str(value).map_err(|error| error.to_string())
    } else if value.starts_with('\'') {
        if value.len() < 2 || !value.ends_with('\'') {
            return Err("unterminated single-quoted value".into());
        }
        Ok(value[1..value.len() - 1].replace("''", "'"))
    } else {
        Ok(value.split(" #").next().unwrap_or("").trim().to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn request(url: &str) -> DownloadRequest {
        DownloadRequest {
            url: url.into(),
            save_dir: "/music".into(),
            save_name: "My: Mix".into(),
            format: AudioFormat::Flac,
            archive_path: Some("/state/global.archive".into()),
        }
    }

    #[test]
    fn audio_formats_are_exact() {
        assert_eq!(AudioFormat::Mp3.as_str(), "mp3");
        assert_eq!(AudioFormat::Flac.as_str(), "flac");
        assert_eq!(AudioFormat::Wav.as_str(), "wav");
        assert!("opus".parse::<AudioFormat>().is_err());
    }

    #[test]
    fn metadata_arguments_are_exact() {
        assert_eq!(
            metadata_args("https://example.test/video"),
            vec![
                "-J",
                "--flat-playlist",
                "--no-warnings",
                "--quiet",
                "--ignore-errors",
                "--ignore-config",
                "--no-check-formats",
                "--js-runtimes",
                "node",
                "https://example.test/video",
            ]
        );
    }

    #[test]
    fn search_arguments_are_exact() {
        assert_eq!(
            search_args("lo fi"),
            vec![
                "ytsearch5:lo fi",
                "-j",
                "--no-warnings",
                "--quiet",
                "--flat-playlist",
                "--ignore-config",
            ]
        );
    }

    #[test]
    fn single_download_arguments_are_exact() {
        assert_eq!(
            download_args(&request("https://example.test/watch?v=1")).unwrap(),
            vec![
                "-f",
                "bestaudio[ext=m4a]/bestaudio[ext=webm]/bestaudio/best",
                "--extract-audio",
                "--audio-format",
                "flac",
                "--audio-quality",
                "0",
                "--no-check-formats",
                "--js-runtimes",
                "node",
                "--embed-metadata",
                "--no-warnings",
                "--newline",
                "--progress",
                "--ignore-errors",
                "--no-cache-dir",
                "--lazy-playlist",
                "--no-overwrites",
                "--ignore-config",
                "--download-archive",
                "/state/global.archive",
                "--output",
                "/music/My_ Mix.%(ext)s",
                "--no-playlist",
                "https://example.test/watch?v=1",
            ]
        );
    }

    #[test]
    fn playlist_uses_named_folder_and_local_archive() {
        assert_eq!(
            download_args(&request("https://example.test/playlist?list=abc")).unwrap(),
            vec![
                "-f",
                "bestaudio[ext=m4a]/bestaudio[ext=webm]/bestaudio/best",
                "--extract-audio",
                "--audio-format",
                "flac",
                "--audio-quality",
                "0",
                "--no-check-formats",
                "--js-runtimes",
                "node",
                "--embed-metadata",
                "--no-warnings",
                "--newline",
                "--progress",
                "--ignore-errors",
                "--no-cache-dir",
                "--lazy-playlist",
                "--no-overwrites",
                "--ignore-config",
                "--download-archive",
                "/music/My_ Mix/.archive.txt",
                "--output",
                "/music/My_ Mix/%(title)s.%(ext)s",
                "https://example.test/playlist?list=abc",
            ]
        );
    }

    #[test]
    fn playlist_detection_matches_contract() {
        assert!(is_playlist_url("https://x.test/watch?v=1&list=abc"));
        assert!(is_playlist_url("https://x.test/PLAYLIST/abc"));
        assert!(!is_playlist_url("https://x.test/watch?v=1"));
    }

    #[test]
    fn media_urls_require_an_http_scheme() {
        assert!(is_media_url("https://example.test/video"));
        assert!(is_media_url(" HTTP://example.test/video "));
        assert!(!is_media_url("https://"));
        assert!(!is_media_url("https://not a host/video"));
        assert!(!is_media_url("song title and artist"));
        assert!(download_args(&request("song title and artist")).is_err());
    }

    #[test]
    fn output_discovery_preserves_dots_and_ignores_unrelated_files() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = env::temp_dir().join(format!("soundsnatch-output-{unique}"));
        fs::create_dir_all(&directory).unwrap();

        let mut single = request("https://example.test/watch?v=1");
        single.save_dir = directory.clone();
        single.save_name = "mix.v1".into();
        let output_path = request_output_path(&single);
        let actual = directory.join("mix.v1.flac");
        fs::write(&actual, b"audio").unwrap();
        fs::write(directory.join("unrelated.flac"), b"audio").unwrap();
        assert_eq!(collect_output_files(&single, &output_path), [actual]);

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn diagnostics_are_short_and_actionable() {
        assert_eq!(
            diagnostic_message("WARNING: ignored\nERROR: [generic] 'x' is not a valid URL"),
            "Enter a complete media URL, or use Search for titles and artists"
        );
        assert_eq!(
            diagnostic_message("ERROR: Sign in to confirm you’re not a bot"),
            "The site requires authentication or bot verification for this media"
        );
    }

    #[test]
    fn progress_is_normalized_and_playlist_positions_parse() {
        let progress =
            parse_progress_line("[download]  42.5% of 10.00MiB at 2.50MiB/s ETA 00:03").unwrap();
        assert_eq!(progress.percent, Some(0.425));
        assert_eq!(progress.speed.as_deref(), Some("2.50MiB/s"));
        assert_eq!(progress.eta.as_deref(), Some("00:03"));
        let item = parse_progress_line("[download] Downloading item 3 of 12").unwrap();
        assert_eq!(item.percent, None);
        assert_eq!(item.current_item, Some(3));
        assert_eq!(item.total_items, Some(12));
        assert!(parse_progress_line("ERROR: unavailable").is_none());
    }

    #[test]
    fn settings_yaml_round_trips_escaped_values() {
        let settings = SoundSnatchSettings {
            last_save_dir: PathBuf::from("/Music/a: b"),
            default_format: AudioFormat::Wav,
            archive_path: PathBuf::from("/tmp/a # archive"),
        };
        let yaml = settings_yaml(&settings);
        assert_eq!(parse_settings_yaml(&yaml).unwrap(), settings);
    }

    #[test]
    fn settings_load_and_save_without_yaml_dependency() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = env::temp_dir().join(format!("soundsnatch-settings-{unique}.yaml"));
        let settings = SoundSnatchSettings::default();
        settings.save_to(&path).unwrap();
        assert_eq!(SoundSnatchSettings::load_from(&path).unwrap(), settings);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn legacy_browser_setting_is_ignored() {
        let settings = parse_settings_yaml(
            "last_save_dir: /Music\ndefault_format: mp3\nbrowser: firefox\narchive_path: /tmp/archive\n",
        )
        .unwrap();
        assert_eq!(settings.last_save_dir, PathBuf::from("/Music"));
        assert_eq!(settings.default_format, AudioFormat::Mp3);
        assert_eq!(settings.archive_path, PathBuf::from("/tmp/archive"));
    }

    #[test]
    fn metadata_and_search_json_parse() {
        let metadata =
            parse_metadata(r#"{"_type":"playlist","id":"p","title":"Mix","entries":[{},{}]}"#)
                .unwrap();
        assert!(metadata.playlist);
        assert_eq!(metadata.entry_count, Some(2));
        let result = parse_search_result(
            r#"{"id":"xyz","title":"Track","channel":"Channel","duration":"42"}"#,
        )
        .unwrap();
        assert_eq!(result.url, "https://www.youtube.com/watch?v=xyz");
        assert_eq!(result.duration, Some(42.0));
    }

    #[test]
    fn empty_yt_dlp_metadata_is_rejected() {
        assert!(matches!(
            parse_metadata("null"),
            Err(SoundSnatchError::InvalidMetadata(_))
        ));
        assert!(matches!(
            parse_metadata("{}"),
            Err(SoundSnatchError::InvalidMetadata(_))
        ));
        assert!(matches!(
            parse_search_result("{}"),
            Err(SoundSnatchError::InvalidMetadata(_))
        ));
    }

    #[test]
    fn python_fallback_prefix_is_shell_free() {
        let command = YtDlpCommand::PythonModule("/usr/bin/python3".into());
        assert_eq!(command.program(), Path::new("/usr/bin/python3"));
        assert_eq!(command.prefix_args(), &["-m", "yt_dlp"]);
    }
}
