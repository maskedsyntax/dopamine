use serde::{Serialize, Deserialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub year: i32,
    pub favorite: bool,
    pub play_count: i32,
    pub last_played: Option<i64>,
    pub duration_secs: i64,
    pub lyrics: Option<String>,
}
