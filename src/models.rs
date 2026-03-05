#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Track {
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_secs: i64,
}
