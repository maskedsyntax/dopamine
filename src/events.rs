#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApplicationEvent {
    ScanStarted,
    ScanProgress(usize, usize),
    ScanFinished,
    MediaPlayPause,
    MediaNext,
    MediaPrevious,
    LyricsFetched(String, String),
}

pub type Message = ApplicationEvent;
